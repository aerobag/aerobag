// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app.domain

import java.io.ByteArrayInputStream
import java.lang.reflect.Proxy
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.atomic.AtomicInteger
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.async
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Assert.fail
import org.junit.Test

class LiveFeedCacheTest {
    @Test
    fun cleanSseCloseUsesCoreReconnectCommand() {
        val decision = runtimeDecision(
            LiveFeedRuntimeCommand(kind = "reconnect", delayMs = 5_000),
        )

        assertEquals(5_000L, reconnectDelayMs(decision))
    }

    @Test
    fun failedResourceUsesCoreRetryWakeupCommand() {
        val decision = runtimeDecision(
            LiveFeedRuntimeCommand(kind = "retry_resources", delayMs = 300_000),
        )

        assertEquals(300_000L, retryResourcesDelayMs(decision))
    }

    @Test
    fun boundedLiveFeedReadAcceptsPayloadAtLimit() {
        val bytes = byteArrayOf(1, 2, 3, 4)

        val actual = readLiveFeedBytesBounded(
            input = ByteArrayInputStream(bytes),
            maxBytes = bytes.size.toLong(),
            url = "http://example.test/live-feeds/state.json",
        )

        assertArrayEquals(bytes, actual)
    }

    @Test
    fun boundedLiveFeedReadRejectsPayloadPastLimit() {
        try {
            readLiveFeedBytesBounded(
                input = ByteArrayInputStream(byteArrayOf(1, 2, 3, 4, 5)),
                maxBytes = 4,
                url = "http://example.test/live-feeds/state.json",
            )
            fail("expected oversized live-feed response to be rejected")
        } catch (error: LiveFeedResponseTooLargeException) {
            assertTrue(error.message.orEmpty().contains("observedBytes=5"))
            assertTrue(error.message.orEmpty().contains("maxBytes=4"))
        }
    }

    @Test
    fun cacheCloseIsIdempotentAndLateUseIsCancellation() {
        val destroyCount = AtomicInteger(0)
        val bridge = liveFeedBridge(
            destroyLiveFeedCache = {
                destroyCount.incrementAndGet()
            },
        )
        val cache = LiveFeedCache(sourceRootUrl = "http://live.test", bridge = bridge)

        assertTrue(cache.missingRequests().isEmpty())

        cache.close()
        cache.close()

        assertEquals(1, destroyCount.get())
        assertTrue(cache.isClosed)
        try {
            cache.missingRequests()
            fail("expected closed live-feed cache to reject late native use")
        } catch (error: CancellationException) {
            assertTrue(error.message.orEmpty().contains("closed"))
        }
    }

    @Test
    fun cacheCloseReturnsWithoutDestroyingHandleStillInUse() {
        val enteredMissingRequests = CountDownLatch(1)
        val releaseMissingRequests = CountDownLatch(1)
        val inMissingRequests = AtomicBoolean(false)
        val destroyedDuringMissingRequests = AtomicBoolean(false)
        val destroyCount = AtomicInteger(0)
        val bridge = liveFeedBridge(
            missingRequestsJson = {
                inMissingRequests.set(true)
                enteredMissingRequests.countDown()
                assertTrue(releaseMissingRequests.await(2, TimeUnit.SECONDS))
                inMissingRequests.set(false)
                "[]"
            },
            destroyLiveFeedCache = {
                destroyedDuringMissingRequests.set(inMissingRequests.get())
                destroyCount.incrementAndGet()
            },
        )
        val cache = LiveFeedCache(sourceRootUrl = "http://live.test", bridge = bridge)
        val reader = Thread {
            cache.missingRequests()
        }
        reader.start()

        assertTrue(enteredMissingRequests.await(2, TimeUnit.SECONDS))
        val closer = Thread {
            cache.close()
        }
        closer.start()
        closer.join(2_000)

        assertFalse("close must not wait for an in-flight native call", closer.isAlive)
        assertEquals("close must not destroy native handle while it is in use", 0, destroyCount.get())

        releaseMissingRequests.countDown()
        reader.join(2_000)

        assertEquals(1, destroyCount.get())
        assertFalse(destroyedDuringMissingRequests.get())
    }

