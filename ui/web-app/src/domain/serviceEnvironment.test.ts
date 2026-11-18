// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later
import { expect, it } from "vitest";
import { resolvePackageSourceBaseUrl } from "./packageSourceUrl";
import { resolveLiveFeedSourceUrl } from "./liveFeedUrls";

it("selects the same contract environment for package and live-feed requests", () => {
  for (const [pathname, prefix] of [["/", ""], ["/staging/", "/staging"], ["/staging/about", "/staging"]]) {
    const runtime = { location: { origin: "https://aerobag.org", pathname } };
    expect(resolvePackageSourceBaseUrl("https://aerobag.org/contracts/C1/packages/", runtime))
      .toBe(`https://aerobag.org${prefix}/contracts/C1/packages`);
    expect(resolveLiveFeedSourceUrl("https://aerobag.org/contracts/C1", runtime))
      .toBe(`https://aerobag.org${prefix}/contracts/C1`);
  }
});

it("does not infer contract routes for legacy or development URLs", () => {
  const runtime = { location: { origin: "https://aerobag.org", pathname: "/staging/" } };
  expect(resolvePackageSourceBaseUrl("https://aerobag.org/releases/old/packages/", runtime))
    .toBe("https://aerobag.org/releases/old/packages");
  expect(resolveLiveFeedSourceUrl("http://localhost:18093", runtime)).toBe("http://localhost:18093");
});
