// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import assert from "node:assert/strict";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { createJourneyRuntime } from "./release-journey-runtime.mjs";
import { openProcedureRow } from "./release-journey-implementations.mjs";

for (const platform of ["web", "android"]) {
  for (const outcome of ["opens", "lost-click", "unreachable"]) {
    test(`${platform} procedure row above the viewport: ${outcome}`, async t => {
      const artifactDir = mkdtempSync(join(tmpdir(), "aerobag-procedure-row-"));
      t.after(() => rmSync(artifactDir, { recursive: true, force: true }));
      const row = { id: "parity:plan-procedure-row:CHINS5:uid:arrival", text: "CHINS5" };
      let visible = false, tray = false, clock = 0;
      const reveals = [], clicks = [];
      const driver = {
        platform,
        async readProjection(prefix) {
          assert.equal(prefix, "parity:plan-procedure-row:CHINS5:uid:");
          return [row];
        },
        async revealElement(id) {
          reveals.push(id);
          assert.equal(id, platform === "web" ? "plan-row-arrival" : "plan-row:arrival");
          visible = outcome !== "unreachable";
          return visible ? { test_id: id, enabled: true, actionable: true } : null;
        },
        async readAction(id) {
          assert.equal(id, "plan-row:arrival");
          return visible ? { test_id: id, enabled: true, actionable: true } : null;
        },
        async performAction(id, ready) {
          assert.equal(ready.test_id, id);
          assert.ok(visible, "never click an offscreen procedure based on model presence");
          clicks.push(id);
          if (outcome === "opens") tray = true;
        },
        async readElement(id) {
          assert.equal(id, "plan-row-tray-scrim");
          return tray ? { enabled: true } : null;
        },
        async readSessionRevision() { return 22; },
      };
      const runtime = createJourneyRuntime({
        journey: { id: "test.procedure-row", assertions: [] }, platform, driver, artifactDir,
      });
      const transition = runtime.transition;
      runtime.transition = (description, contract) => transition(description, {
        ...contract,
        scheduler: { now: () => clock, setTimeout, clearTimeout },
        waitForObservation: async () => { clock += 50; },
      });
      if (outcome === "opens") assert.equal(await openProcedureRow(runtime, "CHINS5"), row);
      else await assert.rejects(openProcedureRow(runtime, "CHINS5"),
        outcome === "lost-click" ? /timed out/ : /not present after explicit traversal/);
      assert.equal(reveals.length, 1);
      assert.equal(clicks.length, outcome === "unreachable" ? 0 : 1);
    });
  }
}
