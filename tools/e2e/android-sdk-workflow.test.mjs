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
const runtimeAction = parse(readFileSync(new URL("../../.github/actions/setup-e2e-android/action.yml", import.meta.url), "utf8"));
const documents = readdirSync(workflows).filter((name) => /\.ya?ml$/.test(name)).sort()
  .map((name) => [name, parse(readFileSync(new URL(name, workflows), "utf8"))]);
documents.push(["setup-e2e-android/action.yml", { jobs: { runtime: { steps: runtimeAction.runs.steps } } }]);
let setupSteps = 0;
for (const [name, workflow] of documents) {
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
const emulatorConsumers = ["release-journey-android-baseline", "release-journey-android", "android-native", "android-chrome-live-feed"];
for (const jobId of emulatorConsumers) {
  const job = e2e.jobs[jobId];
  test(`${jobId}: immutable app consumers do not install or cache Android build tools`, () => {
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
    const bootIndex = job.steps.findIndex((step) => /run_e2e_ci\.sh|android-boot-install|run_android_chrome_livefeed_e2e\.sh/.test(step.run ?? ""));
    assert.ok(verifyIndex >= 0 && bootIndex > verifyIndex,
      "verify immutable app inputs before the emulator is booted");
  });
}

test("emulator consumers share runtime setup and disk accounting, never the APK builder", () => {
  for (const jobId of emulatorConsumers) {
    const steps = e2e.jobs[jobId].steps;
    const runtimeIndex = steps.findIndex((step) => step.uses === "./.github/actions/setup-e2e-android");
    assert.ok(runtimeIndex >= 0, `${jobId} must use the shared runtime-only SDK setup`);
    const image = steps[runtimeIndex].with?.["system-image"];
    assert.match(image === "${{ env.AVD_PACKAGE_PATH }}" ? e2e.jobs[jobId].env.AVD_PACKAGE_PATH : image ?? "", /system-images;android-34;/);
    const bootIndex = steps.findIndex((step) => /run_e2e_ci\.sh|android-boot-install|run_android_chrome_livefeed_e2e\.sh/.test(step.run ?? ""));
    assert.ok(runtimeIndex < bootIndex);
    assert.ok(steps.some((step) => step.if === "${{ always() }}"
      && step.run?.includes("tools/ci/emulator_disk.py --phase after-test")));
  }
  assert.ok(!e2e.jobs["build-release-apps"].steps.some((step) => step.uses === "./.github/actions/setup-e2e-android"),
    "the app builder needs the compiler SDKs that runtime-only setup removes");
});

test("shared emulator setup prunes only hosted SDK compilers and reports installed runtime footprint", () => {
  const steps = runtimeAction.runs.steps;
  const cleanupIndex = steps.findIndex((step) => step.run?.includes("--clean-unused-sdk"));
  const installIndex = steps.findIndex((step) => step.run?.includes("install_android_sdk.py"));
  assert.ok(cleanupIndex >= 0 && installIndex > cleanupIndex);
  assert.equal(steps[cleanupIndex].if, "${{ runner.environment == 'github-hosted' && runner.os == 'Linux' }}");
  assert.match(steps[cleanupIndex].run, /tools\/ci\/emulator_disk\.py --phase before-sdk-install --clean-unused-sdk/);
  assert.match(steps[installIndex].run, /"platform-tools" "emulator" "\$SYSTEM_IMAGE"/);
  assert.equal(steps[installIndex].env.SYSTEM_IMAGE, "${{ inputs.system-image }}");
  assert.ok(steps.findIndex((step) => step.run?.includes("--phase after-sdk-install")) > installIndex);
  assert.doesNotMatch(steps.map((step) => step.run ?? "").join("\n"), /ndk;|build-tools;|platforms;android-/);
  const launcher = readFileSync(new URL("../../ui/android-app/scripts/start_emulator_stack.sh", import.meta.url), "utf8");
  assert.match(launcher, /emulator_disk\.py" --phase before-boot --avd-path "\$avd_path"/);
});

test("Chrome-on-Android consumes the same immutable web build without Rust or build caches", () => {
  const job = e2e.jobs["android-chrome-live-feed"];
  assert.ok(job.needs?.includes("build-release-apps"));
  assert.ok(job.needs?.includes("release-journey-fixture"));
  assert.equal(job.env.AEROBAG_RELEASE_JOURNEY_WEB_DIST, "${{ github.workspace }}/.ci/release-e2e-apps/web-dist");
  assert.equal(job.env.AEROBAG_RELEASE_JOURNEY_FIXTURE, "${{ github.workspace }}/.ci/release-journey-materialized/fixture.json");
  assert.doesNotMatch(job.steps.map((step) => step.run ?? "").join("\n"), /rustup|cargo|wasm_bindgen|build:wasm/);
  assert.ok(!job.steps.some((step) => step.uses?.startsWith("actions/cache@")));
  assert.ok(job.steps.some((step) => step.run?.includes("materialize_release_journey_fixture.py")));
});

test("native Android smoke lane supplies both immutable APKs and keeps the host fixture compiler", () => {
  const job = e2e.jobs["android-native"];
  const run = job.steps.find((step) => step.run?.includes("./ui/android-app/scripts/run_e2e_ci.sh"));
  assert.equal(run.env.AEROBAG_E2E_APK, "${{ github.workspace }}/.ci/release-e2e-apps/aerobag-release-e2e.apk");
  assert.equal(run.env.AEROBAG_E2E_DRIVER_APK, "${{ github.workspace }}/.ci/release-e2e-apps/aerobag-e2e-driver.apk");
  assert.ok(job.steps.some((step) => step.run?.includes('rustup toolchain install "$RUST_TOOLCHAIN" --profile minimal')),
    "the package-only server still compiles service-bulletin-contract for the host");
});
