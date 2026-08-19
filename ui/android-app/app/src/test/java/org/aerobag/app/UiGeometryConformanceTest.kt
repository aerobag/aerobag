// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app

import androidx.compose.ui.geometry.Offset
import java.io.File
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.aerobag.app.domain.FlightPlanRouteDistanceAnnotation
import org.aerobag.app.domain.ImageViewportState
import org.aerobag.app.domain.LatLonPoint
import org.aerobag.app.domain.MapDisplayFrame
import org.aerobag.app.domain.MapViewportState
import org.aerobag.app.domain.PlateGeoref
import org.aerobag.app.domain.RouteSegmentStatus
import org.aerobag.app.domain.ScreenPoint
import org.aerobag.app.domain.SituationRingCandidate
import org.aerobag.app.domain.WorldPoint
import org.aerobag.app.domain.clampImageViewport
import org.aerobag.app.domain.worldToScreen
import org.junit.Assert.assertEquals
import org.junit.Test

class UiGeometryConformanceTest {
    private val vectors = Json.parseToJsonElement(
        sourceFile(
            "ui/core-rust/crates/app-ui-contracts/tests/goldens/" +
                "ui-geometry-conformance.json",
        ).readText(),
    ).jsonObject

    @Test
    fun `map geometry matches core conformance vectors`() {
        val antimeridian = vectors.objectAt("map_antimeridian")
        val projected = worldToScreen(
            antimeridian.objectAt("viewport").viewport(),
            antimeridian.objectAt("world").worldPoint(),
            antimeridian.floatAt("width"),
            antimeridian.floatAt("height"),
        )
        assertScreenPoint(antimeridian.objectAt("expected_screen"), projected)

        val frame = vectors.objectAt("map_frame_transform")
        val transformed = MapDisplayFrame(
            frame.objectAt("to_viewport").viewport(),
            frame.floatAt("to_width"),
            frame.floatAt("to_height"),
        ).transformScreenPointFrom(
            MapDisplayFrame(
                frame.objectAt("from_viewport").viewport(),
                frame.floatAt("from_width"),
                frame.floatAt("from_height"),
            ),
            frame.objectAt("point").screenPoint(),
        )
        assertScreenPoint(frame.objectAt("expected_screen"), transformed)
    }

    @Test
    fun `image and plate geometry match core conformance vectors`() {
        val image = vectors.objectAt("image_clamp")
        val actual = clampImageViewport(
            ImageViewportState(
                image.objectAt("state").floatAt("left"),
                image.objectAt("state").floatAt("top"),
                image.objectAt("state").floatAt("zoom"),
            ),
            image.floatAt("image_width"),
            image.floatAt("image_height"),
            image.floatAt("viewport_width"),
            image.floatAt("viewport_height"),
            image.floatAt("overscroll"),
        )
        val expectedImage = image.objectAt("expected")
        assertEquals(expectedImage.floatAt("left"), actual.leftPx, 1e-4f)
        assertEquals(expectedImage.floatAt("top"), actual.topPx, 1e-4f)
        assertEquals(expectedImage.floatAt("zoom"), actual.zoom, 1e-4f)

        val plate = vectors.objectAt("plate_affine")
        val georef = plate.objectAt("georef")
        val actualPlate = plateImagePoint(
            LatLonPoint(
                plate.objectAt("position").doubleAt("lat"),
                plate.objectAt("position").doubleAt("lon"),
            ),
            PlateGeoref.AirportDiagramTransformV1(
                georef.doubleAt("pixel_x_from_lon"),
                georef.doubleAt("pixel_x_from_lat"),
                georef.doubleAt("pixel_x_offset"),
                georef.doubleAt("pixel_y_from_lon"),
                georef.doubleAt("pixel_y_from_lat"),
                georef.doubleAt("pixel_y_offset"),
            ),
        )
        assertEquals(plate.objectAt("expected_image").doubleAt("x"), actualPlate.x, 1e-9)
        assertEquals(plate.objectAt("expected_image").doubleAt("y"), actualPlate.y, 1e-9)
    }

