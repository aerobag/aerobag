#!/usr/bin/env node

import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const specPath = path.join(repoRoot, "ui/shared-symbols/nav-symbols.json");
const cwdPackageJsonPath = path.join(process.cwd(), "package.json");

const args = new Map();
for (let index = 2; index < process.argv.length; index += 2) {
  args.set(process.argv[index], process.argv[index + 1]);
}

function defaultWebOutPath() {
  if (fs.existsSync(cwdPackageJsonPath)) {
    try {
      const packageJson = JSON.parse(fs.readFileSync(cwdPackageJsonPath, "utf8"));
      if (packageJson.name === "aerobag-web") {
        return path.join(process.cwd(), "src/generated/navSymbols.ts");
      }
    } catch {
      // Fall through to the source-tree default.
    }
  }
  return path.join(repoRoot, "ui/web-app/src/generated/navSymbols.ts");
}

const webOut = args.get("--web-out") ?? defaultWebOutPath();
const androidOut =
  args.get("--android-out") ??
  path.join(
    repoRoot,
    "ui/android-app/app/build/generated/aerobagSymbols/kotlin/net/jonh/aerobag/prototype/generated",
  );

const spec = JSON.parse(fs.readFileSync(specPath, "utf8"));

function fmtNumber(value) {
  if (Object.is(value, -0)) {
    return "0";
  }
  const rounded = Math.round(value * 1000) / 1000;
  return Number.isInteger(rounded) ? `${rounded}` : `${rounded}`;
}

function pathFromPoints(points) {
  return points.map(([x, y], index) => `${index === 0 ? "M" : "L"} ${fmtNumber(x)} ${fmtNumber(y)}`).join(" ") + " Z";
}

function polygonSignedArea(points) {
  let area = 0;
  for (let index = 0; index < points.length; index += 1) {
    const [x, y] = points[index];
    const [nextX, nextY] = points[(index + 1) % points.length];
    area += x * nextY - nextX * y;
  }
  return area / 2;
}

function intersectLines(originA, directionA, originB, directionB) {
  const cross = directionA[0] * directionB[1] - directionA[1] * directionB[0];
  if (Math.abs(cross) < 1e-6) {
    return originA;
  }
  const deltaX = originB[0] - originA[0];
  const deltaY = originB[1] - originA[1];
  const t = (deltaX * directionB[1] - deltaY * directionB[0]) / cross;
  return [originA[0] + directionA[0] * t, originA[1] + directionA[1] * t];
}

function offsetPolygonByEdgeDistances(points, edgeDistances) {
  const signedArea = polygonSignedArea(points);
  const inwardNormalForEdge = (from, to, distance) => {
    const dx = to[0] - from[0];
    const dy = to[1] - from[1];
    const length = Math.hypot(dx, dy) || 1;
    return signedArea > 0 ? [(dy / length) * distance, (-dx / length) * distance] : [(-dy / length) * distance, (dx / length) * distance];
  };
  return points.map((point, index) => {
    const prevIndex = (index + points.length - 1) % points.length;
    const nextIndex = (index + 1) % points.length;
    const prevPoint = points[prevIndex];
    const nextPoint = points[nextIndex];
    const prevShift = inwardNormalForEdge(prevPoint, point, edgeDistances[prevIndex]);
    const nextShift = inwardNormalForEdge(point, nextPoint, edgeDistances[index]);
    return intersectLines(
      [prevPoint[0] + prevShift[0], prevPoint[1] + prevShift[1]],
      [point[0] - prevPoint[0], point[1] - prevPoint[1]],
      [point[0] + nextShift[0], point[1] + nextShift[1]],
      [nextPoint[0] - point[0], nextPoint[1] - point[1]],
    );
  });
}

function ktFloat(value) {
  return `${fmtNumber(value)}f`;
}

