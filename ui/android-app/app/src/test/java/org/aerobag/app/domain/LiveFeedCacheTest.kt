package org.aerobag.app.domain

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
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
}
