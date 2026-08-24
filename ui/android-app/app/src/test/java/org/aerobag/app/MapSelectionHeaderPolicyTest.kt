// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app

import java.io.File
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class MapSelectionHeaderPolicyTest {
    @Test
    fun headerReservesScaledSpaceForBothTextLines() {
        val source = sourceFile("src/main/java/org/aerobag/app/MapExplorerPage.kt").readText()
        val headerBody = balancedBlockAfterMarker(source, "internal fun MapSelectionHeader")

        assertTrue(
            "The two-line selection header must derive its height from scaled text units.",
            headerBody.contains("with(LocalDensity.current) { 34.sp.toDp() }"),
        )
        assertTrue(
            "Both selection-header lines must share one full-size text style.",
            headerBody.contains(
                "val headerTextStyle = MaterialTheme.typography.labelMedium.copy(lineHeight = 15.sp)",
            ) && headerBody.contains("style = headerTextStyle") &&
                headerBody.contains("style = headerTextStyle.copy("),
        )
        assertFalse(
            "The selection-header secondary line must not be shrunk.",
            headerBody.contains("fontSize = 13.sp") || headerBody.contains("lineHeight = 14.sp"),
        )
        assertFalse(
            "The old fixed 0.52-thumb height is smaller than two rendered text lines.",
            headerBody.contains(".height(ThumbSize * 0.52f)"),
        )
    }

    @Test
    fun airportNameAndLocationShareOneReadableTextStyle() {
        val source = sourceFile("src/main/java/org/aerobag/app/MapExplorerPage.kt").readText()
        val modalBody = balancedBlockAfterMarker(source, "internal fun AirportInfoModal")

        assertTrue(
            "Airport name and city/state must use the same text style.",
            modalBody.contains(
                "val airportIdentityStyle = MaterialTheme.typography.labelSmall.copy(fontWeight = FontWeight.Bold)",
            ) && modalBody.split("style = airportIdentityStyle").size - 1 == 2,
        )
    }

    @Test
    fun airportInfoPopupExportsItsSemanticIdentity() {
        val source = sourceFile("src/main/java/org/aerobag/app/MapExplorerPage.kt").readText()
        val modalBody = balancedBlockAfterMarker(source, "internal fun AirportInfoModal")

        assertTrue(
            "Popup content must export its test tag from the popup's separate semantics tree.",
            modalBody.contains(".testTag(\"parity:airport-info-modal:") &&
                modalBody.contains(".semantics { testTagsAsResourceId = true }"),
        )
        assertTrue(
            "The scroll probe must be attached to the scrollable node, not a prunable spacer.",
            modalBody.contains(
                ".testTag(\"parity:airport-info-scroll:${'$'}{scrollState.value}\")\n" +
                    "                .verticalScroll(scrollState)",
            ),
        )
    }

    @Test
    fun rawMapClickUsesCoresInitialPointSelection() {
        val source = sourceFile("src/main/java/org/aerobag/app/MapExplorerPage.kt").readText()
        val requestBody = balancedBlockAfterMarker(source, "fun requestMapSelection")

        assertTrue(
            "Android must honor core's airport-or-SPOT initial selection.",
            requestBody.contains(
                "selectedItem = mapSelectionItemById(result, result.initialSelectedItemId)",
            ),
        )
        assertFalse(
            "Raw map clicks must not open with an empty detail pane.",
            requestBody.contains("selectedItem = null"),
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
