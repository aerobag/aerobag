// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app

import org.junit.Assert.assertEquals
import org.junit.Test

class FlightPlanLayoutTest {
    private val squareControls = List(6) { 64f }
    private val estimateControl = 179.2f

    @Test
    fun wideLayoutKeepsAllControlsOnOneRow() {
        assertEquals(
            1,
            packedFlightPlanControlRowCount(
                availableWidth = 640f,
                itemWidths = squareControls + estimateControl,
                gap = 4f,
            ),
        )
    }

    @Test
    fun portraitPhoneWrapsControlsWithoutClippingThePlannerButton() {
        assertEquals(
            2,
            packedFlightPlanControlRowCount(
                availableWidth = 403f,
                itemWidths = squareControls + estimateControl,
                gap = 4f,
            ),
        )
    }

    @Test
    fun veryNarrowDeviceAllocatesEveryRequiredRow() {
        assertEquals(
            3,
            packedFlightPlanControlRowCount(
                availableWidth = 300f,
                itemWidths = squareControls + estimateControl,
                gap = 4f,
            ),
        )
    }
}
