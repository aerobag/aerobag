// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const appSource = readFileSync(new URL("./App.tsx", import.meta.url), "utf8");
const styleSource = readFileSync(new URL("./styles.css", import.meta.url), "utf8");

function sourceBetween(source: string, start: string, end: string): string {
  const startIndex = source.indexOf(start);
  const endIndex = source.indexOf(end, startIndex + start.length);
  expect(startIndex).toBeGreaterThanOrEqual(0);
  expect(endIndex).toBeGreaterThan(startIndex);
  return source.slice(startIndex, endIndex);
}

describe("data status page layout", () => {
  it("honors core-owned full-width facts", () => {
    const rowRenderer = sourceBetween(
      appSource,
      "function DataStatusPageRowArticle(",
      "function renderDataStatusFactValue(",
    );

    expect(rowRenderer).toContain('fact.full_width ? " isFullWidth" : ""');
    expect(styleSource).toContain(".dataStatusPageFact.isFullWidth");
    expect(styleSource).toContain("grid-column: 1 / -1;");
  });

  it("wraps status values instead of truncating them", () => {
    const valueRule = sourceBetween(
      styleSource,
      ".dataStatusPageFact dd {",
      ".dataStatusPageFact dd a {",
    );

    expect(valueRule).toContain("overflow-wrap: anywhere;");
    expect(valueRule).toContain("white-space: normal;");
    expect(valueRule).not.toContain("text-overflow: ellipsis;");
  });
});
