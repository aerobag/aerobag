// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import { describe, expect, it, vi } from "vitest";

import { fetchTextResource } from "./fetchTextResource";

describe("fetchTextResource", () => {
  it("recovers one transient transport failure within the user action", async () => {
    const fetchResource = vi.fn()
      .mockRejectedValueOnce(new TypeError("request was aborted"))
      .mockResolvedValueOnce(new Response("trace body", { status: 200 }));

    await expect(fetchTextResource("/trace.json", fetchResource)).resolves.toEqual({
      text: "trace body",
      attempts: 2,
    });
    expect(fetchResource).toHaveBeenCalledTimes(2);
  });

  it("also recovers a transport failure while reading the response body", async () => {
    const brokenResponse = {
      ok: true,
      status: 200,
      text: vi.fn().mockRejectedValue(new TypeError("body stream aborted")),
    } as unknown as Response;
    const fetchResource = vi.fn()
      .mockResolvedValueOnce(brokenResponse)
      .mockResolvedValueOnce(new Response("trace body", { status: 200 }));

    await expect(fetchTextResource("/trace.json", fetchResource)).resolves.toEqual({
      text: "trace body",
      attempts: 2,
    });
    expect(fetchResource).toHaveBeenCalledTimes(2);
  });

  it("does not retry a completed HTTP error response", async () => {
    const fetchResource = vi.fn().mockResolvedValue(
      new Response("missing", { status: 404 }),
    );

    await expect(fetchTextResource("/missing.json", fetchResource)).rejects.toThrow(
      "failed to fetch /missing.json: HTTP 404",
    );
    expect(fetchResource).toHaveBeenCalledOnce();
  });

  it("reports terminal transport failure after exactly two attempts", async () => {
    const fetchResource = vi.fn()
      .mockRejectedValueOnce(new TypeError("first abort"))
      .mockRejectedValueOnce(new TypeError("second abort"));

    await expect(fetchTextResource("/trace.json", fetchResource)).rejects.toThrow(
      "failed to fetch /trace.json after 2 attempts: second abort",
    );
    expect(fetchResource).toHaveBeenCalledTimes(2);
  });
});
