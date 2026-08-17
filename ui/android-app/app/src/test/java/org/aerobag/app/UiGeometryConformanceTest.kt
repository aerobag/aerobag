// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app

import java.io.File
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.aerobag.app.domain.ImageViewportState
import org.aerobag.app.domain.LatLonPoint
import org.aerobag.app.domain.MapDisplayFrame
import org.aerobag.app.domain.MapViewportState
import org.aerobag.app.domain.PlateGeoref
import org.aerobag.app.domain.ScreenPoint
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

    private fun JsonObject.objectAt(name: String) = getValue(name).jsonObject
    private fun JsonObject.doubleAt(name: String) = getValue(name).jsonPrimitive.content.toDouble()
    private fun JsonObject.floatAt(name: String) = doubleAt(name).toFloat()
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

    private fun sourceFile(path: String): File {
        val start = File(".").canonicalFile
        return generateSequence(start) { it.parentFile }
            .map { File(it, path) }
            .firstOrNull { it.isFile }
            ?: error("could not locate fixture $path from $start")
    }
}
