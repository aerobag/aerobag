// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app

import java.io.File
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class WeatherDetailLayoutPolicyTest {
    @Test
    fun weatherModalOwnsTheOnlyWeatherDetailScrollViewport() {
        val source = sourceFile("src/main/java/org/aerobag/app/MapExplorerPage.kt").readText()
        val modalBody = balancedBlockAfterMarker(source, "internal fun WeatherDetailModal")
        val notamBody = balancedBlockAfterMarker(source, "private fun AirportNotamSection")

        assertTrue(
            "The complete weather presentation should scroll as one modal.",
            modalBody.contains(".verticalScroll(rememberScrollState())"),
        )
        assertTrue(
            "METAR and TAF should expand naturally inside the modal scroll viewport.",
            modalBody.split("constrainHeight = false").size - 1 == 2,
        )
        assertFalse(
            "NOTAMs must not introduce a nested lazy scroll viewport.",
            notamBody.contains("LazyColumn("),
        )
        assertFalse(
            "NOTAMs must not be capped to a pinhole inside the modal.",
            notamBody.contains(".heightIn("),
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
