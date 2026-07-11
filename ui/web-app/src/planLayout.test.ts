import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const styles = readFileSync(new URL("./styles.css", import.meta.url), "utf8");
const appSource = readFileSync(new URL("./App.tsx", import.meta.url), "utf8");

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

  it("uses a top-aligned textarea for the route entry caret layer", () => {
    expect(appSource).toMatch(/<textarea\s+className="planEntryInput"[\s\S]*data-testid="plan-append-route-input"/);

    const inputBlocks = [...styles.matchAll(/\.planEntryInput\s*\{([^}]*)\}/g)].map((match) => match[1] ?? "").join("\n");
    expect(inputBlocks).toContain("display: block");
    expect(inputBlocks).toContain("height: var(--thumb)");
    expect(inputBlocks).toContain("resize: none");
    expect(inputBlocks).toContain("overflow: hidden");
  });
});