function parsePathCommands(pathData) {
  const tokens = pathData.match(/[A-Za-z]|-?(?:\d+\.?\d*|\.\d+)/g) ?? [];
  const commands = [];
  let index = 0;
  const readNumber = () => {
    const token = tokens[index++];
    if (token == null || /[A-Za-z]/.test(token)) {
      throw new Error(`expected number in path ${pathData}`);
    }
    return Number(token);
  };
  while (index < tokens.length) {
    const command = tokens[index++];
    if (!/[A-Za-z]/.test(command)) {
      throw new Error(`expected command in path ${pathData}`);
    }
    switch (command) {
      case "M":
      case "L":
        commands.push({ command, values: [readNumber(), readNumber()] });
        break;
      case "H":
      case "V":
        commands.push({ command, values: [readNumber()] });
        break;
      case "Q":
        commands.push({ command, values: [readNumber(), readNumber(), readNumber(), readNumber()] });
        break;
      case "C":
        commands.push({
          command,
          values: [readNumber(), readNumber(), readNumber(), readNumber(), readNumber(), readNumber()],
        });
        break;
      case "Z":
        commands.push({ command, values: [] });
        break;
      default:
        throw new Error(`unsupported generated Compose path command ${command}`);
    }
  }
  return commands;
}

function ktPathCommands(pathData) {
  return parsePathCommands(pathData)
    .map(({ command, values }) => `SymbolPathCommand("${command}", listOf(${values.map(ktFloat).join(", ")}))`)
    .join(",\n    ");
}

function ktPointArray(points) {
  return points.map(([x, y]) => `SymbolPoint(${ktFloat(x)}, ${ktFloat(y)})`).join(",\n    ");
}

function symbolSource(name) {
  return (spec.symbols[name] ?? []).map((layer) => ({
    path: spec.paths[layer.path],
    paint: layer.paint,
    fill: layer.fill ?? null,
    stroke: layer.stroke ?? null,
    stroke_width: layer.stroke_width ?? null,
    line_cap: layer.line_cap ?? null,
    line_join: layer.line_join ?? null,
    transform_degrees: layer.transform_degrees ?? null,
  }));
}

function ktSymbolLayers(name) {
  return symbolSource(name)
    .map(
      (layer) =>
        `NavSymbolLayer(
        path = symbolPath(listOf(
            ${ktPathCommands(layer.path).replaceAll("\n", "\n            ")}
        ), center, scale),
        paint = "${layer.paint}",
        fill = ${layer.fill == null ? "null" : JSON.stringify(layer.fill)},
        stroke = ${layer.stroke == null ? "null" : JSON.stringify(layer.stroke)},
        strokeWidth = ${layer.stroke_width == null ? "null" : ktFloat(layer.stroke_width)},
        lineCap = ${layer.line_cap == null ? "null" : JSON.stringify(layer.line_cap)},
        lineJoin = ${layer.line_join == null ? "null" : JSON.stringify(layer.line_join)},
        transformDegrees = ${layer.transform_degrees == null ? "null" : ktFloat(layer.transform_degrees)},
    )`,
    )
    .join(",\n    ");
}

const vorOuterHexPoints = spec.vor.outer_hex_points;
const vorInnerHexPoints = offsetPolygonByEdgeDistances(vorOuterHexPoints, spec.vor.edge_inset_distances);
const vorOuterHexPath = pathFromPoints(vorOuterHexPoints);
const vorInnerHexPath = pathFromPoints(vorInnerHexPoints);
const vorBandPath = `${vorOuterHexPath} ${vorInnerHexPath}`;

const generatedBanner = "// @generated by tools/generate-nav-symbols.mjs from ui/shared-symbols/nav-symbols.json\n";

