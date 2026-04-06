package net.jonh.aerobag.prototype.domain

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class MapViewportTest {
    private val mapView = MapView(
        chartFamily = MapChartFamily.Tac,
        chartName = "Boston TAC",
        chartIndex = 1,
        tileRoot = "charts-tac",
        tileSize = 256,
        minZoom = 8.6,
        maxZoom = 10.8,
        initialViewport = MapViewportSeed(
            lat = 42.24,
            lon = -70.949202,
            zoom = 9.6,
        ),
        levels = listOf(
            TileLevelAvailability(
                zoom = 9,
                xMin = 149,
                xMax = 157,
                yTmsMin = 317,
                yTmsMax = 323,
            ),
            TileLevelAvailability(
                zoom = 10,
                xMin = 300,
                xMax = 314,
                yTmsMin = 634,
                yTmsMax = 647,
            ),
        ),
    )

    @Test
    fun zoomAroundPointKeepsAnchorStable() {
        val viewport = createInitialViewport(mapView)
        val anchor = ScreenPoint(320f, 280f)
        val anchoredWorld = screenToWorld(viewport, anchor, 1200f, 900f)

        val zoomed = zoomAroundPoint(viewport, mapView, anchor, 1200f, 900f, viewport.zoom + 0.8)
        val anchoredWorldAfter = screenToWorld(zoomed, anchor, 1200f, 900f)

        assertEquals(anchoredWorld.x, anchoredWorldAfter.x, 1e-8)
        assertEquals(anchoredWorld.y, anchoredWorldAfter.y, 1e-8)
    }

    @Test
    fun pinchGesturePreservesBothAnchorsForStraightLineMotion() {
        val viewport = createInitialViewport(mapView)
        val startFirst = ScreenPoint(320f, 450f)
        val startSecond = ScreenPoint(880f, 450f)
        val snapshot = createPinchSnapshot(viewport, startFirst, startSecond, 1200f, 900f)
        val movedFirst = ScreenPoint(260f, 450f)
        val movedSecond = ScreenPoint(940f, 450f)

        val pinched = applyPinchGesture(snapshot, movedFirst, movedSecond, mapView, 1200f, 900f)
        val firstWorldAfter = screenToWorld(pinched, movedFirst, 1200f, 900f)
        val secondWorldAfter = screenToWorld(pinched, movedSecond, 1200f, 900f)

        assertEquals(snapshot.firstAnchorWorld.x, firstWorldAfter.x, 1e-8)
        assertEquals(snapshot.firstAnchorWorld.y, firstWorldAfter.y, 1e-8)
        assertEquals(snapshot.secondAnchorWorld.x, secondWorldAfter.x, 1e-8)
        assertEquals(snapshot.secondAnchorWorld.y, secondWorldAfter.y, 1e-8)
    }

    @Test
    fun initialViewportRendersAvailableTilesAndCenterRoundTrips() {
        val viewport = createInitialViewport(mapView)
        val tiles = renderTiles(mapView, viewport, 1200f, 900f)
        val center = viewportCenterLatLon(viewport)

        assertTrue(tiles.isNotEmpty())
        assertTrue(tiles.any { it.zoom == 10 })
        assertEquals(mapView.initialViewport.lat, center.first, 1e-3)
        assertEquals(mapView.initialViewport.lon, center.second, 1e-3)
    }
}