    @Test
    fun slowPersistedPackagePreparationDoesNotBlockCacheQueries() {
        val enteredRestore = CountDownLatch(1)
        val releaseRestore = CountDownLatch(1)
        val enteredQuery = CountDownLatch(1)
        val bridge = liveFeedBridge(
            ingestInstalledPayload = {
                enteredRestore.countDown()
                assertTrue(releaseRestore.await(2, TimeUnit.SECONDS))
            },
            missingRequestsJson = {
                enteredQuery.countDown()
                "[]"
            },
        )
        val cache = LiveFeedCache(sourceRootUrl = "http://live.test", bridge = bridge)
        val restore = Thread {
            cache.ingestInstalledPayload(
                LiveFeedInstalledSummary(
                    product = "nexrad",
                    version = "stale-frame",
                    stateSha256 = "state",
                    payloadKind = "nexrad_package",
                ),
                byteArrayOf(1, 2, 3),
            )
        }
        restore.start()
        assertTrue(enteredRestore.await(2, TimeUnit.SECONDS))

        val query = Thread { cache.missingRequests() }
        query.start()
        assertTrue(
            "persisted package preparation must not hold the cache operation lock",
            enteredQuery.await(500, TimeUnit.MILLISECONDS),
        )

        releaseRestore.countDown()
        restore.join(2_000)
        query.join(2_000)
        cache.close()
    }

    @Test
    fun slowImmutableResourceProjectionDoesNotBlockCacheQueries() {
        val enteredFinish = CountDownLatch(1)
        val releaseFinish = CountDownLatch(1)
        val enteredQuery = CountDownLatch(1)
        val bridge = liveFeedBridge(
            beginRestoringResources = {},
            restoreResourceBytes = {},
            finishRestoringResources = {
                enteredFinish.countDown()
                assertTrue(releaseFinish.await(2, TimeUnit.SECONDS))
            },
            missingRequestsJson = {
                enteredQuery.countDown()
                "[]"
            },
        )
        val cache = LiveFeedCache(sourceRootUrl = "http://live.test", bridge = bridge)
        val restore = Thread {
            cache.restoreInstalledResources(resourceManifest()) { byteArrayOf(1, 2, 3) }
        }
        restore.start()
        assertTrue(enteredFinish.await(2, TimeUnit.SECONDS))

        val query = Thread { cache.missingRequests() }
        query.start()
        assertTrue(
            "immutable resource projection must not hold the cache operation lock",
            enteredQuery.await(500, TimeUnit.MILLISECONDS),
        )

        releaseFinish.countDown()
        restore.join(2_000)
        query.join(2_000)
        cache.close()
    }

    @Test
    fun persistedRestoreRunsOnlyOnceForRetainedCache() = runBlocking {
        val cache = LiveFeedCache(sourceRootUrl = "http://live.test", bridge = liveFeedBridge())
        val restoreCount = AtomicInteger(0)

        assertTrue(cache.restorePersistedOnce { restoreCount.incrementAndGet() })
        assertFalse(cache.restorePersistedOnce { restoreCount.incrementAndGet() })

        assertEquals(1, restoreCount.get())
        cache.close()
    }

    @Test
    fun failedPersistedRestoreCanBeRetried() = runBlocking {
        val cache = LiveFeedCache(sourceRootUrl = "http://live.test", bridge = liveFeedBridge())
        val restoreCount = AtomicInteger(0)

        runCatching {
            cache.restorePersistedOnce {
                restoreCount.incrementAndGet()
                error("interrupted")
            }
        }
        cache.restorePersistedOnce { restoreCount.incrementAndGet() }

        assertEquals(2, restoreCount.get())
        cache.close()
    }

    @Test
    fun persistedWindAndMetarsAreReadyBeforeOtherProductsRestoreInParallel() = runBlocking {
        val events = java.util.concurrent.CopyOnWriteArrayList<String>()
        val bulkStarts = AtomicInteger(0)
        val twoBulkRestoresStarted = CompletableDeferred<Unit>()
        val releaseBulkRestores = CompletableDeferred<Unit>()
        val wind = summary("winds-aloft")
        val metars = summary("metars")
        val nexrad = summary("nexrad")
        val tfrs = summary("tfrs")

        val restore = async {
            restorePersistedProductsInPriorityOrder(
                installed = listOf(metars, nexrad, wind, tfrs),
                restoreOne = { item ->
                    events += "restore:${item.product}"
                    if (item.product != "winds-aloft" && item.product != "metars") {
                        if (bulkStarts.incrementAndGet() == 2) {
                            twoBulkRestoresStarted.complete(Unit)
                        }
                        releaseBulkRestores.await()
                    }
                    true
                },
                onRestored = { item -> events += "promote:${item.product}" },
            )
        }

        twoBulkRestoresStarted.await()
        assertEquals(
            listOf(
                "restore:winds-aloft",
                "promote:winds-aloft",
                "restore:metars",
                "promote:metars",
            ),
            events.take(4),
        )
        releaseBulkRestores.complete(Unit)
        val restored = restore.await()
        assertEquals(wind, restored.first())
        assertEquals(metars, restored[1])
        assertEquals(setOf(nexrad, tfrs), restored.drop(2).toSet())
    }

