// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app

import org.aerobag.app.domain.LatLonPoint
import org.aerobag.app.domain.MapViewportState
import org.aerobag.app.domain.dragViewport
import org.aerobag.app.domain.latLonToWorld
import org.junit.Assert.assertFalse
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class ChartSearchInspectionGateTest {
    @Test
    fun newerSearchOrViewportInputRejectsStaleInspectionResult() {
        val gate = ChartSearchInspectionGate()

        val staleSearch = gate.begin()
        gate.invalidate()
        assertFalse(gate.owns(staleSearch))

        val staleViewport = gate.begin()
        gate.invalidate()
        assertFalse(gate.owns(staleViewport))

        val latest = gate.begin()
        assertTrue(gate.owns(latest))
    }

    @Test
    fun viewportProbeMeasuresTargetOffsetFromCurrentGeographicCenter() {
        val destination = LatLonPoint(lat = 47.1039, lon = -122.2872)
        val world = latLonToWorld(destination.lat, destination.lon)
        val centered = MapViewportState(
            centerWorldX = world.x,
            centerWorldY = world.y,
            zoom = 10.0,
        )

        val centeredTag = buildMapSelectionCenterProbeTag(
            targetLabel = "KPLU",
            targetPosition = destination,
            viewport = centered,
            surfaceWidthPx = 1000f,
            surfaceHeightPx = 600f,
        )
        val displacedTag = buildMapSelectionCenterProbeTag(
            targetLabel = "KPLU",
            targetPosition = destination,
            viewport = dragViewport(centered, dx = 120f, dy = -40f),
            surfaceWidthPx = 1000f,
            surfaceHeightPx = 600f,
        )

        assertTrue(centeredTag.endsWith(":offset-px:0"))
        assertFalse(displacedTag.endsWith(":offset-px:0"))
    }

    @Test
    fun centeredInspectorKeepsViewportOwnershipAgainstStaleParentUpdate() {
        val stale = MapViewportState(centerWorldX = 40.0, centerWorldY = 60.0, zoom = 8.0)
        val centered = MapViewportState(centerWorldX = 120.0, centerWorldY = 140.0, zoom = 11.0)

        assertEquals(centered, viewportOwnedByCenteredInspection(stale, centered))
        assertEquals(stale, viewportOwnedByCenteredInspection(stale, null))
    }
}
