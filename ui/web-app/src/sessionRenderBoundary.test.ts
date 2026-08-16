// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const source = readFileSync(new URL("./App.tsx", import.meta.url), "utf8");

describe("session render boundary", () => {
  it("keeps high-rate session publications out of root React state", () => {
    expect(source).toContain("sessionRenderStore.publish(publication)");
    expect(source).toContain("publicationAffectsGroups(publication, SHELL_SESSION_UPDATE_GROUPS)");
    expect(source).toContain("useSessionSnapshotGroups(");
    expect(source).toContain("HIGH_RATE_SESSION_UPDATE_GROUPS");
  });

  it("keeps map viewport and query invalidations in page-owned stores", () => {
    expect(source).toContain("new RenderValueStore<MapViewportState | null>(null)");
    expect(source).toContain("new RenderValueStore(initialUiInvalidationRevisions())");
    expect(source).not.toContain("useState<UiInvalidationRevisions>");
  });

  it("does not reconcile inactive page subtrees after parent renders", () => {
    expect(source).toContain("(previous, next) => !previous.active && !next.active");
    expect(source).toContain('<PageLayer active={page === "map"}>');
    expect(source).toContain('<PageLayer active={page === "charts"}>');
  });
});
