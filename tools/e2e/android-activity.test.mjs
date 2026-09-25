// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import assert from "node:assert/strict";
import test from "node:test";
import { runInNewContext } from "node:vm";
import { rejectObservationOverrides } from "./android-observation-contract.mjs";
import { androidResumedActivityFromDumpsys } from "./android-harness.mjs";
import { AndroidSemanticJourneyDriver } from "./semantic-journey-driver.mjs";
import { releaseJourneyImplementation } from "./release-journey-implementations.mjs";

function activities(resumed, aboutUrl = "https://aerobag.org/about") {
  const records = {
    browser: "browser u0 com.android.fakesystemapp/.browser.StubBrowserActivity t34",
    app: "app u0 org.aerobag.app/.MainActivity t34",
    other: "other u0 com.android.browser/.BrowserActivity t35",
  };
  return `ACTIVITY MANAGER ACTIVITIES
    topResumedActivity=ActivityRecord{${records[resumed]}}
    * Hist #2: ActivityRecord{${records.other}}
      Intent { act=android.intent.action.VIEW dat=https://example.org/ }
    * Hist #1: ActivityRecord{${records.browser}}
      Intent { act=android.intent.action.VIEW dat=${aboutUrl} }
    * Hist #0: ActivityRecord{${records.app}}
      Intent { act=android.intent.action.MAIN cmp=org.aerobag.app/.MainActivity }
`;
}

test("external About observation uses the resumed activity, not a URL anywhere in history", async () => {
  let dump;
  const readElement = runInNewContext(
    `({ ${AndroidSemanticJourneyDriver.prototype.readElement} }).readElement`,
    { adb: () => dump, androidResumedActivityFromDumpsys, rejectObservationOverrides },
  );
  for (const [resumed, expected] of [["browser", true], ["app", false], ["other", false]]) {
    dump = activities(resumed);
    assert.equal(Boolean(await readElement("external-page:about")), expected);
  }
  dump = activities("browser", "https://aerobag.org/about/not-the-about-page");
  assert.equal(await readElement("external-page:about"), null);
  dump = activities("browser").replace("topResumedActivity=", "mResumedActivity: ");
  assert.ok(await readElement("external-page:about"));
  dump = "topResumedActivity=null\n";
  assert.equal(await readElement("external-page:about"), null);
});

for (const brokenBack of [false, true]) {
  test(`About return ${brokenBack ? "rejects a lost Back action" : "waits for the foreground app"} despite readable background state`, async () => {
    let external = false;
    let page = "home";
    let backs = 0;
    const checks = [];
    const runtime = {
      platform: "android",
      driver: {
        async readProjection() {
          // This remains readable even with the external browser in front.
          return [{ id: `parity:startup-state:ready:true:disclaimer_required:false:tour_pending:false:page:${page}:persisted_page:${page === "settings" ? "Settings" : "Home"}` }];
        },
        async readElement(id) {
          if (id === "external-page:about") return external ? { text: "About" } : null;
          if (id === `page:${page}`) return { enabled: true };
          return null;
        },
        async readNavigationAction(id) {
          assert.equal(id, "map");
          return !external && page === "home" ? { enabled: true } : null;
        },
        async back() { backs++; if (!brokenBack) external = false; },
      },
      async reset() { assert.equal(external, false); page = "home"; },
      async reload() {},
      async openPage(id) { page = id; },
      async eventually(description, read) { const value = await read(); assert.ok(value, description); return value; },
      async action(_description, id, { complete }) {
        assert.equal(id, "home-button:About");
        assert.equal(await complete(), null);
        external = true;
        assert.ok(await complete());
      },
      async transition(description, { ready, act, complete }) {
        const evidence = await ready();
        assert.ok(evidence);
        assert.ok(!(await complete()), "completion must not already hold while the browser is foreground");
        await act(evidence);
        assert.ok(await complete(), description);
      },
      check(id, passed) { assert.ok(passed, id); checks.push(id); },
    };
    const run = releaseJourneyImplementation("shared.about-and-saved-state")(runtime);
    if (brokenBack) await assert.rejects(run, /return from external About page/);
    else {
      await run;
      assert.deepEqual(checks, ["navigation.about", "saved-state.restart"]);
    }
    assert.equal(backs, 1, "Back is dispatched exactly once");
  });
}
