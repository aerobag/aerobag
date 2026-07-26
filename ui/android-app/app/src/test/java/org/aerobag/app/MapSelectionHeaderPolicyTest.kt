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
            "The selection header must retain the declared 15sp and 14sp line heights within its 34sp budget.",
            headerBody.contains("lineHeight = 15.sp") &&
                headerBody.contains("lineHeight = 14.sp"),
        )
        assertFalse(
            "The old fixed 0.52-thumb height is smaller than two rendered text lines.",
            headerBody.contains(".height(ThumbSize * 0.52f)"),
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
