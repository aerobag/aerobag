// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app

import java.io.File
import org.junit.Assert.assertTrue
import org.junit.Test

class PlateLoadPresentationTest {
    private val chartsSource = sourceFile("src/main/java/org/aerobag/app/ChartsPage.kt").readText()

    @Test
    fun approachLoaderRendersCoreOwnedHeaderWithSharedWarningColor() {
        assertTrue(chartsSource.contains("headerLabel = plateProcedureLoadMenu.header"))
        assertTrue(chartsSource.contains("ProcedureLoadHeaderTone.Destructive"))
        assertTrue(chartsSource.contains("uiTheme.controls.situationStatusUnavailableFg"))
        assertTrue(chartsSource.contains("initialValue = emptyProcedureLoadMenu,\n        flightPlanRouteRevision,"))
    }

    private fun sourceFile(path: String): File {
        val start = File(".").canonicalFile
        return generateSequence(start) { it.parentFile }
            .map { File(it, path) }
            .firstOrNull { it.isFile }
            ?: error("could not locate source file $path from $start")
    }
}
