// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import { describe, expect, it, vi } from "vitest";

import { performCloudUiActionWithPlatformEffect } from "./cloudUiAction";

describe("cloud UI platform effects", () => {
  it("starts clipboard access before waiting for the core worker", async () => {
    let finishCore = (_snapshot: string): void => {
      throw new Error("core resolver was not installed");
    };
    const order: string[] = [];
    const coreResult = new Promise<string>((resolve) => { finishCore = resolve; });
    const writeClipboard = vi.fn(async () => { order.push("clipboard"); });

    const action = performCloudUiActionWithPlatformEffect({
      platformEffect: {
        kind: "copy_text",
        text: "AB3.example",
        completion_label: "Copied",
      },
      performCoreAction: () => {
        order.push("core");
        return coreResult;
      },
      applySnapshot: () => { order.push("snapshot"); },
      pumpCloudProvider: async () => { order.push("pump"); },
      writeClipboard,
    });

    expect(order).toEqual(["clipboard", "core"]);
    finishCore("snapshot");
    await expect(action).resolves.toBe("Copied");
    expect(order).toEqual(["clipboard", "core", "snapshot"]);
    expect(writeClipboard).toHaveBeenCalledWith("AB3.example");
  });

  it("surfaces clipboard rejection after applying the core snapshot", async () => {
    const error = new Error("clipboard denied");
    let applied = false;
    const action = performCloudUiActionWithPlatformEffect({
      platformEffect: {
        kind: "copy_text",
        text: "AB3.example",
        completion_label: "Copied",
      },
      performCoreAction: async () => "snapshot",
      applySnapshot: () => { applied = true; },
      pumpCloudProvider: async () => {},
      writeClipboard: async () => { throw error; },
    });

    await expect(action).rejects.toBe(error);
    expect(applied).toBe(true);
  });
});
