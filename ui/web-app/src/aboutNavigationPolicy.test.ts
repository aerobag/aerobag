// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const appSource = readFileSync(new URL("./App.tsx", import.meta.url), "utf8");

function sourceBetween(start: string, end: string): string {
  const startIndex = appSource.indexOf(start);
  expect(startIndex, start).toBeGreaterThanOrEqual(0);
  const endIndex = appSource.indexOf(end, startIndex);
  expect(endIndex, end).toBeGreaterThan(startIndex);
  return appSource.slice(startIndex, endIndex);
}

describe("About navigation policy", () => {
  it("serves a direct about request without mounting the operational app", () => {
    const entry = sourceBetween("export default function App()", "function OperationalApp()");

    expect(entry).toContain('appPageForCurrentPath() === "about"');
    expect(entry).toContain("<AboutPage />");
    expect(entry).toContain("<OperationalApp />");
    expect(entry).not.toContain("useEffect(");
  });

  it("opens the web app through document navigation without requiring a ready core session", () => {
    const aboutPage = sourceBetween("function AboutPage(", "function formatApkSize(");

    expect(aboutPage).toContain('href={urlForAppPage("home")}');
    expect(aboutPage).not.toContain("onOpenApp");
  });
});
