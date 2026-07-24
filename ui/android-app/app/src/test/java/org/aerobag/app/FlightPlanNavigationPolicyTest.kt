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

    private fun sourceFile(path: String): File {
        val start = File(".").canonicalFile
        return generateSequence(start) { it.parentFile }
            .map { File(it, path) }
            .firstOrNull { it.isFile }
            ?: error("could not locate source file $path from $start")
    }
}
