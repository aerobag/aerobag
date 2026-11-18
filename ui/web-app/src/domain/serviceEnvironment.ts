// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

export type ServiceRuntime = {
  location?: { origin?: string | null; pathname?: string | null } | null;
  window?: { location?: { origin?: string | null; pathname?: string | null } | null } | null;
};

/** Deployment mount selects staging; the immutable app still names its contract. */
export function serviceEnvironmentUrl(configured: string, runtime: ServiceRuntime): string {
  const location = runtime.window?.location ?? runtime.location;
  if (location?.pathname?.startsWith("/staging/") && location.origin) {
    const url = new URL(configured, location.origin);
    if (/^\/contracts\/C[1-9][0-9]*(\/|$)/.test(url.pathname)) {
      return `${location.origin}/staging${url.pathname}`.replace(/\/+$/, "");
    }
  }
  return configured.replace(/\/+$/, "");
}
