package org.aerobag.app.domain

import java.io.ByteArrayInputStream
import org.junit.Assert.assertArrayEquals
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
}
