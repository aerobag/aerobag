// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app

import java.io.File
import kotlinx.coroutines.runBlocking
import org.aerobag.app.domain.LiveFeedInstalledSummary
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class RetainedLiveFeedRuntimeTest {
    @Test
    fun mandatoryDisclaimerDefersBackgroundSessionEffects() {
        assertFalse(backgroundSessionEffectsEnabled(disclaimerRequired = true))
        assertTrue(backgroundSessionEffectsEnabled(disclaimerRequired = false))

        val retainedSource = sourceFile("src/main/java/org/aerobag/app/RetainedSession.kt").readText()
        val activitySource = sourceFile("src/main/java/org/aerobag/app/MainActivity.kt").readText()
        assertFalse(retainedSource.contains("retainedSession.liveFeedRuntime.start()"))
        assertTrue(activitySource.contains("liveFeedRuntime.start()"))
        assertTrue(activitySource.contains("if (backgroundEffectsEnabled)"))
    }

    @Test
    fun releasePromotionGateIsEnabledOnlyForE2eArtifacts() {
        val releaseSource = sourceFile("src/release/java/org/aerobag/app/LiveFeedPromotionGate.kt").readText()
        assertTrue(releaseSource.contains("enabled = BuildConfig.AEROBAG_E2E_ENABLED"))
    }

    @Test
    fun persistedWindsAreReportedAsInstallingBeforeTheirPackageIsRead() {
        val source = sourceFile("src/main/java/org/aerobag/app/RetainedLiveFeedRuntime.kt").readText()
        val list = source.substringAfter("private suspend fun listPersistedProducts()")
            .substringBefore("private suspend fun reportWindsAcquisitionPhase")

        assertTrue(
            list.indexOf("listInstalledSummaries(appContext)") <
                list.indexOf("reportWindsAcquisitionPhase(\"installing\")"),
        )
        assertTrue(
            source.indexOf("reportWindsAcquisitionPhase(\"installing\")") <
                source.indexOf("LiveFeedCacheStore.restore("),
        )
        assertTrue(source.contains("finally {\n                                reportWindsAcquisitionPhase(\"idle\")"))
    }

    @Test
    fun failedCachedProductDoesNotBlockOtherProductsOrNetworkRefresh() = runBlocking {
        val first = summary("metars")
        val second = summary("tafs")
        val promoted = mutableListOf<String>()
        val failures = mutableListOf<String>()
        var policyPumpsEnabled = false
        var networkStarted = false

        runLiveFeedStartup(
            listInstalled = { listOf(first, second) },
            awaitInitialPromotion = {},
            restoreInstalled = { installed, onRestored ->
                installed.forEach { onRestored(it) }
                installed
            },
            promote = { summary ->
                promoted += summary.product
                if (summary.product == "metars") error("corrupt cache")
                true
            },
            onInitialPromotionComplete = {},
            enablePolicyPumps = { policyPumpsEnabled = true },
            startNetwork = {
                assertTrue(policyPumpsEnabled)
                networkStarted = true
            },
            reportFailure = { phase, _ -> failures += phase },
        )

        assertEquals(listOf("metars", "tafs"), promoted)
        assertEquals(listOf("cached metars/version promotion"), failures)
        assertTrue(networkStarted)
    }

    @Test
    fun failedPersistedCacheEnumerationStillStartsNetworkRefresh() = runBlocking {
        var networkStarted = false
        var policyPumpsEnabled = false
        val failures = mutableListOf<String>()

        runLiveFeedStartup(
            listInstalled = { error("unreadable cache directory") },
            awaitInitialPromotion = {},
            restoreInstalled = { installed, _ -> installed },
            promote = { error("no cached products should be promoted") },
            onInitialPromotionComplete = {},
            enablePolicyPumps = { policyPumpsEnabled = true },
            startNetwork = {
                assertTrue(policyPumpsEnabled)
                networkStarted = true
            },
            reportFailure = { phase, _ -> failures += phase },
        )

        assertEquals(listOf("persisted cache restore"), failures)
        assertTrue(networkStarted)
    }

    @Test
    fun policyPumpsStayDisabledUntilEveryPersistedProductIsPromoted() = runBlocking {
        val events = mutableListOf<String>()

        runLiveFeedStartup(
            listInstalled = {
                events += "restore"
                listOf(summary("winds-aloft"), summary("nexrad"))
            },
            awaitInitialPromotion = { events += "gate" },
            restoreInstalled = { installed, onRestored ->
                installed.forEach { onRestored(it) }
                installed
            },
            promote = { summary ->
                events += "promote:${summary.product}"
                true
            },
            onInitialPromotionComplete = { events += "promotion-complete" },
            enablePolicyPumps = { events += "policy-pumps-enabled" },
            startNetwork = { events += "network" },
            reportFailure = { phase, cause ->
                throw AssertionError("unexpected $phase failure", cause)
            },
        )

        assertEquals(
            listOf(
                "restore",
                "gate",
                "promote:winds-aloft",
                "promote:nexrad",
                "promotion-complete",
                "policy-pumps-enabled",
                "network",
            ),
            events,
        )
    }

    private fun summary(product: String) = LiveFeedInstalledSummary(
        product = product,
        version = "version",
        stateSha256 = "state",
        payloadKind = "json",
    )

    private fun sourceFile(path: String): File {
        val start = File(".").canonicalFile
        return generateSequence(start) { it.parentFile }
            .map { File(it, path) }
            .firstOrNull { it.isFile }
            ?: error("could not locate source file $path from $start")
    }
}
