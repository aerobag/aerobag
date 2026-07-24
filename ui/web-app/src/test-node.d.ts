// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

declare module "node:fs" {
  export function readFileSync(path: URL | string, encoding: string): string;
}
