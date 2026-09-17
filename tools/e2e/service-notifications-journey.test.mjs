// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import test from "node:test";
import { fixtureServer, liveFeedStream } from "./fixture-test-server.mjs";
import { releaseJourneyImplementation } from "./release-journey-implementations.mjs";
import { journeyById, RELEASE_JOURNEYS } from "./release-journey-registry.mjs";
import { SERVICE_BULLETIN_PATH } from "./service-notifications-fixture.mjs";
import {
  androidActionCandidates, androidElementMayRequireVerticalScroll, androidElementSemanticTag,
  androidPageTag, androidProjectionMayRequireVerticalScan,
  WebSemanticJourneyDriver,
} from "./semantic-journey-driver.mjs";
import { observeUntil, observeValueUntilStable } from "./transition-contract.mjs";

const journeyId = "shared.service-notifications";
const aggregateTag = "data-status-box-service:unread";
const inboxTag = "data-status-action-service:unread-service:inbox";

// The real journey talks to the real HTTP/SSE fixture. Only the rendered UI is
// modeled here; these rejection tests are not evidence of a real platform run.
function modelRuntime(t, platform, fixtureOrigin, defect = null, { unrelatedStatus = true } = {}) {
  let document, stream, page = "map", panel = false, expanded = false, bodyId = null;
  let revealedBody = null, statusEntries = 0;
  const read = new Set(), checks = [], actions = [], pages = [], dismissals = [];
  const captures = [], scans = [];
  const receipt = (notice) => createHash("sha256").update(notice.id).digest("hex");
  const noticeTag = (notice) => `parity:service:notice:${receipt(notice)}`;
  const bodyTag = (notice) => `parity:service:body:${receipt(notice)}`;
  const unread = () => document.notices.filter((notice) => !notice.resolved && !read.has(notice.id));
  const hasStatus = () => unrelatedStatus || unread().length > 0 ||
    ["retained-aggregate", "offscreen-aggregate", "offscreen-launcher"].includes(defect);
  const rowAvailable = (notice) => page === "data_status" && (expanded || defect === "folded-content-leak") &&
    !(defect === "lost-history" && statusEntries > 1 && read.has(notice.id)) &&
    !(defect === "lost-archive" && notice.resolved);
  const element = (id, text = id) => ({ id, test_id: id, text, enabled: true, actionable: true });
  const getDocument = async () => {
    document = await fetch(new URL(SERVICE_BULLETIN_PATH, fixtureOrigin), {
      signal: AbortSignal.timeout(2_000),
    }).then((response) => response.json());
  };
  const sync = async () => {
    if (defect !== "ignored-sse" && stream?.hints.at(-1)?.revision > document.revision) await getDocument();
  };
  const webDriver = new WebSemanticJourneyDriver({
    async firstExisting(selectors) {
      assert.deepEqual(selectors, [".trayScrim", '[aria-label^="Close "]']);
      return panel ? ".trayScrim" : null;
    },
    async clickIfVisible(selector) {
      assert.equal(selector, ".trayScrim");
      assert.ok(panel, "scrim click requires an open popup");
      dismissals.push("web-scrim");
      if (defect !== "undismissed-popup") panel = false;
      return true;
    },
    page: { async send() { assert.fail("web popup must close through its exposed scrim, not keyboard fallback"); } },
  });
  const driver = {
    async readElement(id, { indexed = false } = {}) {
      if (platform === "android") assert.equal(indexed, true, `${id} must not search a tree to prove presence or absence`);
      if (id === `page:${page}`) return element(id);
      if (id === "data-status-launcher") {
        if (page !== "map" || !hasStatus() || (statusEntries > 0 && defect === "offscreen-launcher")) return null;
        return { ...element(id), actionable: platform !== "web" || !panel };
      }
      if (id === "data-status-panel") return panel ? element(id) : null;
      if ([aggregateTag, inboxTag].includes(id)) {
        const label = defect === "wrong-inbox-label" ? "Open service page"
          : platform === "android" ? "READ NOTIFICATIONS" : "Read notifications";
        return panel && (unread().length || defect === "retained-aggregate")
          ? element(id, id === inboxTag ? label : "Service Notifications") : null;
      }
      if (id === "parity:service:section") {
        const state = defect === "missing-expanded-field" ? "window-focus:true"
          : `expanded:${defect === "invalid-expanded-field" ? "trueish" : expanded}:window-focus:true`;
        return page === "data_status"
          ? { ...element(id), state: null, disabled_reason: platform === "android" ? state : null } : null;
      }
      if (id === "parity:service:toggle") {
        return page === "data_status"
          ? { ...element(id, "Service Notifications 0 unread service notifications"),
            expanded: platform === "web" ? expanded : null } : null;
      }
      if (id === "parity:service:mark-all-read") {
        return page === "data_status" && expanded && unread().length ? element(id) : null;
      }
      for (const notice of document.notices) {
        if (!rowAvailable(notice)) continue;
        if (id === noticeTag(notice)) return element(id, notice.title);
        if (id === bodyTag(notice) && bodyId === notice.id && revealedBody === notice.id) {
          return element(id, defect === "wrong-body" ? "unrelated notice text" : notice.body);
        }
      }
      return null;
    },
    async readProjection(prefix, { indexed = false } = {}) {
      if (prefix === "parity:startup-state:") return [element(`${prefix}ready:true:disclaimer_required:false`)];
      if (platform === "android") assert.equal(indexed, true, `${prefix} must not search a tree for folded/absent content`);
      if (prefix === "data-status-launcher") return page === "map" && hasStatus()
        ? [element(platform === "android" ? `parity:${prefix}` : prefix)] : [];
      const result = [];
      for (const notice of document.notices) {
        if (!rowAvailable(notice)) continue;
        result.push(element(noticeTag(notice), notice.title));
        if (bodyId === notice.id && defect !== "missing-body") result.push(element(bodyTag(notice), notice.body));
      }
      return result.filter((entry) => entry.id.startsWith(prefix));
    },
    async scanProjection(prefix) {
      assert.equal(prefix, "data-status-box-");
      scans.push(prefix);
      const boxes = unrelatedStatus ? [element("data-status-box-live-feed:connection")] : [];
      if (defect === "offscreen-aggregate") boxes.push(element(aggregateTag));
      return boxes;
    },
    async back() {
      if (platform === "web") return webDriver.back();
      dismissals.push("android-back");
      if (defect !== "undismissed-popup") panel = false;
    },
    async captureFrame(path) {
      captures.push({ path, page, expanded, unread: unread().length, panel });
      return path;
    },
  };
  const runtime = {
    platform, fixtureOrigin, driver, checks, actions, pages, dismissals, captures, scans,
    artifactDir: `/model/${platform}`,
    async reset() {
      stream?.close();
      read.clear(); bodyId = null; revealedBody = null; statusEntries = 0;
      page = "map"; panel = false; expanded = false;
      await getDocument();
      stream = await liveFeedStream(t, fixtureOrigin);
      await stream.waitForHint(1);
    },
    async openPage(id) {
      pages.push(id); page = id; panel = false;
      if (id === "map" && statusEntries > 0 && defect === "orphan-popup") panel = true;
      if (id === "data_status") {
        statusEntries += 1;
        expanded = unread().length > 0 || defect === "unfolded-reentry";
        if (defect !== "retained-body") bodyId = null;
      }
    },
    async revealElement(id) {
      const notice = document.notices.find((notice) => bodyTag(notice) === id);
      if (notice && defect !== "invisible-body") revealedBody = notice.id;
      return driver.readElement(id, { indexed: true });
    },
    async revealProjectionMatching(prefix, title) {
      const rows = await driver.readProjection(prefix, { indexed: true });
      const row = rows.find((row) => row.text === title);
      assert.ok(row, `missing rendered notice ${title}`);
      return row;
    },
    async eventually(description, probe, timeoutMs = 200) {
      // UI state is synchronous in this model; real HTTP/SSE readiness still
      // needs the bounded resource deadline explicitly supplied by the journey.
      return (await observeUntil(description, async () => {
        await sync();
        return probe();
      }, { timeoutMs, intervalMs: 5 })).value;
    },
    async stable(description, probe) {
      return (await observeValueUntilStable(description, async () => {
        await sync();
        return probe();
      }, { timeoutMs: 200, intervalMs: 5 })).value;
    },
    async transition(description, contract) {
      const ready = await contract.ready();
      assert.ok(ready, `${description} is not ready`);
      assert.ok(!await contract.complete(), `${description} completed before its action`);
      await contract.act(ready);
      return this.eventually(description, contract.complete);
    },
    async action(description, id, contract) {
      const ready = await driver.readElement(id, { indexed: true });
      assert.ok(ready?.enabled && ready.actionable !== false, `action ${id} is not visibly ready`);
      assert.ok(!await contract.complete(), `${description} completed before its action`);
      actions.push(id);
      if (id === "data-status-launcher") panel = !panel;
      else if (id === inboxTag) {
        await this.openPage(defect === "separate-page" ? "service_notifications" : "data_status");
        if (defect === "stuck-popup") panel = true;
        if (defect === "read-on-entry") document.notices.forEach((notice) => read.add(notice.id));
      } else if (id === "parity:service:toggle") {
        if (defect !== "broken-expand") expanded = !expanded;
      } else {
        const notice = document.notices.find((notice) => noticeTag(notice) === id);
        assert.ok(notice, `unexpected action ${id}`);
        bodyId = bodyId === notice.id ? null : notice.id;
        revealedBody = null;
        if (defect !== "not-marked-read") read.add(notice.id);
        if (defect === "read-all-on-open") document.notices.forEach((notice) => read.add(notice.id));
      }
      return this.eventually(description, contract.complete);
    },
    check(id, pass) { assert.ok(pass, `${id} failed`); checks.push(id); },
  };
  return runtime;
}

