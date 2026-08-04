// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app

import java.io.File
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class CompactSquareButtonInputBoundaryTest {
    @Test
    fun sharedButtonsDelegateTapRecognitionToCompose() {
        val source = sourceFile("src/main/java/org/aerobag/app/DebugAndCommonWidgets.kt").readText()
        val button = sourceBetween(
            source,
            "internal fun CompactSquareButton(",
            "internal fun Scrim(",
        )

        assertTrue(
            "Shared buttons must use Compose's touch-slop-aware click recognizer.",
            button.contains(".clickable(") && button.contains("role = Role.Button"),
        )
        assertFalse(
            "Shared buttons must not reject ordinary finger jitter with a custom pointer recognizer.",
            button.contains("pointerInput(") ||
                button.contains("positionChanged()") ||
                button.contains("awaitEachGesture"),
        )
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
