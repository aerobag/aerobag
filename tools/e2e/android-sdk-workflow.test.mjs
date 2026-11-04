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
      if (step.run?.includes("sdkmanager") || step.run?.includes("install_android_sdk.py")) {
        test(`${name}: ${jobId} step ${index + 1} uses bounded SDK dependency installation`, () => {
          assert.doesNotMatch(step.run, /^\s*sdkmanager\s/m, "package downloads use the shared installer");
          assert.match(step.run, /python3 tools\/ci\/install_android_sdk\.py/);
        });
      }
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

const e2e = parse(readFileSync(new URL("e2e-ci.yml", workflows), "utf8"));
for (const jobId of ["release-journey-android-baseline", "release-journey-android", "android-native"]) {
  const job = e2e.jobs[jobId];
  test(`${jobId}: immutable APK consumers do not install or cache Android build tools`, () => {
    const commands = job.steps.map((step) => step.run ?? "").join("\n");
    const sdkPackages = job.steps.map((step) => step.with?.packages ?? "").join("\n");
    assert.doesNotMatch(`${commands}\n${sdkPackages}`, /ndk;|build-tools;|platforms;android-|rustup target add[^\n]*android/,
      "APK consumers need only platform-tools, emulator and their system image; native toolchains belong to the build job");
    assert.doesNotMatch(commands, /keytool|gradlew|build_release_e2e_apps\.sh/,
      "APK consumers must not sign or rebuild the immutable apps");
    assert.ok(!Object.keys(job.env ?? {}).some((name) => /KEYSTORE|KEY_PASSWORD|KEY_ALIAS|ANDROID_TARGET_ABIS/.test(name)),
      "APK consumers must not retain signing/build configuration");
    for (const step of job.steps.filter((step) => step.uses?.startsWith("actions/cache@"))) {
      assert.doesNotMatch(step.with?.path ?? "", /gradle|rust-target/,
        "APK consumers must not restore large app-build caches");
    }
    assert.ok(job.steps.some((step) => step.uses?.startsWith("actions/download-artifact@")
      && step.with?.name === "release-e2e-apps-${{ github.sha }}"));
    const verifyIndex = job.steps.findIndex((step) => step.run?.includes("python3 tools/ci/verify_release_e2e_apps.py"));
    const bootIndex = job.steps.findIndex((step) => /run_e2e_ci\.sh|android-boot-install/.test(step.run ?? ""));
    assert.ok(verifyIndex >= 0 && bootIndex > verifyIndex,
      "verify immutable app inputs before the emulator is booted");
  });
}

test("native Android smoke lane supplies both immutable APKs and keeps the host fixture compiler", () => {
  const job = e2e.jobs["android-native"];
  const run = job.steps.find((step) => step.run?.includes("./ui/android-app/scripts/run_e2e_ci.sh"));
  assert.equal(run.env.AEROBAG_E2E_APK, "${{ github.workspace }}/.ci/release-e2e-apps/aerobag-release-e2e.apk");
  assert.equal(run.env.AEROBAG_E2E_DRIVER_APK, "${{ github.workspace }}/.ci/release-e2e-apps/aerobag-e2e-driver.apk");
  assert.ok(job.steps.some((step) => step.run?.includes('rustup toolchain install "$RUST_TOOLCHAIN" --profile minimal')),
    "the package-only server still compiles service-bulletin-contract for the host");
});
