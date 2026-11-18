// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

declare const __AEROBAG_PACKAGE_SOURCE_BASE_URL__: string | null;
import { serviceEnvironmentUrl, type ServiceRuntime } from "./serviceEnvironment";

export type PackageSourceRuntime = ServiceRuntime;

export function resolvePackageSourceBaseUrl(
  configuredBaseUrl: string | null | undefined,
  runtime: PackageSourceRuntime = globalThis as unknown as PackageSourceRuntime,
): string {
  const configured = configuredBaseUrl?.trim();
  if (configured) {
    return serviceEnvironmentUrl(configured, runtime);
  }
  const origin = runtime.location?.origin?.replace(/\/+$/, "") ?? "";
  return origin ? `${origin}/packages` : "/packages";
}

export function packageSourceBaseUrl(): string {
  return resolvePackageSourceBaseUrl(__AEROBAG_PACKAGE_SOURCE_BASE_URL__);
}
