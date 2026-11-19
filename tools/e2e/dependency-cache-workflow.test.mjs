// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { readFileSync, readdirSync } from "node:fs";
import test from "node:test";
import { requireWebDependency } from "../../ui/web-app/scripts/web-workspace-require.mjs";

const { parse } = requireWebDependency("yaml");
const root = new URL("../../", import.meta.url);
const readYaml = (path) => parse(readFileSync(new URL(path, root), "utf8"));
const workflows = readdirSync(new URL(".github/workflows/", root))
  .filter((name) => /\.ya?ml$/.test(name))
  .map((name) => [name, readYaml(`.github/workflows/${name}`)]);
const cargoAction = "./.github/actions/cache-cargo-downloads";
const gradleAction = "./.github/actions/cache-gradle-downloads";
const cacheSteps = (steps) => steps.filter((step) => step.uses === "actions/cache@v4");

for (const [file, workflow] of workflows) {
  for (const [jobId, job] of Object.entries(workflow.jobs ?? {})) {
    for (const step of cacheSteps(job.steps ?? [])) {
      test(`${file}/${jobId}: ${step.name} does not mix downloads with compiled outputs`, () => {
        assert.doesNotMatch(step.with.path, /~\/\.cargo\/(registry|git)|gradle-user-home\/(caches|wrapper)/,
          "dependency downloads belong to the shared actions, not lane-specific output caches");
        assert.ok(!step.with["restore-keys"], "compiled outputs retain exact-key isolation");
      });
    }
  }
}

test("all existing compiler lanes use the common download cache before building", () => {
  const consumers = {
    "ci.yml": ["rust-shared", "rust-core", "rust-services", "rust-preprocessor", "web"],
    "fixture-ci.yml": ["metar"],
    "heavy-fixture-ci.yml": ["nms-notams", "nexrad"],
    "e2e-ci.yml": ["build-release-apps", "web-nav-db-rollover"],
  };
  for (const [file, jobs] of Object.entries(consumers)) {
    const workflow = workflows.find(([name]) => name === file)[1];
    for (const id of jobs) {
      const steps = workflow.jobs[id]?.steps;
      assert.ok(steps, `missing ${file}/${id}; update the cache ownership inventory`);
      const index = steps.findIndex((step) => step.uses === cargoAction);
      assert.ok(index >= 0, `${file}/${id} must restore Cargo downloads`);
      const output = steps.findIndex((step) => step.uses === "actions/cache@v4");
      assert.ok(output > index, `${file}/${id} restores downloads separately before build outputs`);
    }
  }
  for (const [file, id] of [["ci.yml", "android-jvm"], ["e2e-ci.yml", "build-release-apps"]]) {
    const steps = workflows.find(([name]) => name === file)[1].jobs[id].steps;
    const cache = steps.findIndex((step) => step.uses === gradleAction);
    assert.ok(cache >= 0, `${file}/${id} must share Gradle download policy`);
    assert.equal(steps[cache].with["gradle-user-home"], "${{ env.AEROBAG_UI_TARGET_ROOT }}/android/gradle-user-home");
    const build = steps.findIndex((step) => /build_release_e2e_apps\.sh|scripts\/test\.sh/.test(step.run ?? ""));
    assert.ok(build > cache, `${file}/${id} restores downloads before invoking Gradle`);
  }
});

test("runtime-only consumers never restore compiler dependency caches", () => {
  const workflow = readYaml(".github/workflows/e2e-ci.yml");
  for (const job of Object.values(workflow.jobs)) {
    if (!job.steps?.some((step) => step.uses === "./.github/actions/setup-e2e-android")) continue;
    assert.ok(!job.steps.some((step) => [cargoAction, gradleAction].includes(step.uses)));
  }
});

function actionCache(action) {
  const document = readYaml(`${action}/action.yml`);
  assert.equal(document.runs.using, "composite");
  assert.equal(document.runs.steps.length, 1, "cache restoration must not introduce build retries or extra commands");
  const [step] = document.runs.steps;
  assert.equal(step.uses, "actions/cache@v4");
  assert.ok(!step.if, "restoring downloads is not conditional on compiled-output cache hits");
  return step.with;
}

