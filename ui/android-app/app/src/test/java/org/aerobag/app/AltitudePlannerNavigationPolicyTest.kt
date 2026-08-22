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
        assertTrue(plannerSource.contains("uiSession.performTimeDisplayAction("))
        assertFalse(plannerSource.contains("DatePickerDialog"))
        assertFalse(plannerSource.contains("ZonedDateTime"))
        assertTrue(plannerSource.contains("forecast.summary"))
        assertTrue(plannerSource.contains("actionLabel = forecast.action?.label"))
        assertTrue(plannerSource.contains("performAction(actionUid)"))
        assertTrue(plannerSource.contains("parity:altitude-planner-forecast-action"))
        assertTrue(plannerSource.contains("panel.advisories"))
        assertTrue(plannerSource.contains("CircularProgressIndicator("))
        assertTrue(plannerSource.contains("parity:altitude-comparison-loading"))
        assertTrue(plannerSource.contains("if (userActionLoading || (loading && comparisonPanel == null))"))
        assertTrue(plannerSource.contains(".horizontalScroll(rememberScrollState())"))
        assertTrue(plannerSource.contains("DepartureEditorRow("))
        assertTrue(plannerSource.contains("width = DepartureWhenFieldWidth"))
        assertTrue(plannerSource.contains("DepartureWhenFieldWidth = ThumbSize * 1.25f"))
        assertFalse(plannerSource.contains("departureWhenFieldWidth(whenValue)"))
        assertTrue(plannerSource.contains(".height(ThumbSize)"))
        assertTrue(plannerSource.contains("warning = departure.whenIsPast"))
        assertTrue(plannerSource.contains("uiTheme.controls.dataStatusWarningStroke"))
        assertTrue(plannerSource.contains("color = uiTheme.controls.controlGroupBg"))
        assertTrue(plannerSource.contains("color = uiTheme.controls.textInputBg"))
        assertFalse(plannerSource.contains("if (loading) {\n                Text(\"Calculating…\""))
    }

    @Test
    fun plannerCoreWorkIsSerializedOffTheComposeMainThread() {
        val source = sourceFile("src/main/java/org/aerobag/app/AltitudePlannerPage.kt").readText()

        assertTrue(source.contains("val plannerWorkMutex = remember(uiSession) { Mutex() }"))
        assertTrue(source.contains("comparisonPanel = withContext(Dispatchers.IO)"))
        assertTrue(
            Regex("plannerWorkMutex\\.withLock \\{[\\s\\S]{0,800}uiSession\\.altitudeComparisons\\(\\)")
                .containsMatchIn(source),
        )
        assertTrue(source.contains("val snapshot = withContext(Dispatchers.IO)"))
        assertTrue(
            Regex("plannerWorkMutex\\.withLock \\{[\\s\\S]{0,800}operation\\(\\)")
                .containsMatchIn(source),
        )
        assertTrue(source.contains("operation = { uiSession.performAltitudePlannerAction(actionUid) }"))
        assertTrue(source.contains("operation = { uiSession.setAltitudePlannerDepartureInput(field, input) }"))
        assertTrue(source.contains("uiSession.performTimeDisplayAction(planner.departure.timeDisplayActionId)"))
        assertTrue(source.contains("val plannerProjectionKey = planVersion to planner"))
        assertTrue(
            source.contains(
                "LaunchedEffect(page, plannerProjectionKey, comparisonRefreshRevision)",
            ),
        )
        assertTrue(source.contains("comparisonRefreshRevision += 1"))
        assertTrue(source.contains("pendingUserRefreshRevision = comparisonRefreshRevision"))
        assertTrue(source.contains("userActionsInFlight > 0 && pendingUserRefreshRevision == null"))
        assertTrue(source.contains("it <= requestRefreshRevision"))
        assertTrue(source.contains("userActionsInFlight > 0 || pendingUserRefreshRevision != null"))
        assertFalse(source.contains("pendingUserProjectionFrom"))
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
