// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import assert from "node:assert/strict";
import { readFileSync, readdirSync } from "node:fs";
import test from "node:test";
import { requireWebDependency } from "../../ui/web-app/scripts/web-workspace-require.mjs";

const { parse } = requireWebDependency("yaml");

function assertModernSdkPackages(step) {
  const value = step.with?.packages;
  assert.equal(typeof value, "string", "SDK setup must explicitly override the action's obsolete package defaults");
  const packages = value.trim().split(/\s+/);
  assert.ok(packages.includes("platform-tools"), "SDK setup must install platform-tools");
  assert.ok(!packages.includes("tools"), "SDK setup must not request the obsolete tools package");
}

test("Android SDK package policy rejects implicit and obsolete dependencies", () => {
  for (const step of [{}, { with: {} }, { with: { packages: "" } },
    { with: { packages: "tools platform-tools" } }]) {
    assert.throws(() => assertModernSdkPackages(step), assert.AssertionError);
  }
  assert.doesNotThrow(() => assertModernSdkPackages({ with: { packages: "platform-tools" } }));
  assert.doesNotThrow(() => assertModernSdkPackages({
    with: { packages: "platform-tools platforms;android-34 build-tools;34.0.0" },
  }));
});

const workflows = new URL("../../.github/workflows/", import.meta.url);
let setupSteps = 0;
for (const name of readdirSync(workflows).filter((name) => /\.ya?ml$/.test(name)).sort()) {
  const workflow = parse(readFileSync(new URL(name, workflows), "utf8"));
  for (const [jobId, job] of Object.entries(workflow.jobs ?? {})) {
    for (const [index, step] of (job.steps ?? []).entries()) {
      if (!step.uses?.startsWith("android-actions/setup-android@")) continue;
      setupSteps += 1;
      test(`${name}: ${jobId} step ${index + 1} explicitly installs supported Android SDK packages`, () => {
        assertModernSdkPackages(step);
      });
    }
  }
}
test("Android SDK workflow audit finds setup steps", () => {
  assert.ok(setupSteps > 0, "No SDK setup steps audited; update the audit if setup moved");
});
