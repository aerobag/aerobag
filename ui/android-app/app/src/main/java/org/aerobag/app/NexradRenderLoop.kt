// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app

import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.CoroutineStart
import kotlinx.coroutines.Deferred
import kotlinx.coroutines.async
import kotlinx.coroutines.cancelChildren
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.channels.ReceiveChannel
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withTimeoutOrNull

internal const val NexradRenderLogTag = "AerobagNexrad"

internal class NexradRenderDeadlineState(private val failureRetryMs: Long = 1_000L) {
    var deadlineElapsedRealtimeMs: Long? = null
        private set

    fun consumeWake() {
        deadlineElapsedRealtimeMs = null
    }

    fun renderCompleted(nowElapsedRealtimeMs: Long, nowEpochMs: Long, coreDeadlineEpochMs: Long?) {
        deadlineElapsedRealtimeMs = coreDeadlineEpochMs?.let {
            nowElapsedRealtimeMs + (it - nowEpochMs).coerceAtLeast(0L)
        }
    }

    fun renderFailed(nowElapsedRealtimeMs: Long) {
        deadlineElapsedRealtimeMs = nowElapsedRealtimeMs + failureRetryMs
    }
}

internal data class NexradRenderSchedule(
    val nextUpdateEpochMs: Long?,
    val prefetch: suspend () -> Unit = {},
)

internal class NexradTileLoads(private val scope: CoroutineScope) {
    private val mutex = Mutex()
    private val pending = mutableMapOf<String, Deferred<Result<ByteArray>>>()

    suspend fun load(src: String, fetch: suspend () -> ByteArray): ByteArray {
        // Both render and prefetch can need the same image. Submitting separate
        // pending core requests with the same coalescing key cancels one caller.
        val result = mutex.withLock {
            pending.getOrPut(src) { scope.async(start = CoroutineStart.LAZY) { runCatching { fetch() } } }
        }
        return try {
            result.await().getOrThrow()
        } finally {
            mutex.withLock {
                if (result.isCompleted && pending[src] === result) pending.remove(src)
            }
        }
    }
}

internal suspend fun runNexradRenderLoop(
    requests: ReceiveChannel<Unit>,
    elapsedRealtimeMs: () -> Long,
    epochMs: () -> Long,
    render: suspend (NexradTileLoads) -> NexradRenderSchedule?,
    reportFailure: (Throwable) -> Unit,
) = coroutineScope {
    // Prefetch has one in-flight batch and one replaceable pending batch. It
    // must not prevent an animation deadline or mode change from querying core.
    val prefetchRequests = Channel<suspend () -> Unit>(Channel.CONFLATED)
    val tileLoads = NexradTileLoads(this)
    launch {
        for (prefetch in prefetchRequests) {
            try {
                prefetch()
            } catch (error: CancellationException) {
                throw error
            } catch (error: Throwable) {
                reportFailure(error)
            }
        }
    }
    val deadlineState = NexradRenderDeadlineState()
    try {
        while (true) {
            val deadline = deadlineState.deadlineElapsedRealtimeMs
            val request = if (deadline == null) {
                requests.receiveCatching()
            } else {
                val delayMs = (deadline - elapsedRealtimeMs()).coerceAtLeast(0)
                if (delayMs == 0L) null else withTimeoutOrNull(delayMs) { requests.receiveCatching() }
            }
            if (request?.isClosed == true) break
            deadlineState.consumeWake()
            try {
                val schedule = render(tileLoads)
                deadlineState.renderCompleted(elapsedRealtimeMs(), epochMs(), schedule?.nextUpdateEpochMs)
                prefetchRequests.trySend(schedule?.prefetch ?: {})
            } catch (error: CancellationException) {
                throw error
            } catch (error: Throwable) {
                reportFailure(error)
                deadlineState.renderFailed(elapsedRealtimeMs())
            }
        }
    } finally {
        prefetchRequests.close()
        coroutineContext.cancelChildren()
    }
}
