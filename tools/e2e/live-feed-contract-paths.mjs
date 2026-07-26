// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

export const LIVE_FEED_SCHEMA_VERSION = 3;
export const LIVE_FEED_ROOT = `/live-feeds/v${LIVE_FEED_SCHEMA_VERSION}`;

export function liveFeedPath(relativePath) {
  return `${LIVE_FEED_ROOT}/${relativePath}`;
}

export function metarVersionFromPath(pathname, kind) {
  const prefix = liveFeedPath(`${kind}/metars/`);
  if (!pathname.startsWith(prefix) || !pathname.endsWith(".json")) {
    return null;
  }
  const version = pathname.slice(prefix.length, -".json".length);
  return version && !version.includes("/") ? version : null;
}