const webSource = `${generatedBanner}
export type NavSymbolLayer = {
  path: string;
  paint: string;
  fill?: string | null;
  stroke?: string | null;
  stroke_width?: number | null;
  line_cap?: string | null;
  line_join?: string | null;
  transform_degrees?: number | null;
};

export const airportCircleMarkerPath = ${JSON.stringify(spec.paths.airport_circle_marker)};
export const airportFuelMarkerPath = ${JSON.stringify(spec.paths.airport_fuel_marker)};
export const fixTrianglePath = ${JSON.stringify(spec.paths.fix_triangle)};
export const heliportHPath = ${JSON.stringify(spec.paths.heliport_h)};
export const seaplaneAnchorPath = ${JSON.stringify(spec.paths.seaplane_anchor)};
export const obstacleShortPath = ${JSON.stringify(spec.paths.obstacle_short)};
export const obstacleTallPath = ${JSON.stringify(spec.paths.obstacle_tall)};
export const obstacleShortDotY = ${JSON.stringify(spec.obstacle_dot.short_y)};
export const obstacleTallDotY = ${JSON.stringify(spec.obstacle_dot.tall_y)};
export const obstacleDotRadius = ${JSON.stringify(spec.obstacle_dot.radius)};
export const mapSelectionSpotPegPath = ${JSON.stringify(spec.paths.map_selection_spot_peg)};
export const mapSelectionSpotDotPath = ${JSON.stringify(spec.paths.map_selection_spot_dot)};
export const airportOpenMarkerSymbol = ${JSON.stringify(symbolSource("airport_open_marker"), null, 2)} satisfies readonly NavSymbolLayer[];
export const mapSelectionSpotSymbol = ${JSON.stringify(symbolSource("map_selection_spot"), null, 2)} satisfies readonly NavSymbolLayer[];
export const metarClearSymbol = ${JSON.stringify(symbolSource("metar_clear"), null, 2)} satisfies readonly NavSymbolLayer[];
export const metarFewSymbol = ${JSON.stringify(symbolSource("metar_few"), null, 2)} satisfies readonly NavSymbolLayer[];
export const metarSctSymbol = ${JSON.stringify(symbolSource("metar_sct"), null, 2)} satisfies readonly NavSymbolLayer[];
export const metarBknSymbol = ${JSON.stringify(symbolSource("metar_bkn"), null, 2)} satisfies readonly NavSymbolLayer[];
export const metarOvcSymbol = ${JSON.stringify(symbolSource("metar_ovc"), null, 2)} satisfies readonly NavSymbolLayer[];
export const metarMissingSymbol = ${JSON.stringify(symbolSource("metar_missing"), null, 2)} satisfies readonly NavSymbolLayer[];
export const pirepGenericSymbol = ${JSON.stringify(symbolSource("pirep_generic"), null, 2)} satisfies readonly NavSymbolLayer[];
export const pirepLightTurbulenceSymbol = ${JSON.stringify(symbolSource("pirep_light_turbulence"), null, 2)} satisfies readonly NavSymbolLayer[];
export const pirepModerateTurbulenceSymbol = ${JSON.stringify(symbolSource("pirep_moderate_turbulence"), null, 2)} satisfies readonly NavSymbolLayer[];
export const pirepSevereTurbulenceSymbol = ${JSON.stringify(symbolSource("pirep_severe_turbulence"), null, 2)} satisfies readonly NavSymbolLayer[];
export const pirepLightIcingSymbol = ${JSON.stringify(symbolSource("pirep_light_icing"), null, 2)} satisfies readonly NavSymbolLayer[];
export const pirepModerateIcingSymbol = ${JSON.stringify(symbolSource("pirep_moderate_icing"), null, 2)} satisfies readonly NavSymbolLayer[];
export const pirepSevereIcingSymbol = ${JSON.stringify(symbolSource("pirep_severe_icing"), null, 2)} satisfies readonly NavSymbolLayer[];
export const vorOuterHexPath = ${JSON.stringify(vorOuterHexPath)};
export const vorBandPath = ${JSON.stringify(vorBandPath)};
`;

