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
        val helperBody = Regex(
            """fun performRouteEntryNavigation\(action: \(\) -> Unit\) \{(?<body>.*?)\n    \}""",
            RegexOption.DOT_MATCHES_ALL,
        ).find(source)?.groups?.get("body")?.value
            ?: error("performRouteEntryNavigation helper not found")

        assertTrue(
            "Flight-plan page navigation should clear append-route focus before navigating.",
            helperBody.contains("focusManager.clearFocus(force = true)"),
        )
        assertTrue(
            "Flight-plan page navigation should still run the requested navigation action.",
            helperBody.contains("action()"),
        )
        assertFalse(
            "Focused append-route input must not suppress HOME/CHART/CDI navigation.",
            helperBody.contains("routeEntryFocused"),
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
