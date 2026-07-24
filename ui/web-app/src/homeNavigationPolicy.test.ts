// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const appSource = readFileSync(new URL("./App.tsx", import.meta.url), "utf8");
const adapterSource = readFileSync(new URL("./domain/appCoreAdapter.ts", import.meta.url), "utf8");

function sourceBetween(start: string, end: string): string {
  const startIndex = appSource.indexOf(start);
  expect(startIndex, start).toBeGreaterThanOrEqual(0);
  const endIndex = appSource.indexOf(end, startIndex);
  expect(endIndex, end).toBeGreaterThan(startIndex);
  return appSource.slice(startIndex, endIndex);
}

describe("Home navigation policy", () => {
  it("renders the core-owned button inventory through the disabled-action affordance", () => {
    const homePage = sourceBetween("function HomePage(", "function AboutPage(");

    expect(appSource).not.toContain("HOME_GRID_BUTTONS");
    expect(homePage).toContain("props.state.buttons.map");
    expect(homePage).toContain('aria-disabled={disabled ? "true" : undefined}');
    expect(homePage).toContain("title={disabledReason ?? undefined}");
    expect(homePage).toContain("showDisabledAction(disabledReason)");
    expect(homePage).toContain("disabledActionToast.message");
  });

  it("advertises that web cannot manage Offline Packages", () => {
    expect(adapterSource).toContain("offline_packages: null");
  });
});
