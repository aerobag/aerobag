// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app

import java.io.File
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class HighRateSessionEffectsBoundaryTest {
    @Test
    fun highRateCoreCommandsRunOffMainAndUseTypedSessionPublications() {
        val source = sourceFile("src/main/java/org/aerobag/app/MainActivity.kt").readText()
        val renderModelSource = sourceFile("src/main/java/org/aerobag/app/SessionRenderModel.kt").readText()
        val helper = balancedBlockAfterMarker(source, "suspend fun runHighRateSessionCommand(")
        val subscription = balancedBlockAfterMarker(
            source,
            "uiSession.subscribeSnapshotPublications",
        )

        assertTrue(helper.contains("withContext(Dispatchers.Default)"))
        assertTrue(source.contains("operation: () -> Unit"))
        assertFalse(helper.contains("applySessionSnapshot"))
        assertTrue(renderModelSource.contains("if (latestSnapshot.get() === snapshot) return true"))
        assertTrue(subscription.contains("snapshotDelivery.submit(publication)"))
        assertFalse(subscription.contains("sessionRenderModel.observe"))
        for (command in listOf("refreshOwnshipSource", "tickPlayback", "tickBadAutopilot")) {
            assertTrue(source.contains("runHighRateSessionCommand(\"$command\""))
            assertFalse(source.contains("applyBackgroundSessionCommand(\"$command\""))
        }
    }

    private fun sourceFile(path: String): File {
        var current = File(requireNotNull(System.getProperty("user.dir"))).absoluteFile
        for (ignored in 0 until 8) {
            val candidate = File(current, path)
            if (candidate.isFile) return candidate
            current = current.parentFile ?: break
        }
        error("unable to locate $path")
    }

    private fun balancedBlockAfterMarker(source: String, marker: String): String {
        val markerIndex = source.indexOf(marker)
        require(markerIndex >= 0) { "missing marker $marker" }
        val start = source.indexOf('{', markerIndex)
        require(start >= 0) { "missing block after $marker" }
        var depth = 0
        for (index in start until source.length) {
            when (source[index]) {
                '{' -> depth += 1
                '}' -> {
                    depth -= 1
                    if (depth == 0) return source.substring(start, index + 1)
                }
            }
        }
        error("unterminated block after $marker")
    }
}
