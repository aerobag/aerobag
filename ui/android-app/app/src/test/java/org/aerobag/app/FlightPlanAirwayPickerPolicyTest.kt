// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app

import java.io.File
import org.junit.Assert.assertTrue
import org.junit.Test

class FlightPlanAirwayPickerPolicyTest {
    @Test
    fun airwayPickerBoundsAndScrollsLongChoiceLists() {
        val source = sourceFile("src/main/java/org/aerobag/app/FlightPlanPage.kt").readText()
        val airwayPickerBody = balancedBlockAfterMarker(source, "} else if (airwayPicker != null) {")

        assertTrue(
            "The airway tray must stay inside the available flight-plan viewport.",
            airwayPickerBody.contains(".heightIn(max = waypointTrayMaxHeight)"),
        )
        assertTrue(
            "Airway suggestions, entry fixes, and exit fixes must each use a scrollable lazy list.",
            Regex("""LazyColumn\(""").findAll(airwayPickerBody).count() == 3,
        )
        assertTrue(
            "The scrollable choice lists must consume only the panel space left by fixed controls.",
            Regex("""\.weight\(1f,\s*fill = false\)""")
                .findAll(airwayPickerBody)
                .count() == 3,
        )
    }

    private fun sourceFile(path: String): File {
        val start = File(".").canonicalFile
        return generateSequence(start) { it.parentFile }
            .map { File(it, path) }
            .firstOrNull { it.isFile }
            ?: error("could not locate source file $path from $start")
    }

    private fun balancedBlockAfterMarker(source: String, marker: String): String {
        val start = source.indexOf(marker)
        require(start >= 0) { "missing marker $marker" }
        val bodyStart = source.indexOf('{', start)
        require(bodyStart >= 0) { "missing block start after $marker" }
        var depth = 0
        for (index in bodyStart until source.length) {
            when (source[index]) {
                '{' -> depth += 1
                '}' -> {
                    depth -= 1
                    if (depth == 0) {
                        return source.substring(start, index + 1)
                    }
                }
            }
        }
        error("unterminated block after $marker")
    }
}
