// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app

import java.io.File
import org.junit.Assert.assertFalse
import org.junit.Assert.assertEquals
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
    fun dataGridPinsWaypointsAndScrollsReadableDataColumnsTogether() {
        assertEquals(ThumbSize * 2f, PlanWaypointColumnWidth)
        assertEquals(ThumbSize * 0.2f, PlanChildWaypointIndent)
        assertEquals(ThumbSize * 0.78f, PlanWaypointSymbolTextReserve)
        assertEquals(ThumbSize * 0.54f, PlanChildWaypointSymbolTextReserve)
        assertEquals(
            ThumbSize.value * 1.8f,
            (PlanWaypointColumnWidth - PlanChildWaypointIndent).value,
            0.001f,
        )
        assertEquals(ThumbSize, PlanMinimumDataColumnWidth)
        assertEquals(ThumbSize, planDataColumnWidth(ThumbSize * 6f, 5))
        assertEquals(
            ThumbSize.value * 1.2f,
            planDataColumnWidth(ThumbSize * 8f + PlanGridGap * 5, 5).value,
            0.001f,
        )

        val page = sourceFile("src/main/java/org/aerobag/app/FlightPlanPage.kt").readText()
        val display = sourceFile("src/main/java/org/aerobag/app/PlanDisplayWidgets.kt").readText()
        val widgets = sourceFile("src/main/java/org/aerobag/app/PlanWidgets.kt").readText()
        assertTrue(page.contains("val planDataScrollState = rememberScrollState()"))
        assertTrue(display.contains("dataScrollState = dataScrollState"))
        assertTrue(widgets.contains(".horizontalScroll(dataScrollState)"))
        assertTrue(widgets.contains(".width(dataColumnWidth)"))
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
