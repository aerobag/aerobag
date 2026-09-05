// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app.domain

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
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
    fun plannedRasterCountUsesTheSameIdentityAsTheBitmapCache() {
        fun tile(path: String, x: Int = 41) = RenderTile(
            x = x,
            yTms = 90,
            leftPx = 0f,
            topPx = 0f,
            sizePx = 256f,
            zoom = 10,
            mapViewId = "enr-h:nw",
            sources = listOf(
                RenderTileSource(
                    mapViewId = "enr-h:nw",
                    packageName = "enr-h-nw",
                    storageKind = TileStorageKind.StaticProduct,
                    path = path,
                ),
            ),
        )

        assertEquals(
            2,
            distinctRenderTileCount(
                listOf(
                    tile("first-source.png"),
                    tile("overlapping-source.png"),
                    tile("neighbor.png", x = 42),
                ),
            ),
        )
    }

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
    fun displayScaleZoomHelpersConvertBetweenLogicalAndPhysicalZoom() {
        val logicalMaxZoom = 12.5
        val density = 2.0
        val physicalMaxZoom = physicalDisplayMaxZoom(logicalMaxZoom, density)
        val physicalViewport = MapViewportState(centerWorldX = 40.0, centerWorldY = 50.0, zoom = physicalMaxZoom)
        val logicalViewport = logicalViewportForDisplayScale(physicalViewport, density)

        assertEquals(1.0, displayScaleZoomDelta(density), 1e-8)
        assertEquals(13.5, physicalMaxZoom, 1e-8)
        assertEquals(logicalMaxZoom, logicalViewport.zoom, 1e-8)
        assertEquals(physicalViewport.centerWorldX, logicalViewport.centerWorldX, 1e-8)
        assertEquals(physicalViewport.centerWorldY, logicalViewport.centerWorldY, 1e-8)
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
    fun rotatedViewportRoundTripsScreenAndWorldCoordinates() {
        val viewport = createInitialViewport(initialViewport, minZoom, maxZoom).copy(rotationDeg = 73.0)
        val screen = ScreenPoint(203f, 711f)
        val world = screenToWorld(viewport, screen, 1200f, 900f)
        val roundTrip = worldToScreen(viewport, world, 1200f, 900f)

        assertEquals(screen.x.toDouble(), roundTrip.x.toDouble(), 1e-4)
        assertEquals(screen.y.toDouble(), roundTrip.y.toDouble(), 1e-4)
    }

    @Test
    fun rotatedViewportPansInScreenCoordinates() {
        val viewport = createInitialViewport(initialViewport, minZoom, maxZoom).copy(rotationDeg = 90.0)
        val worldUnderCursor = screenToWorld(viewport, ScreenPoint(600f, 300f), 1200f, 900f)
        val dragged = dragViewport(viewport, dx = 0f, dy = 150f)
        val after = worldToScreen(dragged, worldUnderCursor, 1200f, 900f)

        assertEquals(600.0, after.x.toDouble(), 1e-4)
        assertEquals(450.0, after.y.toDouble(), 1e-4)
    }

    @Test
    fun trackUpMemoryRetainsLastTrackAcrossGapsOnlyWhileModeRemainsSelected() {
        val memory = MapOrientationMemory()

        assertEquals(0.0, memory.resolve(MapOrientationMode.Track, null), 1e-8)
        assertEquals(42.0, memory.resolve(MapOrientationMode.Track, 42.0), 1e-8)
        assertEquals(42.0, memory.resolve(MapOrientationMode.Track, null), 1e-8)
        assertEquals(0.0, memory.resolve(MapOrientationMode.North, null), 1e-8)
        assertEquals(0.0, memory.resolve(MapOrientationMode.Track, null), 1e-8)
    }

    @Test
    fun rotatedViewportEnvelopeCoversAllRotatedCorners() {
        val envelope = rotatedViewportEnvelopeSize(1200f, 800f, 45.0)

        assertEquals(Math.sqrt(0.5) * 2000.0, envelope.x.toDouble(), 1e-4)
        assertEquals(Math.sqrt(0.5) * 2000.0, envelope.y.toDouble(), 1e-4)
    }

    @Test
    fun mapFollowTargetGateBlocksTargetOlderThanLatestSync() {
        val gate = MapFollowTargetGate()

        gate.beginSync()
        gate.acknowledgeSyncSnapshot(
            following = true,
            targetRevision = 42,
        )

        assertFalse(gate.shouldApplyTarget(41))
        assertEquals(42L, gate.minimumRevision())
        assertTrue(gate.shouldApplyTarget(42))
        assertEquals(null, gate.minimumRevision())
    }

    @Test
    fun mapFollowTargetGateBlocksTargetsWhileSyncIsInFlight() {
        val gate = MapFollowTargetGate()

        gate.beginSync()

        assertFalse(gate.shouldApplyTarget(42))
    }

    @Test
    fun mapFollowTargetGateAllowsNewerTargetWhenAcknowledgedSnapshotWasSkipped() {
        val gate = MapFollowTargetGate()

        gate.beginSync()
        gate.acknowledgeSyncSnapshot(
            following = true,
            targetRevision = 42,
        )

        assertTrue(gate.shouldApplyTarget(43))
        assertEquals(null, gate.minimumRevision())
    }

    @Test
    fun mapFollowTargetGateAllowsTargetsWhenNoFollowSyncIsPending() {
        val gate = MapFollowTargetGate()
        assertTrue(gate.shouldApplyTarget(1))
    }

    @Test
    fun mapFollowTargetGateClearsPendingTargetWhenFollowDisengagesDuringSync() {
        val gate = MapFollowTargetGate()
        gate.beginSync()
        gate.acknowledgeSyncSnapshot(
            following = false,
            targetRevision = 42,
        )

        assertEquals(null, gate.minimumRevision())
        assertTrue(gate.shouldApplyTarget(41))
    }

    @Test
    fun completedCtrDragPreservesOwnshipScreenOffsetUntilOwnshipLeavesViewport() {
        val widthPx = 1200f
        val heightPx = 900f
        val ownship = latLonToWorld(47.50, -122.30)
        val centeredViewport = MapViewportState(
            centerWorldX = ownship.x,
            centerWorldY = ownship.y,
            zoom = 10.0,
        )
        val draggedViewport = dragViewport(centeredViewport, dx = 260f, dy = -140f)
        val draggedOwnshipScreen = MapDisplayFrame(draggedViewport, widthPx, heightPx)
            .worldToScreen(ownship)
        val follow = TestMapFollowState()

        val syncViewport = mapFollowSyncViewportForCompletedGesture(
            movedViewportDuringGesture = true,
            finalGestureViewport = draggedViewport,
            displayRotationDeg = 0.0,
        )
        require(syncViewport != null)
        follow.sync(
            ownshipWorld = ownship,
            viewport = syncViewport,
            widthPx = widthPx,
            heightPx = heightPx,
        )

        assertTrue("CTR should stay active while ownship remains in the viewport", follow.following)
        assertEquals(draggedViewport.centerWorldX, follow.targetViewport(ownship).centerWorldX, 1e-8)
        assertEquals(draggedViewport.centerWorldY, follow.targetViewport(ownship).centerWorldY, 1e-8)

        val movedOwnship = WorldPoint(
            x = ownship.x + 0.015,
            y = ownship.y - 0.010,
        )
        val followingViewport = follow.targetViewport(movedOwnship)
        val movedOwnshipScreen = MapDisplayFrame(followingViewport, widthPx, heightPx)
            .worldToScreen(movedOwnship)
        assertEquals(
            "CTR should preserve the screen-space offset established by the drag",
            draggedOwnshipScreen.x.toDouble(),
            movedOwnshipScreen.x.toDouble(),
            1e-4,
        )
        assertEquals(
            "CTR should preserve the screen-space offset established by the drag",
            draggedOwnshipScreen.y.toDouble(),
            movedOwnshipScreen.y.toDouble(),
            1e-4,
        )

        val offscreenViewport = dragViewport(centeredViewport, dx = widthPx * 1.5f, dy = 0f)
        val offscreenSyncViewport = mapFollowSyncViewportForCompletedGesture(
            movedViewportDuringGesture = true,
            finalGestureViewport = offscreenViewport,
            displayRotationDeg = 0.0,
        )
        require(offscreenSyncViewport != null)
        follow.sync(
            ownshipWorld = ownship,
            viewport = offscreenSyncViewport,
            widthPx = widthPx,
            heightPx = heightPx,
        )

        assertFalse("CTR should disengage when a drag moves ownship out of the viewport", follow.following)
    }

    @Test
    fun completedCtrDragReportsTheDisplayedMapRotationToCore() {
        val northUpViewport = MapViewportState(
            centerWorldX = 128.0,
            centerWorldY = 128.0,
            zoom = 10.0,
        )

        val syncViewport = mapFollowSyncViewportForCompletedGesture(
            movedViewportDuringGesture = true,
            finalGestureViewport = northUpViewport,
            displayRotationDeg = 87.0,
        )

        assertEquals(87.0, requireNotNull(syncViewport).rotationDeg, 0.0)
    }

    private class TestMapFollowState {
        var following: Boolean = true
            private set
        private var currentViewport: MapViewportState? = null
        private var anchorOffsetXPx: Double = 0.0
        private var anchorOffsetYPx: Double = 0.0

        fun sync(
            ownshipWorld: WorldPoint,
            viewport: MapViewportState,
            widthPx: Float,
            heightPx: Float,
        ) {
            currentViewport = viewport
            val point = MapDisplayFrame(viewport, widthPx, heightPx).worldToScreen(ownshipWorld)
            if (
                point.x < 0f ||
                point.x > widthPx ||
                point.y < 0f ||
                point.y > heightPx
            ) {
                following = false
                return
            }
            following = true
            anchorOffsetXPx = point.x - widthPx / 2.0
            anchorOffsetYPx = point.y - heightPx / 2.0
        }

        fun targetViewport(ownshipWorld: WorldPoint): MapViewportState {
            val viewport = currentViewport ?: error("follow state was not synced")
            if (!following) {
                return viewport
            }
            val scale = scaleForZoom(viewport.zoom)
            return viewport.copy(
                centerWorldX = ownshipWorld.x - anchorOffsetXPx / scale,
                centerWorldY = ownshipWorld.y - anchorOffsetYPx / scale,
            )
        }
    }
}
