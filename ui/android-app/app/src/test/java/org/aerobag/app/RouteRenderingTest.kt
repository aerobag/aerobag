// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app

import androidx.compose.ui.geometry.Offset
import org.junit.Assert.assertEquals
import org.junit.Test

class RouteRenderingTest {
    @Test
    fun `spaced chevrons follow each section of the displayed route`() {
        val placements = spacedRouteChevronPlacements(
            path = listOf(Offset(0f, 0f), Offset(30f, 0f), Offset(30f, 40f)),
            spacingPx = 16f,
        )

        assertEquals(
            listOf(
                RouteChevronPlacement(Offset(8f, 0f), 0f),
                RouteChevronPlacement(Offset(24f, 0f), 0f),
                RouteChevronPlacement(Offset(30f, 10f), 90f),
                RouteChevronPlacement(Offset(30f, 26f), 90f),
            ),
            placements,
        )
    }

    @Test
    fun `short manual headings retain one directional chevron`() {
        assertEquals(
            listOf(RouteChevronPlacement(Offset(6f, 4f), 0f)),
            spacedRouteChevronPlacements(
                path = listOf(Offset(2f, 4f), Offset(10f, 4f)),
                spacingPx = 24f,
            ),
        )
    }
}
