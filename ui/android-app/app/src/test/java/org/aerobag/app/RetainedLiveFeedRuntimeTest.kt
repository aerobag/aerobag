// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app

import kotlinx.coroutines.runBlocking
import org.aerobag.app.domain.LiveFeedInstalledSummary
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class RetainedLiveFeedRuntimeTest {
    @Test
    fun failedCachedProductDoesNotBlockOtherProductsOrNetworkRefresh() = runBlocking {
        val first = summary("metars")
        val second = summary("tafs")
        val promoted = mutableListOf<String>()
        val failures = mutableListOf<String>()
        var networkStarted = false

        runLiveFeedStartup(
            restoreInstalled = { listOf(first, second) },
            awaitInitialPromotion = {},
            promote = { summary ->
                promoted += summary.product
                if (summary.product == "metars") error("corrupt cache")
                true
            },
            onInitialPromotionComplete = {},
            startNetwork = { networkStarted = true },
            reportFailure = { phase, _ -> failures += phase },
        )

        assertEquals(listOf("metars", "tafs"), promoted)
        assertEquals(listOf("cached metars/version promotion"), failures)
        assertTrue(networkStarted)
    }

    @Test
    fun failedPersistedCacheEnumerationStillStartsNetworkRefresh() = runBlocking {
        var networkStarted = false
        val failures = mutableListOf<String>()

        runLiveFeedStartup(
            restoreInstalled = { error("unreadable cache directory") },
            awaitInitialPromotion = {},
            promote = { error("no cached products should be promoted") },
            onInitialPromotionComplete = {},
            startNetwork = { networkStarted = true },
            reportFailure = { phase, _ -> failures += phase },
        )

        assertEquals(listOf("persisted cache restore"), failures)
        assertTrue(networkStarted)
    }

    private fun summary(product: String) = LiveFeedInstalledSummary(
        product = product,
        version = "version",
        stateSha256 = "state",
        payloadKind = "json",
    )
}
