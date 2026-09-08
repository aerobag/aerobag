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
            "Every core-projected picker stage must share the same bounded lazy list.",
            airwayPickerBody.split("LazyColumn(").size == 2 &&
                airwayPickerBody.contains(".weight(1f, fill = false)"),
        )
        assertTrue(
            "Render core sections with dense rows and the standard gutter.",
            airwayPickerBody.contains("picker.sections.forEachIndexed") &&
                airwayPickerBody.contains("section.dense") &&
                airwayPickerBody.contains("section.buttons.chunked(columns)") &&
                airwayPickerBody.contains("Spacer(Modifier.height(ThumbGap))"),
        )
        assertTrue(
            "Buttons must deliver core action IDs without constructing selections.",
            airwayPickerBody.contains("performAirwayPickerAction(button.actionId)") &&
                !source.contains("selectedEntryUid") &&
                !source.contains("prepareAirwayPresentation") &&
                !source.contains("suggestAirwaysNear"),
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
