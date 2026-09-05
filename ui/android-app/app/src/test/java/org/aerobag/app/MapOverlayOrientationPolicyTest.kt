// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app

import java.io.File
import org.junit.Assert.assertTrue
import org.junit.Test

class MapOverlayOrientationPolicyTest {
    @Test
    fun overlayQueryCarriesDisplayOrientationIntoCoreCollisionPlanning() {
        val source = sourceFile("src/main/java/org/aerobag/app/MapExplorerPage.kt").readText()
        val functionStart = source.indexOf("private fun rememberDisplayedMapOverlay(")
        val functionEnd = source.indexOf("private data class MapRenderPaints", functionStart)
        val function = source.substring(functionStart, functionEnd)

        assertTrue(
            "Map overlay queries must give core the rotated display viewport for upright-label collision planning.",
            function.contains("sessionWorkRunner.submitOverlay(\n            viewport = displayViewport,"),
        )
        assertTrue(
            "Map overlay results remain in the north-up planning frame before display transformation.",
            function.contains("committedViewport = viewport"),
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