    @Test
    fun failedResourceRestoreAbortsNativeTransactionAndAllowsRetry() {
        val active = AtomicBoolean(false)
        val beginCount = AtomicInteger(0)
        val abortCount = AtomicInteger(0)
        val finishCount = AtomicInteger(0)
        val bridge = liveFeedBridge(
            beginRestoringResources = {
                check(active.compareAndSet(false, true)) { "restore already active" }
                beginCount.incrementAndGet()
            },
            abortRestoringResources = {
                active.set(false)
                abortCount.incrementAndGet()
            },
            finishRestoringResources = {
                active.set(false)
                finishCount.incrementAndGet()
            },
        )
        val cache = LiveFeedCache(sourceRootUrl = "http://live.test", bridge = bridge)
        val manifest = resourceManifest()

        val failed = runCatching {
            cache.restoreInstalledResources(manifest) { error("missing persisted blob") }
        }
        assertTrue(failed.exceptionOrNull()?.message.orEmpty().contains("missing persisted blob"))

        cache.restoreInstalledResources(manifest) { byteArrayOf(1, 2, 3) }

        assertEquals(2, beginCount.get())
        assertEquals(1, abortCount.get())
        assertEquals(1, finishCount.get())
        assertFalse(active.get())
        cache.close()
    }

    @Test
    fun failedResourceRestoreFinishIsAbortedWithoutReplacingOriginalError() {
        val abortCount = AtomicInteger(0)
        val bridge = liveFeedBridge(
            beginRestoringResources = {},
            restoreResourceBytes = {},
            finishRestoringResources = { error("invalid restored product") },
            abortRestoringResources = { abortCount.incrementAndGet() },
        )
        val cache = LiveFeedCache(sourceRootUrl = "http://live.test", bridge = bridge)

        val failed = runCatching {
            cache.restoreInstalledResources(resourceManifest()) { byteArrayOf(1, 2, 3) }
        }

        assertTrue(failed.exceptionOrNull()?.message.orEmpty().contains("invalid restored product"))
        assertEquals(1, abortCount.get())
        cache.close()
    }

    private fun liveFeedBridge(
        missingRequestsJson: () -> String = { "[]" },
        destroyLiveFeedCache: () -> Unit = {},
        beginRestoringResources: (String) -> Unit = {},
        restoreResourceBytes: () -> Unit = {},
        abortRestoringResources: () -> Unit = {},
        finishRestoringResources: () -> Unit = {},
        ingestInstalledPayload: () -> Unit = {},
    ): NativeBridge =
        Proxy.newProxyInstance(
            NativeBridge::class.java.classLoader,
            arrayOf(NativeBridge::class.java),
        ) { _, method, args ->
            when (method.name) {
                "createLiveFeedCache" -> 1L
                "liveFeedEventsUrl" -> "http://live.test/live-feeds/v3/events"
                "liveFeedStatusUrl" -> "http://live.test/live-feeds/status.html"
                "normalizeLiveFeedSourceRootUrl" -> args?.first() as String
                "liveFeedCacheMissingRequestsJson" -> missingRequestsJson()
                "liveFeedCacheBeginRestoringResources" -> beginRestoringResources(args?.get(1) as String)
                "liveFeedCacheRestoreResourceBytes" -> restoreResourceBytes()
                "liveFeedCacheAbortRestoringResources" -> abortRestoringResources()
                "liveFeedCacheFinishRestoringResources" -> finishRestoringResources()
                "liveFeedCacheIngestInstalledPayloadBytes" -> ingestInstalledPayload()
                "destroyLiveFeedCache" -> {
                    destroyLiveFeedCache()
                    Unit
                }
                "equals" -> false
                "hashCode" -> 0
                "toString" -> "LiveFeedCacheTestBridge"
                else -> error("unexpected NativeBridge call in LiveFeedCacheTest: ${method.name}")
            }
        } as NativeBridge

    private fun resourceManifest() = LiveFeedResourceManifest(
        summary = LiveFeedInstalledSummary(
            product = "notams",
            version = "version",
            stateSha256 = "state",
            payloadKind = "notam_resources",
        ),
        resources = listOf(
            LiveFeedResourceRef(
                kind = "notam_checkpoint_xz",
                blobSha256 = "a".repeat(64),
                bytes = 3,
            ),
        ),
    )

    private fun summary(product: String) = LiveFeedInstalledSummary(
        product = product,
        version = "version",
        stateSha256 = "state",
        payloadKind = "json",
    )

    private fun runtimeDecision(command: LiveFeedRuntimeCommand) = LiveFeedRuntimeDecision(
        transportPolicy = SseTransportPolicy(
            heartbeatIntervalMs = 30_000,
            connectTimeoutMs = 5_000,
            idleTimeoutMs = 65_000,
            reconnectInitialDelayMs = 5_000,
            reconnectMaxDelayMs = 65_000,
        ),
        commands = listOf(command),
    )
}
