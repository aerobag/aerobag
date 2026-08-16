// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app.domain

import java.util.concurrent.CountDownLatch
import java.util.concurrent.Executors
import java.util.concurrent.TimeUnit
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class PagedSessionOperationMetricsTest {
    @Test
    fun accountsForCoreCallsResourceRoundsLoadsHitsAndIngests() {
        val metrics = PagedSessionOperationMetrics()

        assertEquals("result", metrics.measureCoreCall { "result" })
        val round = metrics.beginResourceRound(
            listOf(CoreResourceRequest("one", CoreResourceSource.PublicUrl("/one"), optional = false)),
        )
        assertEquals(
            3,
            metrics.measureResourceLoad(byteCount = { bytes: ByteArray -> bytes.size }) {
                byteArrayOf(1, 2, 3)
            }.size,
        )
        metrics.recordResourceCacheHit()
        metrics.measureResourceIngest { Unit }
        metrics.finishResourceRound(round)

        val snapshot = metrics.snapshot()
        assertEquals(1, snapshot.coreCallCount)
        assertEquals(1, snapshot.resourceRoundCount)
        assertEquals(2, snapshot.resourceRequestCount)
        assertEquals(1, snapshot.resourceLoadCount)
        assertEquals(1, snapshot.resourceCacheHitCount)
        assertEquals(3L, snapshot.resourceBytes)
        assertTrue(snapshot.coreCallUs >= 0L)
        assertTrue(snapshot.resourceFetchUs >= 0L)
        assertTrue(snapshot.resourceIngestUs >= 0L)
    }

    @Test
    fun recordsFrontierWidthCriticalPathAndConcurrencySeparatelyFromFetchWork() {
        val metrics = PagedSessionOperationMetrics()
        val resources = listOf(
            CoreResourceRequest("one", CoreResourceSource.PublicUrl("/one"), optional = false),
            CoreResourceRequest("two", CoreResourceSource.PublicUrl("/two"), optional = false),
        )
        val round = metrics.beginResourceRound(resources)
        metrics.recordResourceBatch(
            round,
            ResourceFrontierLoadBatch(
                outcomes = listOf(
                    ResourceFrontierLoadOutcome(byteArrayOf(1), null, elapsedUs = 90_000L),
                    ResourceFrontierLoadOutcome(byteArrayOf(2), null, elapsedUs = 80_000L),
                ),
                wallUs = 100_000L,
                maxConcurrency = 2,
            ),
        )
        metrics.measureResourceIngest { Unit }
        metrics.finishResourceRound(round)

        val snapshot = metrics.snapshot()
        assertEquals(170_000L, snapshot.resourceFetchUs)
        assertEquals(1, snapshot.resourceRounds.size)
        assertEquals(100_000L, snapshot.resourceRounds.single().fetchWallUs)
        assertEquals(2, snapshot.resourceRounds.single().maxConcurrency)
        assertEquals(mapOf("public_url" to 2), snapshot.resourceRounds.single().sourceKinds)
    }

    @Test
    fun frontierLoaderStartsKnownLoadsConcurrentlyAndPreservesRequestOrder() {
        val loader = ResourceFrontierLoader(maxParallelism = 4)
        val caller = Executors.newSingleThreadExecutor()
        val allStarted = CountDownLatch(4)
        val release = CountDownLatch(1)
        try {
            val result = caller.submit<ResourceFrontierLoadBatch> {
                loader.load(
                    (0 until 4).map { value ->
                        {
                            allStarted.countDown()
                            check(release.await(2, TimeUnit.SECONDS))
                            byteArrayOf(value.toByte())
                        }
                    },
                )
            }
            assertTrue(allStarted.await(2, TimeUnit.SECONDS))
            release.countDown()
            val batch = result.get(2, TimeUnit.SECONDS)

            assertEquals(4, batch.maxConcurrency)
            assertEquals(listOf(0, 1, 2, 3), batch.outcomes.map { it.bytes?.single()?.toInt() })
        } finally {
            release.countDown()
            caller.shutdownNow()
            loader.close()
        }
    }

    @Test
    fun failedLoadsStillCountTheRequestAndElapsedFetchTime() {
        val metrics = PagedSessionOperationMetrics()

        runCatching {
            metrics.measureResourceLoad(byteCount = { bytes: ByteArray -> bytes.size }) {
                error("unavailable")
            }
        }

        val snapshot = metrics.snapshot()
        assertEquals(1, snapshot.resourceRequestCount)
        assertEquals(0, snapshot.resourceLoadCount)
        assertEquals(0L, snapshot.resourceBytes)
        assertTrue(snapshot.resourceFetchUs >= 0L)
    }
}
