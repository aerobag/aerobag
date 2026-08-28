// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app

import java.io.File
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class FlightPlanNavigationPolicyTest {
    @Test
    fun focusedAppendRouteFieldDoesNotBlockPageNavigation() {
        val source = sourceFile("src/main/java/org/aerobag/app/FlightPlanPage.kt").readText()

        assertTrue(
            "HOME navigation should invoke its destination directly even while route entry is focused.",
            source.contains("onHomeClick = { onSelectPage(AppPage.Home) }"),
        )
        assertTrue(
            "Chart/plate navigation should invoke its destination directly even while route entry is focused.",
            source.contains("onOpenChartOrPlate = onOpenRecentChartOrPlate"),
        )
        assertFalse(
            "Focused append-route input must not install a timing gate over page navigation.",
            source.contains("routeEntrySuppressNavigationUntilMs") ||
                source.contains("performRouteEntryNavigation"),
        )
    }

    @Test
    fun flightPlanControlsRenderCoreSelectedState() {
        val pageSource = sourceFile("src/main/java/org/aerobag/app/FlightPlanPage.kt").readText()
        val inspectorSource = sourceFile("src/main/java/org/aerobag/app/MapExplorerPage.kt").readText()
        val commonWidgetsSource =
            sourceFile("src/main/java/org/aerobag/app/DebugAndCommonWidgets.kt").readText()

        assertTrue(pageSource.contains("selected = control.selected"))
        assertTrue(pageSource.contains("SelectedControlHighlightFrame("))
        assertTrue(inspectorSource.contains("SelectedControlHighlightFrame("))
        assertTrue(commonWidgetsSource.contains("selected -> selectedColor ?: uiTheme.controls.buttonChecked"))
        assertTrue(commonWidgetsSource.contains("Modifier.border(2.dp, uiTheme.controls.buttonChecked, outerShape)"))
        assertTrue(commonWidgetsSource.contains("Modifier.border(2.dp, uiTheme.controls.buttonFg, gapShape)"))
    }

    @Test
    fun flightPlanControlsRenderCoreProjectedSharedVectorSymbols() {
        val pageSource = sourceFile("src/main/java/org/aerobag/app/FlightPlanPage.kt").readText()
        val commonWidgetsSource =
            sourceFile("src/main/java/org/aerobag/app/DebugAndCommonWidgets.kt").readText()

        assertTrue(pageSource.contains("actionSymbolId = control.symbolId"))
        assertTrue(commonWidgetsSource.contains("actionSymbolId: String? = null"))
        assertTrue(commonWidgetsSource.contains("ActionIcon("))
        assertTrue(commonWidgetsSource.contains("actionId = actionSymbolId"))
    }

    private fun sourceFile(path: String): File {
        val start = File(".").canonicalFile
        return generateSequence(start) { it.parentFile }
            .map { File(it, path) }
            .firstOrNull { it.isFile }
            ?: error("could not locate source file $path from $start")
    }
}
