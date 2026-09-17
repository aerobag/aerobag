// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import { readFileSync } from "node:fs";

// Read the generated core enum, using the same snake_case -> PascalCase naming
// convention as the Kotlin wire generator. New layers need no platform table.
const schema = JSON.parse(readFileSync(new URL("../../ui/core-rust/schemas/session-page-wire.schema.json", import.meta.url), "utf8"));
const names = new Map(schema.$defs.MapLayerId.enum.map(id => [id,
  id.split("_").map(part => part[0].toUpperCase() + part.slice(1)).join("")]));

export function androidMapLayerName(id) {
  return names.get(id);
}
