// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app

import java.io.File
import org.junit.Assert.assertTrue
import org.junit.Test

class TimeDisplayParityTest {
    @Test
    fun androidForwardsEveryBannerCellAndCoreActionsFromFlightPlanCells() {
        val banner = sourceFile("src/main/java/org/aerobag/app/FlightDataBanner.kt").readText()
        val planWidgets = sourceFile("src/main/java/org/aerobag/app/PlanDisplayWidgets.kt").readText()
        val planPage = sourceFile("src/main/java/org/aerobag/app/FlightPlanPage.kt").readText()

        assertTrue(
            "Every flight-data cell must consume taps and forward its ID to core.",
            banner.contains(".flightDataCellInput(cell, onCellActivated)") &&
                banner.contains("pointerInput(cell.id, onCellActivated)") &&
                banner.contains("detectTapGestures(onTap = { onCellActivated(cell.id) })"),
        )
        assertTrue(
            "Flight-plan column headers must forward core-supplied action IDs.",
            planWidgets.contains("column.actionId?.let { actionId ->") &&
                planWidgets.contains("Modifier.clickable { onDataColumnAction(actionId) }"),
        )
        assertTrue(
            "The flight-plan page must send time actions back through the shared core command.",
            planPage.contains("onDataColumnAction = { actionId ->") &&
                planPage.contains("uiSession.performTimeDisplayAction(actionId)"),
        )
    }

    private fun sourceFile(path: String): File {
        val direct = File(path)
        if (direct.exists()) return direct
        return File("ui/android-app/app", path)
    }
}