const fuel = spec.airport_fuel_marker;
const androidSource = `${generatedBanner.replace("//", "//")}
package net.jonh.aerobag.prototype.generated

import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.PathFillType

private data class SymbolPoint(val x: Float, val y: Float)
private data class SymbolPathCommand(val command: String, val values: List<Float>)
data class NavSymbolLayer(
    val path: Path,
    val paint: String,
    val fill: String?,
    val stroke: String?,
    val strokeWidth: Float?,
    val lineCap: String?,
    val lineJoin: String?,
    val transformDegrees: Float?,
)

private val airportCircleCommands = listOf(
    ${ktPathCommands(spec.paths.airport_circle_marker)}
)

private val heliportHCommands = listOf(
    ${ktPathCommands(spec.paths.heliport_h)}
)

private val seaplaneAnchorCommands = listOf(
    ${ktPathCommands(spec.paths.seaplane_anchor)}
)

private val obstacleShortCommands = listOf(
    ${ktPathCommands(spec.paths.obstacle_short)}
)

private val obstacleTallCommands = listOf(
    ${ktPathCommands(spec.paths.obstacle_tall)}
)

private val vorOuterHexPoints = listOf(
    ${ktPointArray(vorOuterHexPoints)}
)

private val vorInnerHexPoints = listOf(
    ${ktPointArray(vorInnerHexPoints)}
)

private fun polygonPath(points: List<SymbolPoint>, center: Offset, scale: Float): Path =
    Path().apply {
        if (points.isEmpty()) return@apply
        moveTo(center.x + points.first().x * scale, center.y + points.first().y * scale)
        points.drop(1).forEach { point ->
            lineTo(center.x + point.x * scale, center.y + point.y * scale)
        }
        close()
    }

private fun symbolPath(commands: List<SymbolPathCommand>, center: Offset, scale: Float): Path =
    Path().apply {
        var currentX = 0f
        var currentY = 0f
        commands.forEach { command ->
            val values = command.values
            when (command.command) {
                "M" -> {
                    currentX = values[0]
                    currentY = values[1]
                    moveTo(center.x + currentX * scale, center.y + currentY * scale)
                }
                "L" -> {
                    currentX = values[0]
                    currentY = values[1]
                    lineTo(center.x + currentX * scale, center.y + currentY * scale)
                }
                "H" -> {
                    currentX = values[0]
                    lineTo(center.x + currentX * scale, center.y + currentY * scale)
                }
                "V" -> {
                    currentY = values[0]
                    lineTo(center.x + currentX * scale, center.y + currentY * scale)
                }
                "Q" -> {
                    currentX = values[2]
                    currentY = values[3]
                    quadraticTo(
                        center.x + values[0] * scale,
                        center.y + values[1] * scale,
                        center.x + currentX * scale,
                        center.y + currentY * scale,
                    )
                }
                "C" -> {
                    currentX = values[4]
                    currentY = values[5]
                    cubicTo(
                        center.x + values[0] * scale,
                        center.y + values[1] * scale,
                        center.x + values[2] * scale,
                        center.y + values[3] * scale,
                        center.x + currentX * scale,
                        center.y + currentY * scale,
                    )
                }
                "Z" -> close()
            }
        }
    }

fun airportCircleMarkerPath(center: Offset, scale: Float): Path =
    symbolPath(airportCircleCommands, center, scale)

fun airportFuelMarkerPath(center: Offset, scale: Float): Path {
    val circleRadius = ${ktFloat(fuel.circle_radius)} * scale
    val tabHalf = ${ktFloat(fuel.tab_half)} * scale
    val tabOuter = ${ktFloat(fuel.tab_outer)} * scale
    val arcJoin = ${ktFloat(fuel.arc_join)} * scale
    val circleBounds = Rect(
        left = center.x - circleRadius,
        top = center.y - circleRadius,
        right = center.x + circleRadius,
        bottom = center.y + circleRadius,
    )
    val arcStart = ${ktFloat(fuel.arc_start_deg)}
    val arcSweep = ${ktFloat(fuel.arc_sweep_deg)}
    return Path().apply {
        moveTo(center.x - tabHalf, center.y - tabOuter)
        lineTo(center.x + tabHalf, center.y - tabOuter)
        lineTo(center.x + tabHalf, center.y - arcJoin)
        arcTo(circleBounds, arcStart, arcSweep, false)
        lineTo(center.x + tabOuter, center.y - tabHalf)
        lineTo(center.x + tabOuter, center.y + tabHalf)
        lineTo(center.x + arcJoin, center.y + tabHalf)
        arcTo(circleBounds, arcStart + 90f, arcSweep, false)
        lineTo(center.x + tabHalf, center.y + tabOuter)
        lineTo(center.x - tabHalf, center.y + tabOuter)
        lineTo(center.x - tabHalf, center.y + arcJoin)
        arcTo(circleBounds, arcStart + 180f, arcSweep, false)
        lineTo(center.x - tabOuter, center.y + tabHalf)
        lineTo(center.x - tabOuter, center.y - tabHalf)
        lineTo(center.x - arcJoin, center.y - tabHalf)
        arcTo(circleBounds, arcStart + 270f, arcSweep, false)
        close()
    }
}

fun fixTrianglePath(center: Offset, radius: Float): Path =
    Path().apply {
        val scale = radius / 8f
        moveTo(center.x, center.y - 8f * scale)
        lineTo(center.x + 7f * scale, center.y + 6f * scale)
        lineTo(center.x - 7f * scale, center.y + 6f * scale)
        close()
    }

fun heliportHPath(center: Offset, scale: Float): Path =
    symbolPath(heliportHCommands, center, scale)

fun seaplaneAnchorPath(center: Offset, scale: Float): Path =
    symbolPath(seaplaneAnchorCommands, center, scale)

fun obstacleShortPath(center: Offset, scale: Float): Path =
    symbolPath(obstacleShortCommands, center, scale)

fun obstacleTallPath(center: Offset, scale: Float): Path =
    symbolPath(obstacleTallCommands, center, scale)

fun airportOpenMarkerSymbol(center: Offset, scale: Float): List<NavSymbolLayer> = listOf(
    ${ktSymbolLayers("airport_open_marker")}
)

fun mapSelectionSpotSymbol(center: Offset, scale: Float): List<NavSymbolLayer> = listOf(
    ${ktSymbolLayers("map_selection_spot")}
)

fun metarClearSymbol(center: Offset, scale: Float): List<NavSymbolLayer> = listOf(
    ${ktSymbolLayers("metar_clear")}
)

fun metarFewSymbol(center: Offset, scale: Float): List<NavSymbolLayer> = listOf(
    ${ktSymbolLayers("metar_few")}
)

fun metarSctSymbol(center: Offset, scale: Float): List<NavSymbolLayer> = listOf(
    ${ktSymbolLayers("metar_sct")}
)

fun metarBknSymbol(center: Offset, scale: Float): List<NavSymbolLayer> = listOf(
    ${ktSymbolLayers("metar_bkn")}
)

fun metarOvcSymbol(center: Offset, scale: Float): List<NavSymbolLayer> = listOf(
    ${ktSymbolLayers("metar_ovc")}
)

fun metarMissingSymbol(center: Offset, scale: Float): List<NavSymbolLayer> = listOf(
    ${ktSymbolLayers("metar_missing")}
)

fun pirepGenericSymbol(center: Offset, scale: Float): List<NavSymbolLayer> = listOf(
    ${ktSymbolLayers("pirep_generic")}
)

fun pirepLightTurbulenceSymbol(center: Offset, scale: Float): List<NavSymbolLayer> = listOf(
    ${ktSymbolLayers("pirep_light_turbulence")}
)

fun pirepModerateTurbulenceSymbol(center: Offset, scale: Float): List<NavSymbolLayer> = listOf(
    ${ktSymbolLayers("pirep_moderate_turbulence")}
)

fun pirepSevereTurbulenceSymbol(center: Offset, scale: Float): List<NavSymbolLayer> = listOf(
    ${ktSymbolLayers("pirep_severe_turbulence")}
)

fun pirepLightIcingSymbol(center: Offset, scale: Float): List<NavSymbolLayer> = listOf(
    ${ktSymbolLayers("pirep_light_icing")}
)

fun pirepModerateIcingSymbol(center: Offset, scale: Float): List<NavSymbolLayer> = listOf(
    ${ktSymbolLayers("pirep_moderate_icing")}
)

fun pirepSevereIcingSymbol(center: Offset, scale: Float): List<NavSymbolLayer> = listOf(
    ${ktSymbolLayers("pirep_severe_icing")}
)

const val obstacleShortDotY: Float = ${ktFloat(spec.obstacle_dot.short_y)}
const val obstacleTallDotY: Float = ${ktFloat(spec.obstacle_dot.tall_y)}
const val obstacleDotRadius: Float = ${ktFloat(spec.obstacle_dot.radius)}

fun vorOuterHexPath(center: Offset, radius: Float): Path =
    polygonPath(vorOuterHexPoints, center, radius / 8f)

fun vorBandPath(center: Offset, radius: Float): Path =
    Path().apply {
        fillType = PathFillType.EvenOdd
        addPath(polygonPath(vorOuterHexPoints, center, radius / 8f))
        addPath(polygonPath(vorInnerHexPoints, center, radius / 8f))
    }
`;

fs.mkdirSync(path.dirname(webOut), { recursive: true });
fs.writeFileSync(webOut, webSource);
fs.mkdirSync(androidOut, { recursive: true });
fs.writeFileSync(path.join(androidOut, "NavSymbolPaths.kt"), androidSource);
