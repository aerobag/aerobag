// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app

import java.io.File
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

    @Test
    fun coreProjectedWeatherOverlaysTheExistingWaypointSymbolCell() {
        val models = sourceFile("src/main/java/org/aerobag/app/domain/Models.kt").readText()
        val wireModels = sourceFile("src/main/java/org/aerobag/app/domain/WireModels.kt").readText()
        val adapter = sourceFile("src/main/java/org/aerobag/app/domain/NativeAppCoreAdapter.kt").readText()
        val planDisplay = sourceFile("src/main/java/org/aerobag/app/PlanDisplayWidgets.kt").readText()
        val planWidgets = sourceFile("src/main/java/org/aerobag/app/PlanWidgets.kt").readText()

        assertTrue(models.contains("val weatherBadge: FlightPlanWeatherBadgeUiView?"))
        assertTrue(wireModels.contains("val weather_badge: WireFlightPlanWeatherBadgeUiView? = null"))
        assertTrue(adapter.contains("weatherBadge = weather_badge?.toUi()"))
        assertTrue(planDisplay.contains("weatherBadge = row.weatherBadge"))
        assertTrue(planWidgets.contains("weatherBadge: FlightPlanWeatherBadgeUiView?"))
        assertTrue(planWidgets.contains("drawMetarDisc("))
    }

    private fun sourceFile(path: String): File {
        val direct = File(path)
        if (direct.exists()) return direct
        return File("ui/android-app/app", path)
    }
}