    @Test
    fun `situation geometry matches core conformance vectors`() {
        val vector = vectors.objectAt("situation_overlay")
        val position = LatLon(
            vector.objectAt("position").doubleAt("lat"),
            vector.objectAt("position").doubleAt("lon"),
        )
        val viewport = vector.objectAt("viewport").viewport()
        val width = vector.floatAt("width")
        val height = vector.floatAt("height")
        assertOffset(
            vector.objectAt("expected").objectAt("center"),
            latLonToScreen(position.lat, position.lon, viewport, width, height),
        )

        val predictor = vector.objectAt("predictor")
        val predictorPosition = projectAhead(
            position.lat,
            position.lon,
            predictor.doubleAt("heading_deg"),
            predictor.doubleAt("speed_kt") * predictor.doubleAt("minutes") / 60.0,
        )
        assertOffset(
            vector.objectAt("expected").objectAt("predictor"),
            latLonToScreen(predictorPosition.lat, predictorPosition.lon, viewport, width, height),
        )

        val ring = selectSituationRing(
            position = position,
            viewport = viewport,
            widthUnits = width,
            heightUnits = height,
            ringCandidates = vector.arrayAt("ring_candidates").map { element ->
                element.jsonObject.let { candidate ->
                    SituationRingCandidate(
                        radiusNm = candidate.doubleAt("radius_nm"),
                        label = candidate.stringAt("label"),
                    )
                }
            },
            magneticVariationDeg = vector.floatAt("magnetic_variation_deg"),
        )
        val expected = vector.objectAt("expected").objectAt("ring")
        assertEquals(expected.stringAt("label"), ring.labelText)
        assertEquals(expected.floatAt("radius"), ring.radiusUnits, 1e-4f)
        assertOffset(expected.objectAt("label_point"), ring.labelPointUnits)
        assertEquals(expected.floatAt("label_rotation_degrees"), ring.labelRotationDeg, 1e-4f)
        assertEquals(expected.arrayAt("ticks").size, ring.tickMarks.size)
        ring.tickMarks.forEachIndexed { index, tick ->
            val expectedTick = expected.arrayAt("ticks")[index].jsonObject
            assertOffset(expectedTick.objectAt("inner"), tick.innerUnits)
            assertOffset(expectedTick.objectAt("outer"), tick.outerUnits)
        }
        assertEquals(expected.arrayAt("cardinals").size, ring.cardinalLabels.size)
        ring.cardinalLabels.forEachIndexed { index, cardinal ->
            val expectedCardinal = expected.arrayAt("cardinals")[index].jsonObject
            assertEquals(expectedCardinal.stringAt("text"), cardinal.text)
            assertOffset(expectedCardinal.objectAt("point"), cardinal.pointUnits)
            assertEquals(expectedCardinal.floatAt("rotation_degrees"), cardinal.rotationDeg, 1e-4f)
        }
    }

    @Test
    fun `route annotations match core conformance vectors`() {
        val chevrons = vectors.objectAt("route_chevrons")
        val placements = spacedRouteChevronPlacements(
            chevrons.arrayAt("path").map { it.jsonObject.offset() },
            chevrons.floatAt("spacing"),
        )
        val expectedChevrons = chevrons.arrayAt("expected")
        assertEquals(expectedChevrons.size, placements.size)
        placements.forEachIndexed { index, placement ->
            val expected = expectedChevrons[index].jsonObject
            assertOffset(expected, placement.center)
            assertEquals(expected.floatAt("angle_degrees"), placement.angleDegrees, 1e-4f)
        }

        val pill = vectors.objectAt("route_distance_pill")
        val annotation = FlightPlanRouteDistanceAnnotation(
            id = "conformance-pill",
            segmentIndexes = pill.arrayAt("segment_indexes").map { it.jsonPrimitive.content.toInt() },
            text = pill.stringAt("text"),
            distanceNm = 20.0,
            status = RouteSegmentStatus.Active,
            requiredFeatureIds = pill.arrayAt("required_feature_ids").map { it.jsonPrimitive.content },
            minimumPathToPillWidthRatio = pill.doubleAt("minimum_path_to_pill_width_ratio"),
        )
        val layout = layoutRouteDistancePills(
            annotations = listOf(annotation),
            screenPaths = pill.arrayAt("screen_paths").map { path ->
                path.jsonArray.map { it.jsonObject.offset() }
            },
            visibleFeatureIds = pill.arrayAt("visible_feature_ids").map { it.jsonPrimitive.content }.toSet(),
            measurePillWidth = { pill.floatAt("measured_width") },
        ).single()
        val expectedPill = pill.objectAt("expected")
        assertOffset(expectedPill.objectAt("center"), layout.center)
        assertEquals(expectedPill.floatAt("width"), layout.widthPx, 1e-4f)
        assertEquals(expectedPill.floatAt("rotation_degrees"), layout.rotationDegrees, 1e-4f)
    }

    private fun JsonObject.objectAt(name: String) = getValue(name).jsonObject
    private fun JsonObject.arrayAt(name: String): JsonArray = getValue(name).jsonArray
    private fun JsonObject.doubleAt(name: String) = getValue(name).jsonPrimitive.content.toDouble()
    private fun JsonObject.floatAt(name: String) = doubleAt(name).toFloat()
    private fun JsonObject.stringAt(name: String) = getValue(name).jsonPrimitive.content
    private fun JsonObject.offset() = Offset(floatAt("x"), floatAt("y"))
    private fun JsonObject.worldPoint() = WorldPoint(doubleAt("x"), doubleAt("y"))
    private fun JsonObject.screenPoint() = ScreenPoint(floatAt("x"), floatAt("y"))
    private fun JsonObject.viewport() = MapViewportState(
        centerWorldX = doubleAt("center_world_x"),
        centerWorldY = doubleAt("center_world_y"),
        zoom = doubleAt("zoom"),
        rotationDeg = doubleAt("rotation_deg"),
    )

    private fun assertScreenPoint(expected: JsonObject, actual: ScreenPoint) {
        assertEquals(expected.doubleAt("x"), actual.x.toDouble(), 1e-4)
        assertEquals(expected.doubleAt("y"), actual.y.toDouble(), 1e-4)
    }

    private fun assertOffset(expected: JsonObject, actual: Offset) {
        assertEquals(expected.doubleAt("x"), actual.x.toDouble(), 1e-4)
        assertEquals(expected.doubleAt("y"), actual.y.toDouble(), 1e-4)
    }

    private fun sourceFile(path: String): File {
        val start = File(".").canonicalFile
        return generateSequence(start) { it.parentFile }
            .map { File(it, path) }
            .firstOrNull { it.isFile }
            ?: error("could not locate fixture $path from $start")
    }
}
