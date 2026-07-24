// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app

import org.aerobag.app.domain.ImageDisplaySize
import org.aerobag.app.domain.ImageViewportState
import org.aerobag.app.domain.LatLonPoint
import org.aerobag.app.domain.OwnshipRenderState
import org.aerobag.app.domain.PlateGeoref
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class ChartPlateOwnshipOverlayTest {
    @Test
    fun plateTransformProjectsOwnshipIntoDisplayedViewport() {
        val overlay = resolvePlateOwnshipOverlay(
            ownship = OwnshipRenderState(
                drawAircraft = true,
                position = LatLonPoint(lat = 47.5, lon = -122.5),
                orientationDeg = 91.0,
            ),
            georef = PlateGeoref.PlateTransformV1(
                pixelsPerLongitude = 100.0,
                pixelsPerLatitude = -100.0,
                topLeftLon = -123.0,
                topLeftLat = 48.0,
            ),
            imageWidthPx = 200f,
            imageHeightPx = 200f,
            viewport = ImageViewportState(leftPx = 10f, topPx = 20f, zoom = 1f),
            displaySize = ImageDisplaySize(widthPx = 400f, heightPx = 400f),
        )

        requireNotNull(overlay)
        assertEquals(110f, overlay.screenX, 0.001f)
        assertEquals(120f, overlay.screenY, 0.001f)
        assertEquals(91f, overlay.headingDeg, 0.001f)
    }

    @Test
    fun airportDiagramTransformProjectsOwnshipIntoDisplayedViewport() {
        val overlay = resolvePlateOwnshipOverlay(
            ownship = OwnshipRenderState(
                drawAircraft = true,
                position = LatLonPoint(lat = 5.0, lon = 7.0),
            ),
            georef = PlateGeoref.AirportDiagramTransformV1(
                pixelXFromLon = 10.0,
                pixelXFromLat = 1.0,
                pixelXOffset = 3.0,
                pixelYFromLon = -2.0,
                pixelYFromLat = 20.0,
                pixelYOffset = 4.0,
            ),
            imageWidthPx = 100f,
            imageHeightPx = 100f,
            viewport = ImageViewportState(leftPx = 5f, topPx = 6f, zoom = 1f),
            displaySize = ImageDisplaySize(widthPx = 200f, heightPx = 300f),
        )

        requireNotNull(overlay)
        assertEquals(161f, overlay.screenX, 0.001f)
        assertEquals(276f, overlay.screenY, 0.001f)
        assertEquals(0f, overlay.headingDeg, 0.001f)
    }

    @Test
    fun overlayIsSuppressedWhenOwnshipWouldBeOutsideImage() {
        val overlay = resolvePlateOwnshipOverlay(
            ownship = OwnshipRenderState(
                drawAircraft = true,
                position = LatLonPoint(lat = 48.0, lon = -124.0),
            ),
            georef = PlateGeoref.PlateTransformV1(
                pixelsPerLongitude = 100.0,
                pixelsPerLatitude = -100.0,
                topLeftLon = -123.0,
                topLeftLat = 48.0,
            ),
            imageWidthPx = 200f,
            imageHeightPx = 200f,
            viewport = ImageViewportState(leftPx = 10f, topPx = 20f, zoom = 1f),
            displaySize = ImageDisplaySize(widthPx = 400f, heightPx = 400f),
        )

        assertNull(overlay)
    }
}
