import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const styles = readFileSync(new URL("./styles.css", import.meta.url), "utf8");

describe("flight plan layout CSS", () => {
  it("keeps the table header sticky to the vertical scroll viewport", () => {
    const headerBlocks = [...styles.matchAll(/\.planHeader\s*\{([^}]*)\}/g)].map((match) => match[1] ?? "").join("\n");
    expect(headerBlocks).toContain("position: sticky");
    expect(headerBlocks).toContain("top: 0");

    const viewportBlocks = [...styles.matchAll(/\.planScrollViewport\s*\{([^}]*)\}/g)].map((match) => match[1] ?? "").join("\n");
    expect(viewportBlocks).toMatch(/overflow:\s*auto/);

    const surfaceBlocks = [...styles.matchAll(/\.planScrollSurface\s*\{([^}]*)\}/g)].map((match) => match[1] ?? "");
    expect(surfaceBlocks.length).toBeGreaterThan(0);
    for (const block of surfaceBlocks) {
      expect(block).not.toMatch(/overflow(?:-[xy])?:\s*auto/);
      expect(block).not.toMatch(/overflow(?:-[xy])?:\s*scroll/);
    }
  });
});
