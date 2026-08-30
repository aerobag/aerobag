// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

const releaseWebPath = /^\/releases\/[A-Za-z0-9][A-Za-z0-9._-]{0,79}\/web(?:\/|$)/;

export function appRouteBase(pathname: string): string {
  if (pathname === "/staging" || pathname.startsWith("/staging/")) {
    return "/staging";
  }
  const release = pathname.match(releaseWebPath)?.[0]?.replace(/\/$/, "");
  return release ?? "";
}

export function appPageUrl(page: string, pathname: string): string {
  const base = appRouteBase(pathname);
  return page === "about" ? `${base}/about` : `${base}/`;
}
