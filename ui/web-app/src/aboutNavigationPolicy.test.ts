// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const appSource = readFileSync(new URL("./App.tsx", import.meta.url), "utf8");
const aboutSource = readFileSync(new URL("../about.html", import.meta.url), "utf8");
const viteSource = readFileSync(new URL("../vite.config.ts", import.meta.url), "utf8");

function sourceBetween(start: string, end: string): string {
  const startIndex = appSource.indexOf(start);
  expect(startIndex, start).toBeGreaterThanOrEqual(0);
  const endIndex = appSource.indexOf(end, startIndex);
  expect(endIndex, end).toBeGreaterThan(startIndex);
  return appSource.slice(startIndex, endIndex);
}

describe("About navigation policy", () => {
  it("builds About as a standalone HTML document", () => {
    expect(aboutSource).toContain('data-aerobag-page="about"');
    expect(aboutSource).toContain('id="about-page"');
    expect(aboutSource).toContain("__AEROBAG_ABOUT_README_HTML__");
    expect(aboutSource).toContain("__AEROBAG_NO_WARRANTY_HTML__");
    expect(aboutSource).not.toContain('id="startup-shell"');
    expect(aboutSource).not.toContain('id="root"');
    expect(aboutSource).not.toContain("/src/main.tsx");
    expect(viteSource).toContain('about: path.join(webSourceRoot, "about.html")');
    expect(viteSource).toContain('requestPath === "/about"');
  });

  it("leaves the React application through a normal About link", () => {
    const homePage = sourceBetween("function HomePage(", "function CloudQrCode(");

    expect(appSource).not.toContain("function AboutPage(");
    expect(appSource).not.toContain("appPageForCurrentPath");
    expect(homePage).toContain("href={disabled ? undefined : urlForAppPage(presentation.documentPage)}");
  });
});
