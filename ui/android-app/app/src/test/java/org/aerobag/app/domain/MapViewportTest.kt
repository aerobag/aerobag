package org.aerobag.app.domain

import org.junit.Assert.assertEquals
import org.junit.Test

class MapViewportTest {
    private val initialViewport = MapViewportSeed(
        lat = 42.24,
        lon = -70.949202,
        zoom = 9.6,
    )
    private val minZoom = 8.6
    private val maxZoom = 10.8

    @Test
    fun zoomAroundPointKeepsAnchorStable() {
        val viewport = createInitialViewport(initialViewport, minZoom, maxZoom)
        val anchor = ScreenPoint(320f, 280f)
        val anchoredWorld = screenToWorld(viewport, anchor, 1200f, 900f)

        val zoomed = zoomAroundPoint(
            viewport,
            minZoom,
            maxZoom,
            anchor,
            1200f,
            900f,
            viewport.zoom + 0.8,
        )
        val anchoredWorldAfter = screenToWorld(zoomed, anchor, 1200f, 900f)

        assertEquals(anchoredWorld.x, anchoredWorldAfter.x, 1e-8)
        assertEquals(anchoredWorld.y, anchoredWorldAfter.y, 1e-8)
    }

    @Test
    fun pinchGesturePreservesBothAnchorsForStraightLineMotion() {
        val viewport = createInitialViewport(initialViewport, minZoom, maxZoom)
        val startFirst = ScreenPoint(320f, 450f)
        val startSecond = ScreenPoint(880f, 450f)
        val snapshot = createPinchSnapshot(viewport, startFirst, startSecond, 1200f, 900f)
        val movedFirst = ScreenPoint(260f, 450f)
        val movedSecond = ScreenPoint(940f, 450f)

        val pinched = applyPinchGesture(
            snapshot,
            movedFirst,
            movedSecond,
            minZoom,
            maxZoom,
            1200f,
            900f,
        )
        val firstWorldAfter = screenToWorld(pinched, movedFirst, 1200f, 900f)
        val secondWorldAfter = screenToWorld(pinched, movedSecond, 1200f, 900f)

        assertEquals(snapshot.firstAnchorWorld.x, firstWorldAfter.x, 1e-8)
        assertEquals(snapshot.firstAnchorWorld.y, firstWorldAfter.y, 1e-8)
        assertEquals(snapshot.secondAnchorWorld.x, secondWorldAfter.x, 1e-8)
        assertEquals(snapshot.secondAnchorWorld.y, secondWorldAfter.y, 1e-8)
    }

    @Test
    fun switchingLayersPreservesCenterAndZoom() {
        val viewport = createInitialViewport(initialViewport, minZoom, maxZoom).copy(
            centerWorldX = 140.25,
            centerWorldY = 92.75,
            zoom = 10.4,
        )

        val preserved = preserveViewportForMap(viewport, 4.2, 9.8)

        assertEquals(viewport.centerWorldX, preserved.centerWorldX, 1e-8)
        assertEquals(viewport.centerWorldY, preserved.centerWorldY, 1e-8)
        assertEquals(viewport.zoom, preserved.zoom, 1e-8)
    }
}
