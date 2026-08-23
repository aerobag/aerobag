#!/usr/bin/env node

// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const symbols = JSON.parse(
  fs.readFileSync(path.join(repoRoot, "ui/shared-symbols/nav-symbols.json"), "utf8"),
);
const theme = JSON.parse(
  fs.readFileSync(path.join(repoRoot, "ui/shared-fixtures/ui-theme.json"), "utf8"),
);
const outputPath = path.resolve(process.argv[2] ?? "/tmp/action-icons-contact-sheet.svg");

const labels = {
  undo: "Undo",
  redo: "Redo",
  activate_next_leg: "Next",
  stop_navigation: "Stop",
  toggle_sequencing_suspension: "SUSP",
  restore_direct_to: "Restore",
  activate_leg: "Activate Leg",
  direct_to: "Direct",
  insert_before: "Insert Before",
  move_up: "Move Up",
  insert_after: "Insert After",
  move_down: "Move Down",
  remove: "Remove",
  remove_all_above: "Remove All Above",
  select_departure: "Select Departure",
  add_airway: "Add Airway",
  select_arrival: "Select Arrival",
  select_approach: "Select Approach",
  waypoint_info: "Airport Info",
  plates: "Plates",
  show_plate: "Show Plate",
  remove_procedure: "Remove Procedure",
  weather: "WX",
  insert: "Insert",
  remove_from_flight_plan: "Remove",
  csup: "CSUP",
  wx: "WX",
  airport_info: "Airport Info",
};

const paintColors = {
  none: "none",
  white: "#ffffff",
  button_icon: theme.controls.button_fg,
  button_icon_secondary: theme.controls.button_icon_secondary,
  flight_plan_guidance: theme.flight_plan_route.guidance_arrow,
};

function escapeXml(value) {
  return String(value)
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;");
}

function color(token) {
  if (token == null) return "none";
  const resolved = paintColors[token];
  if (resolved == null) {
    throw new Error(`contact sheet has no color for symbol paint token ${token}`);
  }
  return resolved;
}

function renderSymbol(symbolName, x, y) {
  const layers = symbols.symbols[symbolName];
  if (layers == null) {
    throw new Error(`missing shared symbol ${symbolName}`);
  }
  const paths = layers.map((layer) => {
    const pathData = symbols.paths[layer.path];
    if (pathData == null) {
      throw new Error(`missing shared path ${layer.path}`);
    }
    const attributes = [
      `d="${escapeXml(pathData)}"`,
      `fill="${color(layer.fill)}"`,
      `stroke="${color(layer.stroke)}"`,
    ];
    if (layer.stroke_width != null) attributes.push(`stroke-width="${layer.stroke_width}"`);
    if (layer.line_cap != null) attributes.push(`stroke-linecap="${layer.line_cap}"`);
    if (layer.line_join != null) attributes.push(`stroke-linejoin="${layer.line_join}"`);
    if (layer.transform_degrees != null) {
      attributes.push(`transform="rotate(${layer.transform_degrees})"`);
    }
    return `<path ${attributes.join(" ")} />`;
  });
  return `<g transform="translate(${x} ${y})">${paths.join("")}</g>`;
}

const entries = Object.entries(symbols.action_symbols);
const columns = 2;
const rows = Math.ceil(entries.length / columns);
const pagePadding = 28;
const titleHeight = 54;
const columnGap = 18;
const rowGap = 12;
const buttonWidth = 326;
const buttonHeight = 72;
const width = pagePadding * 2 + columns * buttonWidth + columnGap;
const height = pagePadding * 2 + titleHeight + rows * buttonHeight + (rows - 1) * rowGap;

const buttons = entries.map(([actionId, symbolName], index) => {
  const label = labels[actionId];
  if (label == null) {
    throw new Error(`contact sheet has no label for action ${actionId}`);
  }
  const column = index % columns;
  const row = Math.floor(index / columns);
  const x = pagePadding + column * (buttonWidth + columnGap);
  const y = pagePadding + titleHeight + row * (buttonHeight + rowGap);
  const iconCenterX = x + buttonWidth - 42;
  const iconCenterY = y + buttonHeight / 2;
  return `
    <g>
      <rect x="${x}" y="${y}" width="${buttonWidth}" height="${buttonHeight}" rx="8"
        fill="${theme.controls.button_unchecked}" stroke="#16263199" stroke-width="2" />
      <text x="${x + 18}" y="${y + buttonHeight / 2}" dominant-baseline="central"
        fill="${theme.controls.button_fg}" font-family="Avenir Next, Avenir, sans-serif"
        font-size="18" font-weight="700" letter-spacing="0.3">${escapeXml(label.toUpperCase())}</text>
      ${renderSymbol(symbolName, iconCenterX, iconCenterY)}
    </g>`;
}).join("");

const svg = `<?xml version="1.0" encoding="UTF-8"?>
<svg xmlns="http://www.w3.org/2000/svg" width="${width}" height="${height}" viewBox="0 0 ${width} ${height}">
  <rect width="${width}" height="${height}" fill="#aebcc5" />
  <text x="${pagePadding}" y="${pagePadding + 22}" fill="#18272f"
    font-family="Avenir Next, Avenir, sans-serif" font-size="20" font-weight="800"
    letter-spacing="0.7">ACTION SYMBOLS</text>
  <text x="${width - pagePadding}" y="${pagePadding + 22}" text-anchor="end" fill="#40545e"
    font-family="Avenir Next, Avenir, sans-serif" font-size="13" font-weight="600">48 PX SOURCE GEOMETRY</text>
  ${buttons}
</svg>
`;

fs.writeFileSync(outputPath, svg);
console.log(outputPath);
