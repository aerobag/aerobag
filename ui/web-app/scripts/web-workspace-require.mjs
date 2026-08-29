// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import { createRequire } from "node:module";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";

const SOURCE_REPO_ROOT = resolve(fileURLToPath(new URL("../../..", import.meta.url)));

export function webWorkspaceDirectory(environment = process.env, repoRoot = SOURCE_REPO_ROOT) {
  if (environment.AEROBAG_WEB_WORKSPACE_DIR) {
    return resolve(environment.AEROBAG_WEB_WORKSPACE_DIR);
  }
  return resolve(repoRoot, "ui", "web-app");
}

export function requireWebDependency(name, environment = process.env) {
  const require = createRequire(resolve(webWorkspaceDirectory(environment), "package.json"));
  return require(name);
}
