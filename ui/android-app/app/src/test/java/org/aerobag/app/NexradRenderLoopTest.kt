// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app

import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.async
import kotlinx.coroutines.awaitCancellation
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.coroutines.test.advanceTimeBy
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

@OptIn(ExperimentalCoroutinesApi::class)
class NexradRenderLoopTest {
    @Test
    fun renderingLatencyDoesNotPostponeTheCoreFrameBoundary() = runTest {
        val requests = Channel<Unit>(Channel.CONFLATED)
        val started = mutableListOf<Long>()
        val loop = backgroundScope.launch {
            runNexradRenderLoop(requests, { testScheduler.currentTime }, { 100_000L + testScheduler.currentTime }, {
                started += testScheduler.currentTime
                if (started.size == 1) {
                    delay(800) // Query delivery and image decoding consume most of the frame.
                    NexradRenderSchedule(nextUpdateEpochMs = 101_000)
                } else null
            }, { throw AssertionError(it) })
        }
        requests.send(Unit)
        advanceTimeBy(1_000)
        runCurrent()
        assertEquals(listOf(0L, 1_000L), started)
        loop.cancel()
    }

    @Test
    fun slowPrefetchCannotBlockAnimationOrAnExplicitModeChange() = runTest {
        val requests = Channel<Unit>(Channel.CONFLATED)
        val prefetchStarted = CompletableDeferred<Unit>()
        val releasePrefetch = CompletableDeferred<Unit>()
        var renders = 0
        val loop = backgroundScope.launch {
            runNexradRenderLoop(requests, { testScheduler.currentTime }, { testScheduler.currentTime }, {
                renders++
                NexradRenderSchedule(1_000L.takeIf { renders == 1 }) {
                    prefetchStarted.complete(Unit)
                    releasePrefetch.await()
                }
            }, { throw AssertionError(it) })
        }
        requests.send(Unit)
        runCurrent()
        assertTrue(prefetchStarted.isCompleted)
        advanceTimeBy(1_000)
        runCurrent()
        assertEquals("frame deadline is independent of pending prefetch", 2, renders)
        requests.send(Unit) // e.g. hold-latest or resume animation
        runCurrent()
        assertEquals(3, renders)
        assertFalse(releasePrefetch.isCompleted)
        loop.cancel()
    }

    @Test
    fun prefetchRetainsOnlyLatestPendingBatchAndStopsWhenTheOwnerCloses() = runTest {
        val requests = Channel<Unit>(Channel.CONFLATED)
        val release = CompletableDeferred<Unit>()
        val prefetched = mutableListOf<Int>()
        var renders = 0
        var cancelled = false
        val loop = backgroundScope.launch {
            runNexradRenderLoop(requests, { testScheduler.currentTime }, { testScheduler.currentTime }, {
                val frame = ++renders
                NexradRenderSchedule(null) {
                    prefetched += frame
                    if (frame == 1) release.await() else try {
                        awaitCancellation()
                    } finally { cancelled = true }
                }
            }, { throw AssertionError(it) })
        }
        repeat(3) { requests.send(Unit); runCurrent() }
        assertEquals(3, renders)
        assertEquals(listOf(1), prefetched)
        release.complete(Unit)
        runCurrent()
        assertEquals(listOf(1, 3), prefetched)
        requests.close()
        runCurrent()
        assertTrue(loop.isCompleted)
        assertTrue(cancelled)
    }

    @Test
    fun expiredFrameThenBlankThenHoldDoNotAddDelaysOrSpin() = runTest {
        val requests = Channel<Unit>(Channel.CONFLATED)
        val starts = mutableListOf<Long>()
        backgroundScope.launch {
            runNexradRenderLoop(requests, { testScheduler.currentTime }, { testScheduler.currentTime }, {
                starts += testScheduler.currentTime
                when (starts.size) {
                    1 -> { delay(1_200); NexradRenderSchedule(1_000) }
                    2 -> NexradRenderSchedule(1_500) // Blank phase still has a deadline.
                    else -> NexradRenderSchedule(null) // Hold latest / hidden / idle.
                }
            }, { throw AssertionError(it) })
        }
        requests.send(Unit)
        advanceTimeBy(10_000)
        runCurrent()
        assertEquals(listOf(0L, 1_200L, 1_500L), starts)
    }

    @Test
    fun failedRenderAndFailedPrefetchDoNotOrphanAnimation() = runTest {
        val requests = Channel<Unit>(Channel.CONFLATED)
        val errors = mutableListOf<String?>()
        var renders = 0
        backgroundScope.launch {
            runNexradRenderLoop(requests, { testScheduler.currentTime }, { testScheduler.currentTime }, {
                renders++
                when (renders) {
                    1 -> error("render failed")
                    2 -> NexradRenderSchedule(2_000) { error("prefetch failed") }
                    else -> null
                }
            }, { errors += it.message })
        }
        requests.send(Unit)
        advanceTimeBy(2_000)
        runCurrent()
        assertEquals(3, renders)
        assertEquals(listOf("render failed", "prefetch failed"), errors)
    }

    @Test
    fun paintingJoinsPrefetchOfTheSameImageWithoutDuplicateCoreRequests() = runTest {
        val loads = NexradTileLoads(backgroundScope)
        val release = CompletableDeferred<ByteArray>()
        var fetches = 0
        val prefetch = backgroundScope.async { loads.load("tile") { fetches++; release.await() } }
        runCurrent()
        val paint = backgroundScope.async { loads.load("tile") { error("duplicate core request") } }
        runCurrent()
        prefetch.cancel() // Canceling one consumer must not cancel the shared load.
        release.complete(byteArrayOf(1, 2))
        runCurrent()
        assertEquals(1, fetches)
        assertTrue(paint.await().contentEquals(byteArrayOf(1, 2)))
        val retry = backgroundScope.async { runCatching { loads.load("tile") { error("retired") } } }
        runCurrent()
        assertEquals("retired", retry.await().exceptionOrNull()?.message)
        val next = backgroundScope.async { loads.load("tile") { byteArrayOf(3) } }
        runCurrent()
        assertTrue(next.await().contentEquals(byteArrayOf(3)))
    }

    @Test
    fun closingTheRenderOwnerCancelsSharedResourceLoadsToo() = runTest {
        val requests = Channel<Unit>(Channel.CONFLATED)
        var started = false
        var cancelled = false
        val loop = backgroundScope.launch {
            runNexradRenderLoop(requests, { testScheduler.currentTime }, { testScheduler.currentTime }, { loads ->
                NexradRenderSchedule(null) {
                    loads.load("held") {
                        started = true
                        try { awaitCancellation() } finally { cancelled = true }
                    }
                }
            }, { throw AssertionError(it) })
        }
        requests.send(Unit)
        runCurrent()
        assertTrue(started)
        requests.close()
        runCurrent()
        assertTrue(loop.isCompleted)
        assertTrue(cancelled)
    }
}
