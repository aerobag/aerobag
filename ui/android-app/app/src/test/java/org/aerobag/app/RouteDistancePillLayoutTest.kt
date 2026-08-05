// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app

import androidx.compose.ui.geometry.Offset
import org.aerobag.app.domain.FlightPlanRouteDistanceAnnotation
import org.aerobag.app.domain.RouteSegmentStatus
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class RouteDistancePillLayoutTest {
    private val annotation = FlightPlanRouteDistanceAnnotation(
        id = "procedure-leg",
        segmentIndexes = listOf(0, 1),
        text = "20nm",
        distanceNm = 20.0,
        status = RouteSegmentStatus.Active,
        requiredFeatureIds = listOf("start", "end"),
        minimumPathToPillWidthRatio = 1.6,
    )

    @Test
    fun `requires endpoint features and enough projected path length`() {
        val paths = listOf(
            listOf(Offset(0f, 0f), Offset(100f, 0f)),
            listOf(Offset(100f, 0f), Offset(100f, 60f)),
        )
        assertTrue(layoutRouteDistancePills(listOf(annotation), paths, setOf("start")) { 100f }.isEmpty())
        assertTrue(layoutRouteDistancePills(listOf(annotation), paths, setOf("start", "end")) { 101f }.isEmpty())

        val layout = layoutRouteDistancePills(
            listOf(annotation),
            paths,
            setOf("start", "end"),
        ) { 100f }.single()
        assertEquals(Offset(80f, 0f), layout.center)
        assertEquals(0f, layout.rotationDegrees)
    }

    @Test
    fun `aggregates path elements and points the baseline downish`() {
        val layout = layoutRouteDistancePills(
            listOf(annotation.copy(requiredFeatureIds = emptyList())),
            listOf(
                listOf(Offset(0f, 0f), Offset(20f, 0f)),
                listOf(Offset(20f, 0f), Offset(20f, 100f)),
            ),
            emptySet(),
        ) { 50f }.single()
        assertEquals(Offset(20f, 20f), layout.center)
        assertEquals(90f, layout.rotationDegrees)
    }

    @Test
    fun `reverses an upward route baseline`() {
        val layout = layoutRouteDistancePills(
            listOf(annotation.copy(segmentIndexes = listOf(0), requiredFeatureIds = emptyList())),
            listOf(listOf(Offset(0f, 100f), Offset(0f, -100f))),
            emptySet(),
        ) { 50f }.single()
        assertEquals(Offset(0f, 60f), layout.center)
        assertEquals(90f, layout.rotationDegrees)
    }

    @Test
    fun `keeps a northeast route label upright`() {
        val layout = layoutRouteDistancePills(
            listOf(annotation.copy(segmentIndexes = listOf(0), requiredFeatureIds = emptyList())),
            listOf(listOf(Offset(0f, 20f), Offset(200f, 0f))),
            emptySet(),
        ) { 50f }.single()

        assertEquals(-5.7106f, layout.rotationDegrees, 0.001f)
    }
}
