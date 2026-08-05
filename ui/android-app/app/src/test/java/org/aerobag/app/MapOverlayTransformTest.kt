// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app

import java.lang.reflect.Modifier
import kotlin.math.abs
import org.aerobag.app.domain.AirspaceDisplayDecoration
import org.aerobag.app.domain.AirspaceDisplayDecorationSegment
import org.aerobag.app.domain.AirspaceDisplayLabel
import org.aerobag.app.domain.AirspaceDisplayPath
import org.aerobag.app.domain.AirspaceDisplayStroke
import org.aerobag.app.domain.AirspaceDisplayStyle
import org.aerobag.app.domain.AirspaceDisplaySubpath
import org.aerobag.app.domain.AirspaceLimitGlyph
import org.aerobag.app.domain.AirspaceScreenPoint
import org.aerobag.app.domain.MapOverlayQueryResult
import org.aerobag.app.domain.MapViewportState
import org.aerobag.app.domain.OfflineRegionDisplay
import org.aerobag.app.domain.SituationRingCandidate
import org.aerobag.app.domain.VisibleMapFeature
import org.aerobag.app.domain.VisibleMetarFeature
import org.aerobag.app.domain.VisiblePirepFeature
import org.aerobag.app.domain.VisibleAdsbTraffic
import org.aerobag.app.domain.displayScaleZoomDelta
import org.aerobag.app.domain.dragViewport
import org.aerobag.app.domain.latLonToWorld
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class MapOverlayTransformTest {
    @Test
    fun situationRingSelectionMatchesLogicalDisplayZoomAcrossDensity() {
        val ownship = LatLon(lat = 27.9755, lon = -82.5332)
        val center = latLonToWorld(ownship.lat, ownship.lon)
        val logicalViewport = MapViewportState(
            centerWorldX = center.x,
            centerWorldY = center.y,
            zoom = 12.5,
        )
        val density = 2.0
        val androidViewport = logicalViewport.copy(
            zoom = logicalViewport.zoom + displayScaleZoomDelta(density),
        )
        val candidates = listOf(
            SituationRingCandidate(radiusNm = 1.0, label = "1nm"),
            SituationRingCandidate(radiusNm = 2.0, label = "2nm"),
            SituationRingCandidate(radiusNm = 3.0, label = "3nm"),
            SituationRingCandidate(radiusNm = 5.0, label = "5nm"),
        )

        val webRing = selectSituationRing(
            position = ownship,
            viewport = logicalViewport,
            widthUnits = 600f,
            heightUnits = 400f,
            ringCandidates = candidates,
            magneticVariationDeg = null,
        )
        val androidRing = selectSituationRing(
            position = ownship,
            viewport = androidViewport,
            widthUnits = 1200f,
            heightUnits = 800f,
            ringCandidates = candidates,
            magneticVariationDeg = null,
        )

        assertEquals(webRing.labelText, androidRing.labelText)
        assertEquals(webRing.radiusUnits.toDouble() * density, androidRing.radiusUnits.toDouble(), 1e-3)
    }

    @Test
    fun displayTransformMovesEveryScreenSpaceCoordinateAfterDrag() {
        val fromViewport = MapViewportState(centerWorldX = 92.4, centerWorldY = 96.8, zoom = 9.3)
        val toViewport = dragViewport(fromViewport, dx = 173f, dy = -91f)
        val fromSurface = OverlaySurfaceUnits(width = 1200f, height = 900f)
        val toSurface = OverlaySurfaceUnits(width = 1200f, height = 900f)
        val overlay = populatedMapOverlay()

        val transformed = transformMapOverlayForDisplay(
            overlay = overlay,
            fromViewport = fromViewport,
            fromSurface = fromSurface,
            toViewport = toViewport,
            toSurface = toSurface,
        )

        val before = collectScreenSpaceCoordinates(overlay)
        val after = collectScreenSpaceCoordinates(transformed)
        assertTrue("test overlay must exercise screen-space coordinates", before.isNotEmpty())
        assertEquals(before.keys, after.keys)

        val unmoved = before.filter { (path, value) ->
            abs((after[path] ?: error("missing transformed coordinate $path")) - value) < 1e-6
        }
        assertTrue(
            "screen-space overlay coordinates must be carried forward to the live viewport: ${unmoved.keys.joinToString()}",
            unmoved.isEmpty(),
        )
    }

    private fun populatedMapOverlay(): MapOverlayQueryResult =
        MapOverlayQueryResult(
            visibleFeatures = listOf(visibleFeature("visible", 100.0, 110.0)),
            flightPlanFeatures = listOf(visibleFeature("flight-plan", 120.0, 130.0)),
            visibleMetars = listOf(
                VisibleMetarFeature(
                    stationId = "KAAA",
                    screenX = 140.0,
                    screenY = 150.0,
                    flightCategory = "vfr",
                    ceilingAmount = "none",
                ),
            ),
            visiblePireps = listOf(
                VisiblePirepFeature(
                    id = "pirep",
                    screenX = 160.0,
                    screenY = 170.0,
                    symbol = "generic",
                    icing = "none",
                    turbulence = "none",
                ),
            ),
            visibleTraffic = listOf(
                VisibleAdsbTraffic(
                    id = "traffic",
                    screenX = 175.0,
                    screenY = 176.0,
                    trackDegTrue = 90.0,
                    label = "N12345",
                    detailLabel = "+02",
                ),
            ),
            airspacePaths = listOf(airspacePath("airspace", 180.0)),
            tfrPaths = listOf(airspacePath("tfr", 260.0)),
            airspaceLabels = listOf(
                AirspaceDisplayLabel(
                    featureId = "airspace",
                    glyph = AirspaceLimitGlyph(
                        upper = "100",
                        lower = "SFC",
                        styleKey = "class_b",
                        colorKey = "class_b",
                    ),
                    screenX = 340.0,
                    screenY = 350.0,
                ),
            ),
            offlineRegions = listOf(
                OfflineRegionDisplay(
                    id = "offline",
                    kind = "cycle",
                    regionId = "region",
                    label = "Region",
                    colorKey = "offline",
                    points = listOf(
                        AirspaceScreenPoint(360.0, 370.0),
                        AirspaceScreenPoint(380.0, 390.0),
                        AirspaceScreenPoint(400.0, 410.0),
                    ),
                    labelX = 420.0,
                    labelY = 430.0,
                ),
            ),
        )

    private fun visibleFeature(id: String, screenX: Double, screenY: Double): VisibleMapFeature =
        VisibleMapFeature(
            id = id,
            kind = "fix",
            label = id.uppercase(),
            symbolKind = "fix",
            styleClass = "fix",
            obstacleVariant = null,
            obstacleTone = null,
            screenX = screenX,
            screenY = screenY,
            towered = false,
            fuelAvailable = false,
            hasPavedRunway = null,
            heliport = null,
            hasWaterRunway = null,
            runwayLengthRatio = 0.0,
            longestRunwayHeadingTrueDeg = null,
        )

    private fun airspacePath(id: String, base: Double): AirspaceDisplayPath =
        AirspaceDisplayPath(
            id = id,
            name = id,
            styleKey = "class_b",
            style = AirspaceDisplayStyle(
                fillColorKey = "class_b",
                fillOpacity = 0.12,
                strokes = listOf(
                    AirspaceDisplayStroke(
                        colorKey = "class_b",
                        widthPx = 2.0,
                        dashPx = emptyList(),
                        lineCap = "round",
                    ),
                ),
            ),
            paths = listOf(
                AirspaceDisplaySubpath(
                    closed = false,
                    points = listOf(
                        AirspaceScreenPoint(base, base + 10.0),
                        AirspaceScreenPoint(base + 20.0, base + 30.0),
                    ),
                ),
            ),
            decorations = listOf(
                AirspaceDisplayDecoration(
                    colorKey = "class_b",
                    widthPx = 1.0,
                    lineCap = "round",
                    paths = listOf(
                        AirspaceDisplaySubpath(
                            closed = false,
                            points = listOf(AirspaceScreenPoint(base + 40.0, base + 50.0)),
                        ),
                    ),
                    segments = listOf(
                        AirspaceDisplayDecorationSegment(
                            x1 = base + 60.0,
                            y1 = base + 70.0,
                            x2 = base + 80.0,
                            y2 = base + 90.0,
                        ),
                    ),
                ),
            ),
        )

    private fun collectScreenSpaceCoordinates(root: Any?): Map<String, Double> {
        val coordinates = linkedMapOf<String, Double>()
        fun visit(value: Any?, path: String) {
            when (value) {
                null -> return
                is String, is Number, is Boolean, is Enum<*> -> return
                is Iterable<*> -> {
                    value.forEachIndexed { index, item -> visit(item, "$path[$index]") }
                    return
                }
                is Array<*> -> {
                    value.forEachIndexed { index, item -> visit(item, "$path[$index]") }
                    return
                }
            }
            val type = value.javaClass
            if (!type.name.startsWith("org.aerobag.app.domain.")) return
            type.declaredFields
                .filterNot { Modifier.isStatic(it.modifiers) }
                .sortedBy { it.name }
                .forEach { field ->
                    field.isAccessible = true
                    val child = field.get(value)
                    val childPath = "$path.${field.name}"
                    if (child is Number && isScreenSpaceCoordinateField(type.simpleName, field.name)) {
                        coordinates[childPath] = child.toDouble()
                    } else {
                        visit(child, childPath)
                    }
                }
        }

        visit(root, root?.javaClass?.simpleName ?: "root")
        return coordinates
    }

    private fun isScreenSpaceCoordinateField(ownerClassName: String, fieldName: String): Boolean =
        when {
            fieldName.startsWith("screen") -> true
            fieldName == "labelX" || fieldName == "labelY" -> true
            fieldName == "x1" || fieldName == "y1" || fieldName == "x2" || fieldName == "y2" -> true
            ownerClassName == "AirspaceScreenPoint" && (fieldName == "x" || fieldName == "y") -> true
            else -> false
        }
}
