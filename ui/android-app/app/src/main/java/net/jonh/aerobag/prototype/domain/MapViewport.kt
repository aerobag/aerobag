package net.jonh.aerobag.prototype.domain

import kotlin.math.PI
import kotlin.math.asinh
import kotlin.math.atan
import kotlin.math.exp
import kotlin.math.hypot
import kotlin.math.ln
import kotlin.math.max
import kotlin.math.min
import kotlin.math.pow
import kotlin.math.roundToInt
import kotlin.math.sinh
import kotlin.math.tan

private const val WORLD_SIZE = 256.0
private const val MAX_LATITUDE = 85.05112878

data class MapViewportState(
    val centerWorldX: Double,
    val centerWorldY: Double,
    val zoom: Double,
)

data class ScreenPoint(
    val x: Float,
    val y: Float,
)

data class WorldPoint(
    val x: Double,
    val y: Double,
)

data class RenderTile(
    val x: Int,
    val yTms: Int,
    val leftPx: Float,
    val topPx: Float,
    val sizePx: Float,
    val zoom: Int,
    val mapViewId: String,
    val family: MapChartFamily,
    val sources: List<RenderTileSource>,
)

data class RenderTileSource(
    val mapViewId: String,
    val packageName: String?,
    val storageKind: TileStorageKind,
    val path: String,
)

data class RenderTileKey(
    val mapViewId: String,
    val zoom: Int,
    val x: Int,
    val yTms: Int,
)

data class PinchSnapshot(
    val viewport: MapViewportState,
    val firstAnchorWorld: WorldPoint,
    val secondAnchorWorld: WorldPoint,
    val first: ScreenPoint,
    val second: ScreenPoint,
)

fun createInitialViewport(seed: MapViewportSeed, minZoom: Double, maxZoom: Double): MapViewportState {
    val center = latLonToWorld(seed.lat, seed.lon)
    return MapViewportState(
        centerWorldX = center.x,
        centerWorldY = center.y,
        zoom = clampZoom(seed.zoom, minZoom, maxZoom),
    )
}

fun preserveViewportForMap(viewport: MapViewportState, minZoom: Double, maxZoom: Double): MapViewportState =
    viewport.copy(zoom = clampZoom(viewport.zoom, minZoom, maxZoom))

fun clampZoom(zoom: Double, minZoom: Double, maxZoom: Double): Double = min(maxZoom, max(minZoom, zoom))

fun latLonToWorld(lat: Double, lon: Double): WorldPoint {
    val clampedLat = min(MAX_LATITUDE, max(-MAX_LATITUDE, lat))
    val x = ((lon + 180.0) / 360.0) * WORLD_SIZE
    val y = ((1.0 - asinh(tan(Math.toRadians(clampedLat))) / PI) / 2.0) * WORLD_SIZE
    return WorldPoint(x = x, y = y)
}

fun worldToLatLon(worldX: Double, worldY: Double): Pair<Double, Double> {
    val lon = (worldX / WORLD_SIZE) * 360.0 - 180.0
    val n = PI - (2.0 * PI * worldY) / WORLD_SIZE
    val lat = Math.toDegrees(atan(sinh(n)))
    return lat to lon
}

fun scaleForZoom(zoom: Double): Double = 2.0.pow(zoom)

fun dragViewport(viewport: MapViewportState, dx: Float, dy: Float): MapViewportState {
    val scale = scaleForZoom(viewport.zoom)
    return viewport.copy(
        centerWorldX = viewport.centerWorldX - dx / scale,
        centerWorldY = viewport.centerWorldY - dy / scale,
    )
}

fun screenToWorld(
    viewport: MapViewportState,
    point: ScreenPoint,
    widthPx: Float,
    heightPx: Float,
): WorldPoint {
    val scale = scaleForZoom(viewport.zoom)
    return WorldPoint(
        x = viewport.centerWorldX + (point.x - widthPx / 2f) / scale,
        y = viewport.centerWorldY + (point.y - heightPx / 2f) / scale,
    )
}

fun zoomAroundPoint(
    viewport: MapViewportState,
    minZoom: Double,
    maxZoom: Double,
    anchor: ScreenPoint,
    widthPx: Float,
    heightPx: Float,
    nextZoom: Double,
): MapViewportState {
    val clampedZoom = clampZoom(nextZoom, minZoom, maxZoom)
    val anchorWorld = screenToWorld(viewport, anchor, widthPx, heightPx)
    val nextScale = scaleForZoom(clampedZoom)
    return MapViewportState(
        centerWorldX = anchorWorld.x - (anchor.x - widthPx / 2f) / nextScale,
        centerWorldY = anchorWorld.y - (anchor.y - heightPx / 2f) / nextScale,
        zoom = clampedZoom,
    )
}

fun createPinchSnapshot(
    viewport: MapViewportState,
    first: ScreenPoint,
    second: ScreenPoint,
    widthPx: Float,
    heightPx: Float,
): PinchSnapshot = PinchSnapshot(
    viewport = viewport,
    firstAnchorWorld = screenToWorld(viewport, first, widthPx, heightPx),
    secondAnchorWorld = screenToWorld(viewport, second, widthPx, heightPx),
    first = first,
    second = second,
)

fun applyPinchGesture(
    snapshot: PinchSnapshot,
    currentFirst: ScreenPoint,
    currentSecond: ScreenPoint,
    minZoom: Double,
    maxZoom: Double,
    widthPx: Float,
    heightPx: Float,
): MapViewportState {
    val startDistance = hypot(
        (snapshot.second.x - snapshot.first.x).toDouble(),
        (snapshot.second.y - snapshot.first.y).toDouble(),
    )
    val currentDistance = hypot(
        (currentSecond.x - currentFirst.x).toDouble(),
        (currentSecond.y - currentFirst.y).toDouble(),
    )
    val zoomDelta = if (startDistance > 0.0) ln(currentDistance / startDistance) / ln(2.0) else 0.0
    val nextZoom = clampZoom(snapshot.viewport.zoom + zoomDelta, minZoom, maxZoom)
    val nextScale = scaleForZoom(nextZoom)
    val centerOne = WorldPoint(
        x = snapshot.firstAnchorWorld.x - (currentFirst.x - widthPx / 2f) / nextScale,
        y = snapshot.firstAnchorWorld.y - (currentFirst.y - heightPx / 2f) / nextScale,
    )
    val centerTwo = WorldPoint(
        x = snapshot.secondAnchorWorld.x - (currentSecond.x - widthPx / 2f) / nextScale,
        y = snapshot.secondAnchorWorld.y - (currentSecond.y - heightPx / 2f) / nextScale,
    )
    return MapViewportState(
        centerWorldX = (centerOne.x + centerTwo.x) / 2.0,
        centerWorldY = (centerOne.y + centerTwo.y) / 2.0,
        zoom = nextZoom,
    )
}

fun viewportCenterLatLon(viewport: MapViewportState): Pair<Double, Double> =
    worldToLatLon(viewport.centerWorldX, viewport.centerWorldY)

fun renderTileKey(tile: RenderTile): RenderTileKey =
    RenderTileKey(
        mapViewId = tile.mapViewId,
        zoom = tile.zoom,
        x = tile.x,
        yTms = tile.yTms,
    )
