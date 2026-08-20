// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import { describe, expect, it, vi } from "vitest";

import type { NexradOverlayCachePlan } from "../generated/nexradOverlayWire";
import { NexradFrameImageCache } from "./nexradFrameCache";

function plan(
  retainedFrameVersions: string[],
  resources: Array<[frameVersion: string, src: string]>,
): NexradOverlayCachePlan {
  return {
    retained_frame_versions: retainedFrameVersions,
    fetch_resources: resources.map(([frame_version, src]) => ({ frame_version, src })),
  };
}

describe("NEXRAD frame image cache", () => {
  it("does not fetch while hidden and reuses fresh frames after toggling back on", async () => {
    const loadBlob = vi.fn(async (src: string) => new Blob([src]));
    const revokeObjectUrl = vi.fn();
    let nextObjectUrl = 0;
    const cache = new NexradFrameImageCache(
      loadBlob,
      () => `blob:nexrad-${++nextObjectUrl}`,
      revokeObjectUrl,
    );

    await expect(cache.applyPlan(plan(["v1", "v2"], []))).resolves.toEqual({ loaded: 0, failed: 0 });
    expect(loadBlob).not.toHaveBeenCalled();

    await expect(cache.applyPlan(plan(["v1", "v2"], [
      ["v1", "/v1/tile.png"],
      ["v2", "/v2/tile.png"],
    ]))).resolves.toEqual({ loaded: 2, failed: 0 });
    expect(loadBlob).toHaveBeenCalledTimes(2);
    expect(cache.imageUrlFor("/v1/tile.png")).toBe("blob:nexrad-1");
    expect(cache.imageUrlFor("/v2/tile.png")).toBe("blob:nexrad-2");

    await cache.applyPlan(plan(["v1", "v2"], []));
    expect(cache.imageUrlFor("/v1/tile.png")).toBe("blob:nexrad-1");
    expect(revokeObjectUrl).not.toHaveBeenCalled();

    await cache.applyPlan(plan(["v1", "v2", "v3"], [
      ["v1", "/v1/tile.png"],
      ["v2", "/v2/tile.png"],
      ["v3", "/v3/tile.png"],
    ]));
    expect(loadBlob).toHaveBeenCalledTimes(3);
    expect(cache.imageUrlFor("/v3/tile.png")).toBe("blob:nexrad-3");
  });

  it("revokes frames after core removes them from the animation window", async () => {
    const revokeObjectUrl = vi.fn();
    let nextObjectUrl = 0;
    const cache = new NexradFrameImageCache(
      async (src) => new Blob([src]),
      () => `blob:nexrad-${++nextObjectUrl}`,
      revokeObjectUrl,
    );
    await cache.applyPlan(plan(["old", "fresh"], [
      ["old", "/old/tile.png"],
      ["fresh", "/fresh/tile.png"],
    ]));

    await cache.applyPlan(plan(["fresh", "new"], [
      ["fresh", "/fresh/tile.png"],
      ["new", "/new/tile.png"],
    ]));

    expect(cache.imageUrlFor("/old/tile.png")).toBeNull();
    expect(cache.imageUrlFor("/fresh/tile.png")).toBe("blob:nexrad-2");
    expect(cache.imageUrlFor("/new/tile.png")).toBe("blob:nexrad-3");
    expect(revokeObjectUrl).toHaveBeenCalledExactlyOnceWith("blob:nexrad-1");
  });

  it("releases a pending image that finishes after its frame was pruned", async () => {
    let finishLoad: (blob: Blob) => void = () => {
      throw new Error("pending NEXRAD load was not initialized");
    };
    const pendingBlob = new Promise<Blob>((resolve) => { finishLoad = resolve; });
    const revokeObjectUrl = vi.fn();
    const cache = new NexradFrameImageCache(
      () => pendingBlob,
      () => "blob:late",
      revokeObjectUrl,
    );

    const pending = cache.applyPlan(plan(["v1"], [["v1", "/v1/tile.png"]]));
    await cache.applyPlan(plan([], []));
    finishLoad(new Blob(["late"]));
    await pending;

    expect(cache.imageUrlFor("/v1/tile.png")).toBeNull();
    expect(revokeObjectUrl).toHaveBeenCalledWith("blob:late");
  });

  it("cancels pending transfers without discarding completed fresh frames", async () => {
    const pendingSignal: { current: AbortSignal | null } = { current: null };
    const cache = new NexradFrameImageCache(
      (src, signal) => {
        if (src === "/fresh/tile.png") {
          return Promise.resolve(new Blob([src]));
        }
        pendingSignal.current = signal;
        return new Promise((_resolve, reject) => {
          signal.addEventListener("abort", () => reject(new DOMException("aborted", "AbortError")));
        });
      },
      () => "blob:fresh",
      () => {},
    );
    await cache.applyPlan(plan(["fresh"], [["fresh", "/fresh/tile.png"]]));
    const pending = cache.applyPlan(plan(["fresh", "new"], [["new", "/new/tile.png"]]));

    cache.cancelPendingLoads();
    await expect(pending).resolves.toEqual({ loaded: 0, failed: 1 });

    expect(pendingSignal.current?.aborted).toBe(true);
    expect(cache.imageUrlFor("/fresh/tile.png")).toBe("blob:fresh");
    expect(cache.imageUrlFor("/new/tile.png")).toBeNull();
  });
});
