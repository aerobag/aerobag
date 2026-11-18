// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later
import assert from "node:assert/strict";
import test from "node:test";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { createJourneyRuntime } from "./release-journey-runtime.mjs";

for (const platform of ["web", "android"]) {
  for (const completedPage of ["home", "data_status"]) {
    test(`${platform}: navigation observes ${completedPage} completing during action readiness`, async () => {
      let page = completedPage === "home" ? "map" : "home";
      const actions = [];
      const directory = mkdtempSync(join(tmpdir(), "aerobag-navigation-convergence-"));
      const runtime = createJourneyRuntime({
        journey: { id: "navigation-convergence", assertions: [] }, platform,
        fixture: null, artifactDir: directory,
        driver: {
          async readCurrentPage() { return { pageId: page }; },
          async readPage(expected) { return expected === page ? { pageId: page } : null; },
          async readNavigationAction(destination) {
            // A preceding navigation finishes after current-page discovery,
            // before the real transition runner checks its precondition.
            if (destination === completedPage) page = completedPage;
            return { destination };
          },
          async activateNavigation(destination) { actions.push(destination); page = destination; },
          async captureFrame() {},
        },
      });
      try {
        assert.deepEqual(await runtime.openPage("data_status"), { pageId: "data_status" });
        assert.deepEqual(actions, completedPage === "home" ? ["data_status"] : []);
      } finally { rmSync(directory, { recursive: true, force: true }); }
    });
  }
}
