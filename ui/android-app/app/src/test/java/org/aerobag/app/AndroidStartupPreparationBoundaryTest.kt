// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app

import java.io.File
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class AndroidStartupPreparationBoundaryTest {
    private val activitySource = sourceFile(
        "src/main/java/org/aerobag/app/MainActivity.kt",
    ).readText()
    private val retainedSource = sourceFile(
        "src/main/java/org/aerobag/app/RetainedSession.kt",
    ).readText()

    @Test
    fun retainedPreparationStartsBeforeComposeAndUiOnlyAwaitsIt() {
        val onCreate = sourceBetween(activitySource, "override fun onCreate(", "override fun onNewIntent(")
        assertTrue(
            onCreate.indexOf("retainedModel.beginStartupPreparation(") <
                onCreate.indexOf("setContent {")
        )
        assertTrue(activitySource.contains("retainedModel.awaitStartupPreparation("))
        assertTrue(activitySource.contains("retainedModel.preparedCoreSession(fixture)"))
        assertFalse(activitySource.contains("getOrCreateCoreSession("))
    }

    @Test
    fun preparationIsIoOwnedAndResetInvalidatesPendingPublication() {
        assertTrue(retainedSource.contains("SupervisorJob() + Dispatchers.IO"))
        assertTrue(retainedSource.contains("startupScope.async(start = CoroutineStart.LAZY)"))
        assertTrue(retainedSource.contains("startupGeneration += 1"))
        assertTrue(retainedSource.contains("preparation?.cancel()"))
        assertTrue(retainedSource.contains("generation != startupGeneration"))
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
