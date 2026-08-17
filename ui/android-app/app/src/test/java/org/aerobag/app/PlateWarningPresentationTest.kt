// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app

import java.io.File
import org.junit.Assert.assertTrue
import org.junit.Test

class PlateWarningPresentationTest {
    private val chartsSource = sourceFile("src/main/java/org/aerobag/app/ChartsPage.kt").readText()

    @Test
    fun viewerUsesStandardStatusTrayAndFolderUsesItsCompactFace() {
        val chartsPage = sourceBetween(
            "internal fun ChartsPage(",
            "internal data class PlateOwnshipOverlay(",
        )
        val folder = sourceBetween(
            "internal fun PlateFolderGrid(",
            "internal fun MenuDock(",
        )

        assertTrue(chartsPage.contains("dataStatusState = procedureGeometryStatus"))
        assertTrue(chartsPage.contains("open = procedureWarningTrayOpen"))
        assertTrue(folder.contains("DataStatusBadgeFace("))
        assertTrue(folder.contains("count = chart.procedureGeometryWarningCount.toString()"))
        assertTrue(folder.contains(".align(Alignment.TopEnd)"))
    }

    @Test
    fun folderUsesIndependentCoreModeledProcedureNotamBadgeAndModal() {
        val chartsPage = sourceBetween(
            "internal fun ChartsPage(",
            "internal data class PlateOwnshipOverlay(",
        )
        val folder = sourceBetween(
            "internal fun PlateFolderGrid(",
            "internal fun MenuDock(",
        )

        assertTrue(folder.contains("chart.procedureNotamBadge?.let"))
        assertTrue(folder.contains("onOpenProcedureNotams(badge.detail)"))
        assertTrue(folder.contains("badge.accessibilityLabel"))
        assertTrue(folder.contains("parity:plate-notam:${'$'}{badge.actionId}"))
        assertTrue(folder.contains("shape = RectangleShape"))
        assertTrue(folder.contains("badgeSize = 22.dp"))
        assertTrue(chartsPage.contains("badgeSize = ThumbSize * 0.5f"))
        assertTrue(chartsPage.contains("procedureNotamBadge?.takeUnless { folderOpen }"))
        assertTrue(chartsPage.contains("procedureNotamDetail = badge.detail"))
        assertTrue(chartsPage.contains("Popup("))
        assertTrue(chartsPage.contains("ProcedureNotamModal("))
    }

    private fun sourceBetween(start: String, end: String): String {
        val startIndex = chartsSource.indexOf(start)
        check(startIndex >= 0) { "could not find $start" }
        val endIndex = chartsSource.indexOf(end, startIndex)
        check(endIndex > startIndex) { "could not find $end after $start" }
        return chartsSource.substring(startIndex, endIndex)
    }

    private fun sourceFile(path: String): File {
        val start = File(".").canonicalFile
        return generateSequence(start) { it.parentFile }
            .map { File(it, path) }
            .firstOrNull { it.isFile }
            ?: error("could not locate source file $path from $start")
    }
}
