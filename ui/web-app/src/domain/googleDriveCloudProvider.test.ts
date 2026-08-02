// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import { afterEach, describe, expect, it, vi } from "vitest";

import { readGoogleDrivePrincipal } from "./googleDriveCloudProvider";

afterEach(() => {
  vi.unstubAllGlobals();
});

describe("Google Drive cloud provider effects", () => {
  it("reports a stable provider principal separately from its display label", async () => {
    const fetchMock = vi.fn(async (_input: string | URL | Request) => new Response(JSON.stringify({
      user: {
        permissionId: "drive-principal-17",
        displayName: "Test Pilot",
        emailAddress: "pilot@example.com",
      },
    }), { status: 200, headers: { "content-type": "application/json" } }));
    vi.stubGlobal("fetch", fetchMock);

    await expect(readGoogleDrivePrincipal("token")).resolves.toEqual({
      stable_id: "drive-principal-17",
      display_label: "pilot@example.com",
    });
    expect(String(fetchMock.mock.calls[0]?.[0])).toContain("/about?fields=");
  });
});
