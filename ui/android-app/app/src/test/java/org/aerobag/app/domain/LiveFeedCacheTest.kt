package org.aerobag.app.domain

import java.io.ByteArrayInputStream
import java.lang.reflect.Proxy
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.atomic.AtomicInteger
import kotlinx.coroutines.CancellationException
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Assert.fail
import org.junit.Test

class LiveFeedCacheTest {
    @Test
    fun retryGateSuppressesOnlyTheExactFailedRequestUntilCooldownExpires() {
        val gate = LiveFeedRequestRetryGate(retryDelayMs = 300_000)

        assertTrue(gate.shouldAttempt("live_feed_cache/full/tfrs/old", nowMs = 1_000))

        gate.recordFailure("live_feed_cache/full/tfrs/old", nowMs = 1_000)

        assertFalse(gate.shouldAttempt("live_feed_cache/full/tfrs/old", nowMs = 299_999))
        assertTrue(gate.shouldAttempt("live_feed_cache/full/tfrs/new", nowMs = 2_000))
        assertTrue(gate.shouldAttempt("live_feed_cache/full/tfrs/old", nowMs = 301_000))
    }

    @Test
    fun retryGateClearsCooldownAfterSuccess() {
        val gate = LiveFeedRequestRetryGate(retryDelayMs = 300_000)

        gate.recordFailure("live_feed_cache/full/obstacles/v1", nowMs = 1_000)
        gate.recordSuccess("live_feed_cache/full/obstacles/v1")

        assertTrue(gate.shouldAttempt("live_feed_cache/full/obstacles/v1", nowMs = 2_000))
    }

    @Test
    fun retryGateClearsAllCooldownsAfterConnectivityChange() {
        val gate = LiveFeedRequestRetryGate(retryDelayMs = 300_000)

        gate.recordFailure("live_feed_cache/current", nowMs = 1_000)
        gate.recordFailure("live_feed_cache/full/tafs/v1", nowMs = 1_000)
        gate.clearAll()

        assertTrue(gate.shouldAttempt("live_feed_cache/current", nowMs = 2_000))
        assertTrue(gate.shouldAttempt("live_feed_cache/full/tafs/v1", nowMs = 2_000))
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
        val cache = LiveFeedCache(bridge = bridge)

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
    fun cacheCloseWaitsForInFlightNativeUseBeforeDestroyingHandle() {
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
        val cache = LiveFeedCache(bridge = bridge)
        val reader = Thread {
            cache.missingRequests()
        }
        reader.start()

        assertTrue(enteredMissingRequests.await(2, TimeUnit.SECONDS))
        val closer = Thread {
            cache.close()
        }
        closer.start()
        Thread.sleep(50)

        assertEquals("close must not destroy native handle while it is in use", 0, destroyCount.get())

        releaseMissingRequests.countDown()
        reader.join(2_000)
        closer.join(2_000)

        assertEquals(1, destroyCount.get())
        assertFalse(destroyedDuringMissingRequests.get())
    }

    private fun liveFeedBridge(
        missingRequestsJson: () -> String = { "[]" },
        destroyLiveFeedCache: () -> Unit = {},
    ): NativeBridge =
        Proxy.newProxyInstance(
            NativeBridge::class.java.classLoader,
            arrayOf(NativeBridge::class.java),
        ) { _, method, args ->
            when (method.name) {
                "createLiveFeedCache" -> 1L
                "liveFeedCacheMissingRequestsJson" -> missingRequestsJson()
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
}
