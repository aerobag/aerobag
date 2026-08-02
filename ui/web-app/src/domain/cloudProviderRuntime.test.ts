// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import { afterEach, describe, expect, it, vi } from "vitest";

import { executeCloudHttpRequest } from "./cloudProviderRuntime";

afterEach(() => vi.unstubAllGlobals());

const authorization = {
  provider: "google_drive" as const,
  credential: "test-token",
  expiresAtEpochMs: Date.now() + 60_000,
};

describe("cloud provider HTTP effect", () => {
  it("executes only the core-planned request and returns opaque bytes", async () => {
    const fetchMock = vi.fn(async (_input: RequestInfo | URL, _init?: RequestInit) =>
      new Response(new Uint8Array([1, 2, 3]), { status: 206 }));
    vi.stubGlobal("fetch", fetchMock);

    await expect(executeCloudHttpRequest({
      request_id: 9,
      provider: "google_drive",
      method: "post",
      url: "https://example.invalid/core-owned-path",
      headers: [{ name: "Content-Type", value: "application/octet-stream" }],
      body_base64: "BAU",
      max_response_bytes: 16,
    }, authorization)).resolves.toEqual({
      result: "completed",
      status_code: 206,
      body_base64: "AQID",
    });

    const [url, init] = fetchMock.mock.calls[0] ?? [];
    expect(url).toBe("https://example.invalid/core-owned-path");
    expect((init?.headers as Headers).get("Authorization")).toBe("Bearer test-token");
    expect(Array.from(new Uint8Array(init?.body as ArrayBuffer))).toEqual([4, 5]);
  });

  it("stops reading a response that exceeds the core-owned limit", async () => {
    vi.stubGlobal("fetch", vi.fn(async () => new Response(new Uint8Array([1, 2, 3, 4]))));

    await expect(executeCloudHttpRequest({
      request_id: 10,
      provider: "google_drive",
      method: "get",
      url: "https://example.invalid/large",
      headers: [],
      body_base64: null,
      max_response_bytes: 3,
    }, authorization)).resolves.toEqual({ result: "response_too_large", limit_bytes: 3 });
  });
});
