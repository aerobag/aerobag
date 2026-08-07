// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app

import java.io.File
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class AltitudePlannerNavigationPolicyTest {
    @Test
    fun flightPlanAndHomeOpenTheStandaloneCoreDrivenPlanner() {
        val mainSource = sourceFile("src/main/java/org/aerobag/app/MainActivity.kt").readText()
        val flightPlanSource = sourceFile("src/main/java/org/aerobag/app/FlightPlanPage.kt").readText()
        val homeSource = sourceFile("src/main/java/org/aerobag/app/HomePage.kt").readText()
        val plannerSource = sourceFile("src/main/java/org/aerobag/app/AltitudePlannerPage.kt").readText()

        assertTrue(mainSource.contains("AppPage.AltitudePlanner ->"))
        assertTrue(
            flightPlanSource.contains(
                ".testTag(\"parity:plan-estimate-mode\")\n                        .clickable { onSelectPage(AppPage.AltitudePlanner) }",
            ),
        )
        assertTrue(homeSource.contains("UiHomeDestination.AltitudePlanner ->"))
        assertTrue(homeSource.contains("iconResId = R.drawable.home_altitude_planner_icon"))
        assertTrue(plannerSource.contains("uiSession.altitudeComparisons()"))
        assertTrue(plannerSource.contains("uiSession.performAltitudePlannerAction(actionUid)"))
        assertTrue(plannerSource.contains("uiSession.setAltitudePlannerDepartureInput(field, input)"))
        assertTrue(plannerSource.contains("setDepartureInput(\"time\", departureTimeInput)"))
        assertTrue(plannerSource.contains("setDepartureInput(\"when\", departureWhenInput)"))
        assertTrue(plannerSource.contains("departureWhenInput = planner.departure.whenValue"))
        assertTrue(plannerSource.contains("uiSession.toggleAltitudePlannerDepartureTimeBasis()"))
        assertFalse(plannerSource.contains("DatePickerDialog"))
        assertFalse(plannerSource.contains("ZonedDateTime"))
        assertTrue(plannerSource.contains("forecast.summary"))
        assertTrue(plannerSource.contains("panel.advisories"))
        assertTrue(plannerSource.contains("CircularProgressIndicator("))
        assertTrue(plannerSource.contains("parity:altitude-comparison-loading"))
        assertTrue(plannerSource.contains(".horizontalScroll(rememberScrollState())"))
        assertTrue(plannerSource.contains("DepartureEditorRow("))
        assertTrue(plannerSource.contains(".height(ThumbSize)"))
        assertTrue(plannerSource.contains("warning = departure.whenIsPast"))
        assertTrue(plannerSource.contains("uiTheme.controls.dataStatusWarningStroke"))
        assertTrue(plannerSource.contains("color = uiTheme.controls.controlGroupBg"))
        assertTrue(plannerSource.contains("color = uiTheme.controls.textInputBg"))
        assertFalse(plannerSource.contains("if (loading) {\n                Text(\"Calculating…\""))
    }

    @Test
    fun flightPlanDoesNotRetainTheOldEmbeddedPlannerImplementation() {
        val source = sourceFile("src/main/java/org/aerobag/app/FlightPlanPage.kt").readText()

        assertFalse(source.contains("altitudePlannerStatusOpen"))
        assertFalse(source.contains("altitudeComparisonOpen"))
        assertFalse(source.contains("uiSession.altitudeComparisons()"))
    }

    private fun sourceFile(path: String): File {
        val start = File(".").canonicalFile
        return generateSequence(start) { it.parentFile }
            .map { File(it, path) }
            .firstOrNull { it.isFile }
            ?: error("could not locate source file $path from $start")
    }
}
