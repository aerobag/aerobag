// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class FlightPlanWaypointLayoutTest {
    @Test
    fun symbolFreeRowsUseTheFullWaypointCell() {
        assertTrue(flightPlanWaypointUsesFullWidthLabel(false, false))
        assertTrue(flightPlanWaypointUsesFullWidthLabel(true, true))
        assertFalse(flightPlanWaypointUsesFullWidthLabel(false, true))
    }
}
