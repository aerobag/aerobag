// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import assert from "node:assert/strict";
import test from "node:test";
import { androidFixtureNetworkState, useAndroidFixtureNetwork } from "./android-harness.mjs";
import { observeUntil, ObservationTimeoutError } from "./transition-contract.mjs";

function dump(id, validated = false) {
  return id === null ? "Active default network: none\n" :
    `Active default network: ${id}\n  NetworkAgentInfo{network{${id}} Score(IS_VALIDATED) nc{[ Capabilities: INTERNET&${validated ? "VALIDATED&" : ""}NOT_SUSPENDED ]}}\n`;
}

test("fixture readiness uses the default network's capability, not historical validation text", () => {
  assert.throws(() => androidFixtureNetworkState("service unavailable"), /no default-network state/);
  assert.deepEqual(androidFixtureNetworkState(dump("10")), {
    defaultNetwork: "10", networks: [{ id: "10", validated: false }],
  });
  const unrelated = dump("10").replace("Active default network: 10", "Active default network: 20") +
    dump("30", true).split("\n")[1];
  assert.equal(androidFixtureNetworkState(unrelated).networks.some(n => n.id === "20" && n.validated), false);
});

function model({ failValidation = false, qemu = "1", wifi = "1" } = {}) {
  const settings = new Map([
    ["captive_portal_http_url", "http://original.invalid/204"],
    ["captive_portal_https_url", "null"], ["captive_portal_fallback_url", "null"],
    ["wifi_on", wifi], ["mobile_data", "1"],
  ]);
  const before = new Map(settings);
  const commands = [];
  let phase = "original", reads = 0, clock = 0;
  const adbCommand = (_serial, args) => {
    commands.push(args);
    if (args[1] === "getprop") return qemu;
    if (args[1] === "settings") {
      const key = args[4];
      if (args[2] === "get") return settings.get(key);
      settings.set(key, args[2] === "delete" ? "null" : args[5]);
      return "";
    }
    if (args[1] === "svc") { phase = args[3]; reads = 0; return ""; }
    assert.deepEqual(args, ["shell", "dumpsys", "connectivity"]);
    reads++;
    if (phase === "disable") return reads === 1 ? dump("10", true) : dump(null);
    if (phase === "enable") return dump("11", reads > 1 && !failValidation);
    return dump("10", true);
  };
  const observe = (description, read, options) => observeUntil(description, read, {
    ...options,
    scheduler: { now: () => clock, setTimeout, clearTimeout },
    waitForNextProbe: async () => { clock += 1000; },
  });
  return { adbCommand, observe, settings, before, commands };
}

test("local probes require disconnection and fresh validation; restoration preserves absent settings and radios", async () => {
  const m = model({ wifi: "0" });
  const restore = await useAndroidFixtureNetwork("emulator-test", "http://127.0.0.1:18093", m);
  for (const key of ["captive_portal_http_url", "captive_portal_https_url", "captive_portal_fallback_url"]) {
    assert.equal(m.settings.get(key), "http://127.0.0.1:18093/__connectivity");
  }
  assert.equal(m.commands.filter(args => args[1] === "dumpsys").length, 4);
  await restore();
  assert.deepEqual(m.settings, m.before);
  assert.equal(m.commands.some(args => args[1] === "svc" && args[2] === "wifi"), false);
});

test("failed validation is diagnostic and restores settings instead of accepting an unvalidated network", async () => {
  const m = model({ failValidation: true });
  await assert.rejects(useAndroidFixtureNetwork("emulator-test", "http://127.0.0.1:18093", m), error => {
    assert.ok(error instanceof ObservationTimeoutError);
    assert.deepEqual(error.diagnostics.last_value, {
      defaultNetwork: "11", networks: [{ id: "11", validated: false }],
    });
    return true;
  });
  assert.deepEqual(m.settings, m.before);
});

for (const [qemu, origin] of [["0", "http://127.0.0.1:18093"], ["1", "https://aerobag.org"]]) {
  test(`refuse non-fixture network modification: emulator=${qemu} origin=${origin}`, async () => {
    const m = model({ qemu });
    await assert.rejects(useAndroidFixtureNetwork("device", origin, m), /requires an emulator/);
    assert.ok(m.commands.every(args => args[1] === "getprop"));
    assert.deepEqual(m.settings, m.before);
  });
}
