// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import { describe, expect, it } from "vitest";

import { appPageUrl } from "./webRouteUrl";

describe("release-aware web routes", () => {
  it.each([
    ["/", "/about", "/"],
    ["/staging/", "/staging/about", "/staging/"],
    [
      "/releases/2026-08-22.1/web/",
      "/releases/2026-08-22.1/web/about",
      "/releases/2026-08-22.1/web/",
    ],
  ])("keeps navigation in the current serving view", (current, about, home) => {
    expect(appPageUrl("about", current)).toBe(about);
    expect(appPageUrl("home", about)).toBe(home);
  });
});
