// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const appSource = readFileSync(new URL("./App.tsx", import.meta.url), "utf8");
const styles = readFileSync(new URL("./styles.css", import.meta.url), "utf8");

describe("plate approach loader presentation", () => {
  it("renders the core-owned header and destructive tone", () => {
    expect(appSource).toContain("header={plateProcedureLoadMenu.header}");
    expect(appSource).toContain("headerTone={plateProcedureLoadMenu.header_tone}");
    expect(appSource).toContain("const planProcedureLoadKey = props.flightPlanRouteRevision");
    expect(appSource).toContain('headerTone === "destructive" ? " isDestructive"');
    expect(appSource).toContain('["--theme-situation-status-unavailable-fg" as string]');
    expect(styles).toContain(".trayHeader.isDestructive");
    expect(styles).toContain("color: var(--theme-situation-status-unavailable-fg)");
  });
});
