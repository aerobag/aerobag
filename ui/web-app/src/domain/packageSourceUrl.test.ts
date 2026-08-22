// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import { describe, expect, it } from "vitest";

import { resolvePackageSourceBaseUrl } from "./packageSourceUrl";

describe("resolvePackageSourceBaseUrl", () => {
  it("uses a release-scoped build value without consulting the channel URL", () => {
    expect(resolvePackageSourceBaseUrl(
      "https://aerobag.org/releases/2026-08-22.1/packages/",
      { location: { origin: "https://aerobag.org" } },
    )).toBe("https://aerobag.org/releases/2026-08-22.1/packages");
  });

  it("uses the same-origin production path for development builds", () => {
    expect(resolvePackageSourceBaseUrl(null, {
      location: { origin: "http://localhost:8084" },
    })).toBe("http://localhost:8084/packages");
  });
});
