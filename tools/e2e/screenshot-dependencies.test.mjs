// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import { requireWebDependency } from "../../ui/web-app/scripts/web-workspace-require.mjs";

const { parse } = requireWebDependency("yaml");
const workflow = parse(readFileSync(new URL("../../.github/workflows/e2e-ci.yml", import.meta.url), "utf8"));
const setupAction = "./.github/actions/setup-e2e-python";

for (const jobId of ["release-journey-android", "android-native"]) {
  test(`${jobId}: screenshot analysis does not depend on unrelated system package repositories`, () => {
    const steps = workflow.jobs[jobId].steps;
    assert.doesNotMatch(steps.map((step) => step.run ?? "").join("\n"), /\bapt(?:-get)?\b/);
    const installers = steps.filter((step) => step.uses === setupAction);
    assert.equal(installers.length, 1, "install screenshot dependencies explicitly through shared setup");
    assert.equal(installers[0]["continue-on-error"], undefined);
    const consumer = steps.findIndex((step) => /Run Android shard|Run \$\{\{ matrix.test/.test(step.name ?? ""));
    assert.ok(consumer > steps.indexOf(installers[0]), "install before running the consumer");
    if (jobId === "release-journey-android") assert.equal(installers[0].if, undefined);
    else assert.equal(installers[0].if, "${{ matrix.test == 'android.plate-first-render-smoke' }}");
  });
}

test("shared screenshot dependency setup pins Python and Pillow and verifies the interpreter", () => {
  const action = parse(readFileSync(new URL(`../../${setupAction}/action.yml`, import.meta.url), "utf8"));
  assert.equal(action.runs.using, "composite");
  const [python, install] = action.runs.steps;
  assert.match(python.uses, /^actions\/setup-python@/);
  assert.equal(python.with["python-version"], "3.12");
  assert.equal(python.with.cache, "pip");
  const requirements = "tools/e2e/screenshot-requirements.txt";
  assert.equal(python.with["cache-dependency-path"], requirements);
  const packages = readFileSync(new URL(`../../${requirements}`, import.meta.url), "utf8")
    .split("\n").map((line) => line.trim()).filter((line) => line && !line.startsWith("#"));
  assert.equal(packages.length, 1);
  assert.match(packages[0], /^Pillow==\d+\.\d+\.\d+$/);
  assert.match(install.run, /python3 -m pip install --only-binary=:all: --requirement tools\/e2e\/screenshot-requirements\.txt/);
  assert.match(install.run, /python3 -c .*from PIL import Image/);
  assert.doesNotMatch(install.run, /\bapt(?:-get)?\b|\|\|\s*true/);
  assert.equal(install["continue-on-error"], undefined);
});
