// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

beforeEach(() => {
  vi.useFakeTimers();
  vi.resetModules();
  vi.stubGlobal("location", { protocol: "http:", href: "http://app.example.test/" });
});

afterEach(() => {
  vi.useRealTimers();
  vi.unstubAllGlobals();
});

describe("debug log upload", () => {
  it("bounds queued diagnostics while the uploader is enabled", async () => {
    const uploaded: unknown[] = [];
    vi.stubGlobal("fetch", vi.fn(async (_url: string, init?: RequestInit) => {
      uploaded.push(...JSON.parse(String(init?.body)) as unknown[]);
      return new Response(null, { status: 204 });
    }));
    const logging = await import("./debugLog");
    logging.setDebugLogDeveloperServerUploadEnabled(true);

    for (let index = 0; index < 10_100; index += 1) {
      logging.debugLog("bounded", { index });
    }
    await vi.advanceTimersByTimeAsync(10_000);

    expect(uploaded).toHaveLength(10_000);
    logging.setDebugLogDeveloperServerUploadEnabled(false);
  });

  it("backs off after a failed upload instead of retrying every 250ms", async () => {
    const fetchMock = vi.fn(async () => {
      throw new Error("sink unavailable");
    });
    vi.stubGlobal("fetch", fetchMock);
    const logging = await import("./debugLog");
    logging.setDebugLogDeveloperServerUploadEnabled(true);
    logging.debugLog("retry-me");

    await vi.advanceTimersByTimeAsync(250);
    expect(fetchMock).toHaveBeenCalledTimes(1);
    await vi.advanceTimersByTimeAsync(499);
    expect(fetchMock).toHaveBeenCalledTimes(1);
    await vi.advanceTimersByTimeAsync(1);
    expect(fetchMock).toHaveBeenCalledTimes(2);
    logging.setDebugLogDeveloperServerUploadEnabled(false);
  });
});