for (const platform of ["web", "android"]) {
  test(`service journey observes SSE arrival, Status entry, bodies, read aggregate and folded history on ${platform}`, { timeout: 5_000 }, async (t) => {
    const { origin } = await fixtureServer(t);
    const runtime = modelRuntime(t, platform, origin, "retained-body");
    await releaseJourneyImplementation(journeyId)(runtime);
    assert.deepEqual(runtime.checks, journeyById(journeyId).assertions);
    assert.equal(runtime.actions.filter((id) => id === inboxTag).length, 1);
    assert.equal(runtime.actions.filter((id) => id === "parity:service:toggle").length, 1);
    assert.equal(runtime.actions.filter((id) => id.startsWith("parity:service:notice:")).length, 7);
    assert.equal(runtime.actions.filter((id) => id === "data-status-launcher").length, 2);
    assert.deepEqual(runtime.dismissals, [platform === "web" ? "web-scrim" : "android-back"]);
    assert.deepEqual(runtime.scans, ["data-status-box-"]);
    assert.deepEqual(runtime.captures, [
      { path: `${runtime.artifactDir}/status-unread.png`, page: "data_status", expanded: true, unread: 3, panel: false },
      { path: `${runtime.artifactDir}/status-read-history.png`, page: "data_status", expanded: true, unread: 0, panel: false },
    ]);
    assert.ok(!runtime.pages.includes("service_notifications"));
    const health = await fetch(`${origin}/__health`).then((response) => response.json());
    assert.equal(health.service_notifications.publication, "empty");
  });
  test(`service journey observes the entire launcher disappearing with no unrelated faults on ${platform}`, { timeout: 5_000 }, async (t) => {
    const { origin } = await fixtureServer(t);
    const runtime = modelRuntime(t, platform, origin, null, { unrelatedStatus: false });
    await releaseJourneyImplementation(journeyId)(runtime);
    assert.deepEqual(runtime.checks, journeyById(journeyId).assertions);
    assert.equal(runtime.actions.filter((id) => id === "data-status-launcher").length, 1);
    assert.deepEqual(runtime.scans, []);
    assert.deepEqual(runtime.dismissals, []);
    assert.deepEqual(runtime.captures.map(({ unread }) => unread), [3, 0]);
  });
  for (const [defect, expected] of [
    ["offscreen-launcher", /data-status-launcher is not visibly ready/],
    ["orphan-popup", /service.read-hides-aggregate failed/],
    ["offscreen-aggregate", /service.read-hides-aggregate failed/],
  ]) {
    test(`healthy service journey rejects ${defect} on ${platform}`, { timeout: 5_000 }, async (t) => {
      const { origin } = await fixtureServer(t);
      const runtime = modelRuntime(t, platform, origin, defect, { unrelatedStatus: false });
      await assert.rejects(releaseJourneyImplementation(journeyId)(runtime), expected);
      assert.ok(!runtime.checks.includes("service.read-hides-aggregate"));
    });
  }
  test(`service journey rejects an undismissed status popup on ${platform}`, { timeout: 5_000 }, async (t) => {
    const { origin } = await fixtureServer(t);
    const runtime = modelRuntime(t, platform, origin, "undismissed-popup");
    await assert.rejects(releaseJourneyImplementation(journeyId)(runtime), /dismiss status popup/);
    assert.deepEqual(runtime.dismissals, [platform === "web" ? "web-scrim" : "android-back"]);
    assert.ok(!runtime.checks.includes("service.reentry-folded"));
    const health = await fetch(`${origin}/__health`).then((response) => response.json());
    assert.equal(health.service_notifications.publication, "empty");
  });
}

