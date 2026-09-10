// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import assert from "node:assert/strict";
import { copyFileSync, existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { spawnSync } from "node:child_process";
import test from "node:test";
import { fileURLToPath } from "node:url";

const repoRoot = fileURLToPath(new URL("../..", import.meta.url));
const runner = join(repoRoot, "ui/web-app/scripts/run-target-workspace.sh");
const harnessScript = JSON.parse(readFileSync(join(repoRoot, "ui/web-app/package.json"), "utf8"))
  .scripts["inner:test:harness"];

function run(command, args, cwd, env) {
  const result = spawnSync(command, args, { cwd, env, encoding: "utf8", timeout: 20_000 });
  assert.ifError(result.error);
  return { status: result.status, output: `${result.stdout}\n${result.stderr}` };
}

test("harness bootstraps cold offline dependencies without source node_modules or workspace environment", () => {
  const temporary = mkdtempSync(join(tmpdir(), "aerobag-harness-bootstrap-"));
  try {
    const root = join(temporary, "repo with spaces");
    const source = join(root, "ui/web-app");
    const tests = join(root, "tools/e2e");
    const dependency = join(temporary, "local-dependency");
    for (const directory of [join(source, "scripts"), tests, dependency]) {
      mkdirSync(directory, { recursive: true });
    }
    const environment = {
      PATH: process.env.PATH,
      AEROBAG_REPO_ROOT: root,
      npm_config_cache: join(temporary, "npm-cache"),
      npm_config_offline: "true",
      npm_config_audit: "false",
      npm_config_fund: "false",
    };
    // Use real npm ci, but no registry, existing cache, browser, or app build.
    // Only this tiny test dependency is synthetic; the entrypoint and module
    // resolver are the same ones used by every CI/preflight harness invocation.
    writeFileSync(join(dependency, "package.json"), JSON.stringify({
      name: "harness-bootstrap-probe", version: "1.0.0", main: "index.cjs",
    }));
    writeFileSync(join(dependency, "index.cjs"), 'module.exports = "locked local dependency";\n');
    const packed = run("npm", ["pack", "--ignore-scripts", "--pack-destination", temporary], dependency, environment);
    assert.equal(packed.status, 0, packed.output);
    writeFileSync(join(source, "package.json"), JSON.stringify({
      name: "harness-bootstrap-fixture", version: "1.0.0", private: true, type: "module",
      scripts: { "inner:test:harness": harnessScript },
      devDependencies: { "harness-bootstrap-probe": `file:${join(temporary, "harness-bootstrap-probe-1.0.0.tgz")}` },
    }));
    writeFileSync(join(root, "ui/target-root.txt"), "../target\n");
    for (const name of ["tsconfig.json", "vite.config.ts", "index.html", "about.html"]) {
      writeFileSync(join(source, name), "");
    }
    copyFileSync(
      join(repoRoot, "ui/web-app/scripts/web-workspace-require.mjs"),
      join(source, "scripts/web-workspace-require.mjs"),
    );
    writeFileSync(join(tests, "a.test.mjs"), `
import assert from "node:assert/strict";
import test from "node:test";
import { requireWebDependency } from "../../ui/web-app/scripts/web-workspace-require.mjs";
test("first selected harness contract", () => {
  assert.equal(requireWebDependency("harness-bootstrap-probe"), "locked local dependency");
  assert.equal(process.env.AEROBAG_WEB_WORKSPACE_DIR, process.cwd());
});
`);
    writeFileSync(join(tests, "b.test.mjs"), `
import test from "node:test";
test("second selected harness contract", () => {});
`);
    writeFileSync(join(tests, "not-a-test.mjs"), 'throw new Error("not a harness test");\n');
    const locked = run("npm", ["install", "--package-lock-only", "--ignore-scripts"], source, environment);
    assert.equal(locked.status, 0, locked.output);
    assert.equal(existsSync(join(source, "node_modules")), false);

    const cold = run(runner, ["inner:test:harness"], root, environment);
    assert.equal(cold.status, 0, cold.output);
    assert.match(cold.output, /first selected harness contract/);
    assert.match(cold.output, /second selected harness contract/);
    assert.match(cold.output, /# tests 2\b/);
    assert.equal(existsSync(join(source, "node_modules")), false);
    assert.equal(existsSync(join(temporary, "target/web/workspace/node_modules/harness-bootstrap-probe")), true);

    // Even a poisoned source checkout cannot supply the dependency when the
    // caller explicitly chooses a different, initially empty workspace.
    const poison = join(source, "node_modules/harness-bootstrap-probe");
    mkdirSync(poison, { recursive: true });
    writeFileSync(join(poison, "index.js"), 'throw new Error("source dependency leaked");\n');
    const explicitEnvironment = {
      ...environment, AEROBAG_WEB_WORKSPACE_DIR: join(temporary, "explicit workspace"),
    };
    const explicit = run(runner, ["inner:test:harness"], root, explicitEnvironment);
    assert.equal(explicit.status, 0, explicit.output);
    assert.match(explicit.output, /# tests 2\b/);
    assert.equal(existsSync(join(explicitEnvironment.AEROBAG_WEB_WORKSPACE_DIR, "node_modules/harness-bootstrap-probe")), true);

    // Preparation must not turn a failing selected contract into a green lane.
    writeFileSync(join(tests, "b.test.mjs"), 'throw new Error("intentional harness assertion");\n');
    const failed = run(runner, ["inner:test:harness"], root, explicitEnvironment);
    assert.notEqual(failed.status, 0, failed.output);
    assert.match(failed.output, /intentional harness assertion/);
  } finally {
    rmSync(temporary, { recursive: true, force: true });
  }
});
