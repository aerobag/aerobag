import { describe, expect, it } from "vitest";
import { createInitialViewport, dragViewport } from "./mapViewport";
import { mapView } from "./sampleData";
import { MapFollowTargetGate } from "./mapFollowTargetGate";

describe("MapFollowTargetGate", () => {
  it("blocks a stale follow target between sync completion and parent snapshot propagation", () => {
    const gate = new MapFollowTargetGate();
    const oldTarget = createInitialViewport(mapView);
    const draggedViewport = dragViewport(oldTarget, 120, 80);
    const acknowledgedTarget = dragViewport(oldTarget, 122, 82);

    gate.beginSync(draggedViewport);
    gate.acknowledgeSyncSnapshot({
      following: true,
      targetViewport: acknowledgedTarget,
    });

    expect(gate.shouldApplyTarget(oldTarget)).toBe(false);
    expect(gate.awaitedViewport()).toEqual(acknowledgedTarget);
    expect(gate.shouldApplyTarget(acknowledgedTarget)).toBe(true);
    expect(gate.awaitedViewport()).toBeNull();
  });

  it("allows targets when no follow sync is pending", () => {
    const gate = new MapFollowTargetGate();
    const target = createInitialViewport(mapView);

    expect(gate.shouldApplyTarget(target)).toBe(true);
  });

  it("clears the pending target when follow disengages during sync", () => {
    const gate = new MapFollowTargetGate();
    const oldTarget = createInitialViewport(mapView);
    const draggedViewport = dragViewport(oldTarget, 120, 80);

    gate.beginSync(draggedViewport);
    gate.acknowledgeSyncSnapshot({
      following: false,
      targetViewport: null,
    });

    expect(gate.awaitedViewport()).toBeNull();
    expect(gate.shouldApplyTarget(oldTarget)).toBe(true);
  });
});