for (const [defect, expected] of [
  ["ignored-sse", /SSE-announced bulletin fetched/],
  ["wrong-inbox-label", /Read notifications entry/],
  ["separate-page", /Read notifications opens Status/],
  ["stuck-popup", /Read notifications opens Status/],
  ["missing-expanded-field", /Read notifications opens Status/],
  ["invalid-expanded-field", /Read notifications opens Status/],
  ["missing-body", /open Journey information notice/],
  ["wrong-body", /visible body of Journey information notice/],
  ["invisible-body", /visible body of Journey information notice/],
  ["read-on-entry", /opening one notice marked other notices read/],
  ["read-all-on-open", /opening one notice marked other notices read/],
  ["not-marked-read", /read notices remove only the service aggregate/],
  ["retained-aggregate", /read notices remove only the service aggregate/],
  ["offscreen-aggregate", /service.read-hides-aggregate failed/],
  ["unfolded-reentry", /Status reentry folds read history/],
  ["folded-content-leak", /service.reentry-folded failed/],
  ["broken-expand", /explicitly expand notification history/],
  ["lost-history", /missing rendered notice/],
  ["lost-archive", /missing rendered notice Journey archived notice/],
]) {
  test(`service journey rejects ${defect} and resets its publication`, { timeout: 20_000 }, async (t) => {
    const { origin } = await fixtureServer(t);
    await assert.rejects(releaseJourneyImplementation(journeyId)(modelRuntime(t, "android", origin, defect)), expected);
    const health = await fetch(`${origin}/__health`).then((response) => response.json());
    assert.deepEqual(health.service_notifications, {
      revision: 1, publication: "empty", subscribers: 0, live_announcements: 0,
    });
  });
}

test("service controls use shared exact tags and support physical scrolling; the separate page is removed", () => {
  assert.equal(androidPageTag("data_status"), "parity:page:data_status");
  assert.equal(androidPageTag("service_notifications"), null);
  assert.equal(androidElementSemanticTag(inboxTag), `parity:${inboxTag}`);
  assert.equal(androidActionCandidates(inboxTag)[0], `parity:${inboxTag}`);
  for (const tag of ["parity:service:toggle", `parity:service:notice:${"a".repeat(64)}`, `parity:service:body:${"a".repeat(64)}`]) {
    assert.equal(androidElementSemanticTag(tag), tag);
    assert.equal(androidElementMayRequireVerticalScroll(tag), true);
  }
  assert.equal(androidElementMayRequireVerticalScroll(inboxTag), true);
  assert.equal(androidProjectionMayRequireVerticalScan("parity:service:notice:"), true);
  assert.equal(androidProjectionMayRequireVerticalScan("data-status-box-"), true);
  assert.deepEqual(journeyById(journeyId).platforms, ["web", "android"]);
  assert.ok(RELEASE_JOURNEYS.every((entry) => !entry.assertions.some((id) =>
    ["home.service-notifications", "navigation.service-notifications"].includes(id))));
});
