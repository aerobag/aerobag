// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app

import java.io.File
import org.junit.Assert.assertTrue
import org.junit.Test

class AndroidGpsNotificationBoundaryTest {
    private val serviceSource = sourceFile(
        "src/main/java/org/aerobag/app/AerobagGpsService.kt",
    ).readText()
    private val activitySource = sourceFile(
        "src/main/java/org/aerobag/app/MainActivity.kt",
    ).readText()

    @Test
    fun dismissingActiveNotificationStopsGpsAndRequestsCorePause() {
        val notificationBuilder = sourceBetween(
            serviceSource,
            "private fun buildActiveNotification()",
            "private fun buildPausedNotification()",
        )
        assertTrue(notificationBuilder.contains(".setDeleteIntent("))
        assertTrue(
            notificationBuilder.contains(
                "serviceIntent(AndroidGpsPower.NotificationDismissedAction, 3)",
            ),
        )

        val startHandler = sourceBetween(
            serviceSource,
            "override fun onStartCommand(",
            "override fun onDestroy()",
        )
        assertTrue(
            startHandler.contains(
                "AndroidGpsPower.requestControl(this, SituationControlInput.Pause)",
            ),
        )
        assertTrue(startHandler.contains("pauseGps(postPausedNotification = false)"))
    }

    @Test
    fun pendingNotificationControlIsAppliedAndAcknowledgedThroughCoreSession() {
        val ownshipEffects = sourceBetween(
            activitySource,
            "LaunchedEffect(uiSession) {",
            "LaunchedEffect(perfScenario?.id, uiSession)",
        )
        assertTrue(ownshipEffects.contains("AndroidGpsPower.pendingControl(appContext)"))
        assertTrue(ownshipEffects.contains("uiSession.applySituationControlInput("))
        assertTrue(ownshipEffects.contains("AndroidGpsPower.acknowledgeControl(appContext, input)"))
        assertTrue(ownshipEffects.contains("AndroidGpsPower.controlRequests.collect"))
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
