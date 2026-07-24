// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app.domain

import kotlinx.serialization.decodeFromString
import kotlinx.serialization.json.Json
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class ChartGeorefWireTest {
    private val json = Json {
        ignoreUnknownKeys = true
    }

    @Test
    fun chartAssetCarriesPlateTransformGeorefFromCore() {
        val asset = json.decodeFromString<WireDerivedChartAsset>(
            """
            {
              "id": "KSEA-I16C",
              "airport_id": "KSEA",
              "label": "ILS or LOC 16C",
              "kind": "approach",
              "folder_category": "approach",
              "has_thumbnail": true,
              "georef": {
                "kind": "plate_transform_v1",
                "pixels_per_longitude": 12.5,
                "pixels_per_latitude": -13.5,
                "top_left_lon": -123.0,
                "top_left_lat": 48.0
              }
            }
            """.trimIndent(),
        ).toUi()

        val georef = asset.georef
        assertTrue(georef is PlateGeoref.PlateTransformV1)
        georef as PlateGeoref.PlateTransformV1
        assertEquals(12.5, georef.pixelsPerLongitude, 0.001)
        assertEquals(-13.5, georef.pixelsPerLatitude, 0.001)
        assertEquals(-123.0, georef.topLeftLon, 0.001)
        assertEquals(48.0, georef.topLeftLat, 0.001)
    }

    @Test
    fun chartAssetCarriesAirportDiagramGeorefFromCore() {
        val asset = json.decodeFromString<WireDerivedChartAsset>(
            """
            {
              "id": "KSEA-APD",
              "airport_id": "KSEA",
              "label": "Airport Diagram",
              "kind": "airport_diagram",
              "folder_category": "airport",
              "has_thumbnail": true,
              "georef": {
                "kind": "airport_diagram_transform_v1",
                "pixel_x_from_lon": 10.0,
                "pixel_x_from_lat": 1.0,
                "pixel_x_offset": 3.0,
                "pixel_y_from_lon": -2.0,
                "pixel_y_from_lat": 20.0,
                "pixel_y_offset": 4.0
              }
            }
            """.trimIndent(),
        ).toUi()

        val georef = asset.georef
        assertTrue(georef is PlateGeoref.AirportDiagramTransformV1)
        georef as PlateGeoref.AirportDiagramTransformV1
        assertEquals(10.0, georef.pixelXFromLon, 0.001)
        assertEquals(1.0, georef.pixelXFromLat, 0.001)
        assertEquals(3.0, georef.pixelXOffset, 0.001)
        assertEquals(-2.0, georef.pixelYFromLon, 0.001)
        assertEquals(20.0, georef.pixelYFromLat, 0.001)
        assertEquals(4.0, georef.pixelYOffset, 0.001)
    }
}
