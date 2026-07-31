// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import { afterEach, describe, expect, it, vi } from "vitest";

import { executeGoogleDriveCloudRequest } from "./googleDriveCloudProvider";

afterEach(() => {
  vi.unstubAllGlobals();
});

describe("Google Drive cloud provider effects", () => {
  it("maps paginated appDataFolder listings into the provider-neutral contract", async () => {
    const fetchMock = vi.fn(async (_input: string | URL | Request) => new Response(JSON.stringify({
      files: [{ id: "object-1", size: "37", createdTime: "2026-07-31T12:00:00Z" }],
      nextPageToken: "next page",
    }), { status: 200, headers: { "content-type": "application/json" } }));
    vi.stubGlobal("fetch", fetchMock);

    const response = await executeGoogleDriveCloudRequest({
      request_id: 1,
      provider: "google_drive",
      operation: { operation: "list", page_token: "current page" },
    }, "token");

    expect(response).toEqual({
      result: "listed",
      objects: [{
        id: "object-1",
        size_bytes: 37,
        created_at: "2026-07-31T12:00:00Z",
      }],
      next_page_token: "next page",
    });
    const url = new URL(String(fetchMock.mock.calls[0]?.[0]));
    expect(url.searchParams.get("spaces")).toBe("appDataFolder");
    expect(url.searchParams.get("pageToken")).toBe("current page");
  });

  it("distinguishes deleting a missing immutable object", async () => {
    vi.stubGlobal("fetch", vi.fn(async () => new Response(null, { status: 404 })));

    const response = await executeGoogleDriveCloudRequest({
      request_id: 2,
      provider: "google_drive",
      operation: { operation: "delete", id: "missing" },
    }, "token");

    expect(response).toEqual({ result: "deleted", existed: false });
  });

  it("maps occupied generated IDs to create-once contention", async () => {
    vi.stubGlobal("fetch", vi.fn(async () => new Response(null, { status: 409 })));
    vi.stubGlobal("crypto", { randomUUID: () => "00000000-0000-4000-8000-000000000000" });

    const response = await executeGoogleDriveCloudRequest({
      request_id: 3,
      provider: "google_drive",
      operation: {
        operation: "create_once",
        id: "occupied",
        name: "state",
        bytes_base64: "AA",
      },
    }, "token");

    expect(response).toEqual({ result: "already_exists" });
  });
});
