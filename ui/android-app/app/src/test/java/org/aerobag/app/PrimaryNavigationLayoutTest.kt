// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app

import androidx.compose.ui.unit.dp
import java.io.File
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class PrimaryNavigationLayoutTest {
    private val chartsSource = sourceFile("src/main/java/org/aerobag/app/ChartsPage.kt").readText()

    @Test
    fun sharedBottomDockOwnsAllThreeControls() {
        val dock = sourceBetween(
            chartsSource,
            "internal fun PrimaryNavigationDock(",
            "internal fun PageToggleIndicator(",
        )

        assertTrue(dock.contains("HomePageButton("))
        assertTrue(dock.contains("NavElementDock("))
        assertTrue(dock.contains("ChartPlateToggleButton("))
        assertTrue(dock.contains("ChartPlateReturnButton("))

        val dataStatusSource = sourceFile("src/main/java/org/aerobag/app/DataStatusPage.kt").readText()
        val settingsSource = sourceFile("src/main/java/org/aerobag/app/SettingsPage.kt").readText()
        val pageSources = listOf(
            chartsSource,
            sourceFile("src/main/java/org/aerobag/app/MapExplorerPage.kt").readText(),
            sourceFile("src/main/java/org/aerobag/app/HomePage.kt").readText(),
            sourceFile("src/main/java/org/aerobag/app/FlightPlanPage.kt").readText(),
            dataStatusSource,
            settingsSource,
        )
        val callCount = pageSources.sumOf { Regex("""(?m)^\s{8,}PrimaryNavigationDock\(""").findAll(it).count() }
        assertEquals("Every top-level product page must render the shared dock.", 6, callCount)
        assertFalse(dataStatusSource.contains("HomeReturnDock("))
        assertFalse(settingsSource.contains("HomeReturnDock("))
    }

    @Test
    fun topRowsDoNotOwnPageNavigation() {
        val mapControls = sourceBetween(
            chartsSource,
            "internal fun MapTopLeftControls(",
            "internal fun AndroidChartSearchBox(",
        )
        val plateControls = sourceBetween(
            chartsSource,
            "internal fun ChartViewerSelectors(",
            "internal fun PlateFolderGrid(",
        )

        for (controls in listOf(mapControls, plateControls)) {
            assertFalse(controls.contains("HomePageButton("))
            assertFalse(controls.contains("ChartPlateToggleButton("))
        }
    }

    @Test
    fun ctrAndOrientationFollowSearchAndAreNotBottomDocked() {
        val mapControls = sourceBetween(
            chartsSource,
            "internal fun MapTopLeftControls(",
            "internal fun AndroidChartSearchBox(",
        )
        val searchIndex = mapControls.indexOf("AndroidChartSearchBox(")
        val ctrIndex = mapControls.indexOf("""label = "CTR"""")
        val orientationIndex = mapControls.indexOf("MapOrientationButton(")
        assertTrue("CTR must follow Search in the map top row.", ctrIndex > searchIndex)
        assertTrue("Orientation must follow CTR in the map top row.", orientationIndex > ctrIndex)

        val mapPage = sourceFile("src/main/java/org/aerobag/app/MapExplorerPage.kt").readText()
        assertFalse("CTR must not return to the map's bottom-right corner.", mapPage.contains("""label = "CTR""""))
        assertFalse("Orientation must use the shared top-row control.", mapPage.contains("internal fun MapOrientationButton("))
    }

    @Test
    fun cdiIsOneThumbHigh() {
        val startIndex = chartsSource.indexOf("internal fun NavElementDock(")
        check(startIndex >= 0) { "could not find NavElementDock" }
        val navElement = chartsSource.substring(startIndex)
        assertTrue(navElement.contains(".height(ThumbSize)"))
        assertFalse(navElement.contains("ThumbSize * 0.67f"))
    }

    @Test
    fun narrowSurfacesRaiseCornerControlsByOneRow() {
        val collisionWidth = PrimaryNavigationDockWidth + (BottomRightControlClearance * 2f)
        assertTrue(shouldRaiseBottomCornerControls(collisionWidth - 0.1.dp))
        assertFalse(shouldRaiseBottomCornerControls(collisionWidth))
    }

    private fun sourceBetween(source: String, start: String, end: String): String {
        val startIndex = source.indexOf(start)
        check(startIndex >= 0) { "could not find $start" }
        val endIndex = source.indexOf(end, startIndex)
        check(endIndex > startIndex) { "could not find $end after $start" }
        return source.substring(startIndex, endIndex)
    }

    private fun sourceFile(path: String): File {
        val start = File(".").canonicalFile
        return generateSequence(start) { it.parentFile }
            .map { File(it, path) }
            .firstOrNull { it.isFile }
            ?: error("could not locate source file $path from $start")
    }
}