// Evaluate only the key expression vocabulary used by these actions. Hash input
// changes model independent manifests; unsupported expressions fail the audit.
function expand(template, changes = {}) {
  const context = { "runner.os": "Linux", "runner.arch": "X64", "github.job": "build-release-apps",
    "env.RUST_TOOLCHAIN": "1.94.1", ...changes };
  return template.replace(/\$\{\{\s*(.*?)\s*\}\}/g, (_, expression) => {
    if (Object.hasOwn(context, expression)) return context[expression];
    assert.match(expression, /^hashFiles\('[^']+'(?:, '[^']+')*\)$/);
    const patterns = [...expression.matchAll(/'([^']+)'/g)].map((match) => match[1]);
    return createHash("sha256").update(patterns.map((pattern) => context[pattern] ?? pattern).join("\n")).digest("hex");
  });
}

function assertReuse(cache, changes, compatible) {
  const oldKey = expand(cache.key);
  const newKey = expand(cache.key, changes);
  assert.notEqual(newKey, oldKey, "changed inputs must produce a new, writable cache entry");
  const prefixes = expand(cache["restore-keys"], changes).trim().split(/\s+/);
  assert.equal(prefixes.some((prefix) => oldKey.startsWith(prefix)), compatible);
}

test("Gradle caches only repository modules and wrapper distributions, never the whole user home", () => {
  const cache = actionCache(gradleAction);
  assert.deepEqual(cache.path.trim().split("\n"), [
    "${{ inputs.gradle-user-home }}/caches/modules-2",
    "!${{ inputs.gradle-user-home }}/caches/modules-2/modules-2.lock",
    "!${{ inputs.gradle-user-home }}/caches/modules-2/gc.properties",
    "${{ inputs.gradle-user-home }}/wrapper/dists",
  ]);
  assertReuse(cache, { "ui/android-app/**/*.gradle*": "changed build configuration" }, true);
  assertReuse(cache, { "github.job": "android-jvm" }, true);
  assertReuse(cache, { "ui/android-app/gradle/wrapper/gradle-wrapper.properties": "new Gradle" }, false);
  assert.equal(expand(cache.key), expand(cache.key, {
    "ui/core-rust/Cargo.lock": "new Rust dependencies", "ui/web-app/package-lock.json": "new npm dependencies",
    "env.RUST_TOOLCHAIN": "new Rust compiler",
  }), "Rust/npm changes must not invalidate Gradle downloads");
});

test("Cargo source caches can seed changed lockfiles without restoring extracted sources or binaries", () => {
  const cache = actionCache(cargoAction);
  assert.deepEqual(cache.path.trim().split("\n"), ["~/.cargo/registry/index", "~/.cargo/registry/cache", "~/.cargo/git/db"]);
  for (const lock of ["crates/Cargo.lock", "ui/core-rust/Cargo.lock", "services/Cargo.lock", "product/preprocessor/Cargo.lock"]) {
    assertReuse(cache, { [lock]: "changed dependencies" }, true);
  }
  assertReuse(cache, { "github.job": "rust-core" }, true);
  assertReuse(cache, { "env.RUST_TOOLCHAIN": "new cache format" }, false);
  assert.equal(expand(cache.key), expand(cache.key, {
    "ui/android-app/**/*.gradle*": "new Gradle dependencies", "ui/web-app/package-lock.json": "new npm dependencies",
  }));
});

test("download cache compatibility stays within runner OS and architecture", () => {
  for (const action of [cargoAction, gradleAction]) {
    const cache = actionCache(action);
    assertReuse(cache, { "runner.os": "Windows" }, false);
    assertReuse(cache, { "runner.arch": "ARM64" }, false);
    assert.ok(!cache["save-always"], "failed builds must not publish incomplete or negative-resolution caches");
  }
});
