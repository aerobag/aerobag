package org.aerobag.app.domain

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
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

    @Test
    fun displayFrameCarryForwardMatchesDirectProjectionAfterDrag() {
        val oldViewport = createInitialViewport(initialViewport, minZoom, maxZoom)
        val oldFrame = MapDisplayFrame(oldViewport, 1200f, 900f)
        val airportLat = 42.10
        val airportLon = -70.80
        val oldScreen = oldFrame.latLonToScreen(airportLat, airportLon)
        val nextViewport = dragViewport(oldViewport, dx = 173f, dy = -91f)
        val nextFrame = MapDisplayFrame(nextViewport, 1200f, 900f)

        val carried = nextFrame.transformScreenPointFrom(oldFrame, oldScreen)
        val direct = nextFrame.latLonToScreen(airportLat, airportLon)

        assertEquals(direct.x.toDouble(), carried.x.toDouble(), 1e-4)
        assertEquals(direct.y.toDouble(), carried.y.toDouble(), 1e-4)
    }

    @Test
    fun displayFrameCarryForwardMatchesDirectProjectionAfterResize() {
        val oldViewport = createInitialViewport(initialViewport, minZoom, maxZoom)
        val oldFrame = MapDisplayFrame(oldViewport, 1200f, 900f)
        val airportLat = 42.10
        val airportLon = -70.80
        val oldScreen = oldFrame.latLonToScreen(airportLat, airportLon)
        val nextFrame = MapDisplayFrame(oldViewport, 900f, 1200f)

        val carried = nextFrame.transformScreenPointFrom(oldFrame, oldScreen)
        val direct = nextFrame.latLonToScreen(airportLat, airportLon)

        assertEquals(direct.x.toDouble(), carried.x.toDouble(), 1e-4)
        assertEquals(direct.y.toDouble(), carried.y.toDouble(), 1e-4)
    }

    @Test
    fun displayFrameProjectsNearestWrappedWorldCopy() {
        val center = latLonToWorld(0.0, 179.0)
        val viewport = MapViewportState(
            centerWorldX = center.x,
            centerWorldY = center.y,
            zoom = 2.0,
        )
        val frame = MapDisplayFrame(viewport, 1000f, 800f)

        val nearbyAcrossAntimeridian = frame.latLonToScreen(0.0, -179.0)

        assertTrue(
            "nearest copy should project just right of center, not one full world left",
            nearbyAcrossAntimeridian.x > 500f && nearbyAcrossAntimeridian.x < 510f,
        )
        assertEquals(400.0, nearbyAcrossAntimeridian.y.toDouble(), 1e-4)
    }

    @Test
    fun mapFollowTargetGateBlocksStaleTargetBetweenSyncAndSnapshotPropagation() {
        val gate = MapFollowTargetGate()
        val oldTarget = createInitialViewport(initialViewport, minZoom, maxZoom)
        val draggedViewport = dragViewport(oldTarget, 120f, 80f)
        val acknowledgedTarget = dragViewport(oldTarget, 122f, 82f)

        gate.beginSync(draggedViewport)
        gate.acknowledgeSyncSnapshot(
            following = true,
            targetViewport = acknowledgedTarget,
        )

        assertTrue(gate.shouldApplyTarget(oldTarget).not())
        assertEquals(acknowledgedTarget, gate.awaitedViewport())
        assertTrue(gate.shouldApplyTarget(acknowledgedTarget))
        assertEquals(null, gate.awaitedViewport())
    }

    @Test
    fun mapFollowTargetGateAllowsTargetsWhenNoFollowSyncIsPending() {
        val gate = MapFollowTargetGate()
        val target = createInitialViewport(initialViewport, minZoom, maxZoom)

        assertTrue(gate.shouldApplyTarget(target))
    }

    @Test
    fun mapFollowTargetGateClearsPendingTargetWhenFollowDisengagesDuringSync() {
        val gate = MapFollowTargetGate()
        val oldTarget = createInitialViewport(initialViewport, minZoom, maxZoom)
        val draggedViewport = dragViewport(oldTarget, 120f, 80f)

        gate.beginSync(draggedViewport)
        gate.acknowledgeSyncSnapshot(
            following = false,
            targetViewport = null,
        )

        assertEquals(null, gate.awaitedViewport())
        assertTrue(gate.shouldApplyTarget(oldTarget))
    }
}
