// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

export const RASTER_TILE_LOAD_RECOVERY_DELAY_MS = 1_500;
export const RASTER_TILE_LOAD_RETRY_LIMIT = 1;

const URL_PARSE_BASE = "https://aerobag.invalid/";

function rewriteRasterTileUrl(source: string, rewrite: (url: URL) => void): string {
  const url = new URL(source, URL_PARSE_BASE);
  rewrite(url);
  if (/^[a-z][a-z0-9+.-]*:/i.test(source)) {
    return url.toString();
  }
  if (source.startsWith("//")) {
    return `//${url.host}${url.pathname}${url.search}${url.hash}`;
  }
  const path = `${url.pathname}${url.search}${url.hash}`;
  return source.startsWith("/") ? path : path.slice(1);
}

export function rasterTileLoadUrl(source: string, attempt: number): string {
  if (attempt <= 0) {
    return source;
  }
  return rewriteRasterTileUrl(source, (url) => {
    url.searchParams.set("aerobag_retry", String(attempt));
  });
}

export function e2eRasterTileStallUrl(source: string): string {
  return rewriteRasterTileUrl(source, (url) => {
    url.pathname = `${url.pathname}.aerobag-e2e-stall`;
  });
}
