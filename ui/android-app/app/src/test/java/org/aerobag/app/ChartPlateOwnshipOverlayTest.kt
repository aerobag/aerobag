// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app

import androidx.compose.ui.geometry.Offset
import org.aerobag.app.domain.ImageDisplaySize
import org.aerobag.app.domain.ImageViewportState
import org.aerobag.app.domain.FlightPlanRouteSegment
import org.aerobag.app.domain.LatLonPoint
import org.aerobag.app.domain.OwnshipRenderState
import org.aerobag.app.domain.PlateGeoref
import org.aerobag.app.domain.RouteSegmentStatus
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class ChartPlateOwnshipOverlayTest {
    private fun routeSegment(path: List<LatLonPoint> = emptyList()) = FlightPlanRouteSegment(
        id = "route-1",
        legId = "leg-1",
        from = LatLonPoint(lat = 47.5, lon = -122.5),
        to = LatLonPoint(lat = 47.0, lon = -122.0),
        path = path,
        style = "solid",
        distanceNm = 10.0,
        courseDeg = 90.0,
        status = RouteSegmentStatus.Active,
    )

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

    @Test
    fun flightPlanUsesCookedRoutePathAndDisplayedPlateTransform() {
        val overlay = resolvePlateFlightPlanOverlay(
            segments = listOf(routeSegment(path = listOf(
                LatLonPoint(lat = 47.5, lon = -122.5),
                LatLonPoint(lat = 47.25, lon = -122.25),
                LatLonPoint(lat = 47.0, lon = -122.0),
            ))),
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
            surfaceWidthPx = 500f,
            surfaceHeightPx = 500f,
        )

        assertEquals(1, overlay.size)
        assertEquals(
            listOf(
                Offset(110f, 120f),
                Offset(160f, 170f),
                Offset(210f, 220f),
            ),
            overlay.single().path,
        )
    }

    @Test
    fun flightPlanFallsBackToSegmentEndpoints() {
        val overlay = resolvePlateFlightPlanOverlay(
            segments = listOf(routeSegment()),
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
            surfaceWidthPx = 500f,
            surfaceHeightPx = 500f,
        )

        assertEquals(listOf(Offset(110f, 120f), Offset(210f, 220f)), overlay.single().path)
    }
}
