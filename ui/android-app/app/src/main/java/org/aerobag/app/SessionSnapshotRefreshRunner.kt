// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app

import android.os.SystemClock
import java.util.concurrent.Executor
import java.util.concurrent.ExecutorService
import java.util.concurrent.Executors
import java.util.concurrent.ScheduledExecutorService
import java.util.concurrent.ScheduledFuture
import java.util.concurrent.TimeUnit
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.decodeFromString
import kotlinx.serialization.json.Json
import org.aerobag.app.domain.NativeBindings
import org.aerobag.app.domain.NativeBridge

/** Executes refresh decisions made by core's shared snapshot scheduler. */
internal class SessionSnapshotRefreshRunner<T>(
    private val refresh: () -> T,
    private val resultExecutor: Executor,
    private val bridge: NativeBridge = NativeBindings,
    private val clockMs: () -> Long = SystemClock::elapsedRealtime,
    private val workerExecutor: ExecutorService = Executors.newSingleThreadExecutor { runnable ->
        Thread(runnable, "aerobag-session-snapshot").apply { isDaemon = true }
    },
    private val timerExecutor: ScheduledExecutorService =
        Executors.newSingleThreadScheduledExecutor { runnable ->
            Thread(runnable, "aerobag-session-snapshot-timer").apply { isDaemon = true }
        },
) : AutoCloseable {
    private val lock = Any()
    private val schedulerHandle = bridge.createSessionSnapshotRefreshScheduler()
    private var scheduledPoll: ScheduledFuture<*>? = null
    private var snapshotListener: ((T) -> Unit)? = null
    private var failureListener: ((Throwable) -> Unit)? = null
    private var closed = false

    fun setListeners(onSnapshot: ((T) -> Unit)?, onFailure: ((Throwable) -> Unit)?) {
        synchronized(lock) {
            snapshotListener = onSnapshot
            failureListener = onFailure
        }
    }

    fun request(priority: SessionSnapshotRefreshPriority, reason: String) {
        val decision = synchronized(lock) {
            if (closed) return
            decodeDecision(
                bridge.sessionSnapshotRefreshSchedulerRequestJson(
                    schedulerHandle,
                    clockMs(),
                    json.encodeToString(SessionSnapshotRefreshPriority.serializer(), priority),
                    reason,
                ),
            )
        }
        handleDecision(decision)
    }

    override fun close() {
        synchronized(lock) {
            if (closed) return
            closed = true
            scheduledPoll?.cancel(false)
            scheduledPoll = null
            snapshotListener = null
            failureListener = null
            bridge.destroySessionSnapshotRefreshScheduler(schedulerHandle)
        }
        timerExecutor.shutdownNow()
        workerExecutor.shutdownNow()
    }

    private fun handleDecision(decision: SessionSnapshotRefreshDecision) {
        when (decision.kind) {
            "idle" -> clearScheduledPoll()
            "schedule" -> schedulePoll(decision.delayMs)
            "start" -> startRefresh()
            else -> error("unknown session snapshot refresh decision: ${decision.kind}")
        }
    }

    private fun schedulePoll(delayMs: Long) {
        synchronized(lock) {
            if (closed) return
            scheduledPoll?.cancel(false)
            scheduledPoll = timerExecutor.schedule(
                {
                    val decision = synchronized(lock) {
                        if (closed) return@schedule
                        decodeDecision(
                            bridge.sessionSnapshotRefreshSchedulerPollJson(
                                schedulerHandle,
                                clockMs(),
                            ),
                        )
                    }
                    handleDecision(decision)
                },
                delayMs.coerceAtLeast(0),
                TimeUnit.MILLISECONDS,
            )
        }
    }

    private fun startRefresh() {
        clearScheduledPoll()
        synchronized(lock) {
            if (closed) return
            workerExecutor.execute {
                val outcome = runCatching(refresh)
                if (isClosed()) return@execute
                resultExecutor.execute {
                    val listeners = synchronized(lock) {
                        if (closed) null else snapshotListener to failureListener
                    }
                    listeners?.let { (onSnapshot, onFailure) ->
                        outcome.fold(
                            onSuccess = { snapshot -> onSnapshot?.invoke(snapshot) },
                            onFailure = { error -> onFailure?.invoke(error) },
                        )
                    }
                }
                val decision = synchronized(lock) {
                    if (closed) return@execute
                    decodeDecision(
                        bridge.sessionSnapshotRefreshSchedulerRefreshCompletedJson(
                            schedulerHandle,
                            clockMs(),
                        ),
                    )
                }
                handleDecision(decision)
            }
        }
    }

    private fun clearScheduledPoll() {
        synchronized(lock) {
            scheduledPoll?.cancel(false)
            scheduledPoll = null
        }
    }

    private fun isClosed(): Boolean = synchronized(lock) { closed }

    private fun decodeDecision(value: String): SessionSnapshotRefreshDecision =
        json.decodeFromString(value)

    private companion object {
        val json = Json { ignoreUnknownKeys = true }
    }
}

@Serializable
internal enum class SessionSnapshotRefreshPriority {
    @SerialName("timely")
    Timely,

    @SerialName("low_priority")
    LowPriority,
}

@Serializable
private data class SessionSnapshotRefreshDecision(
    val kind: String,
    @SerialName("delay_ms") val delayMs: Long = 0,
    val reason: String = "",
)
