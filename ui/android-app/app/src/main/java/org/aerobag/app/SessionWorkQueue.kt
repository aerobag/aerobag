// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app

import android.os.SystemClock
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.atomic.AtomicLong
import kotlinx.coroutines.*
import kotlinx.serialization.decodeFromString
import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.Json
import org.aerobag.app.domain.NativeBridge
import org.aerobag.app.generated.*

internal interface SessionWorkScheduler : AutoCloseable {
    fun request(request: UiSessionWorkRequest): UiSessionWorkRequestDecision
    fun complete(id: Long): UiSessionWorkCompletionDecision
}

internal class NativeSessionWorkScheduler(private val bridge: NativeBridge) : SessionWorkScheduler {
    private val handle = bridge.createUiSessionWorkScheduler()
    override fun request(request: UiSessionWorkRequest): UiSessionWorkRequestDecision =
        Json.decodeFromString(bridge.uiSessionWorkSchedulerRequestJson(handle, Json.encodeToString(request)))
    override fun complete(id: Long): UiSessionWorkCompletionDecision =
        Json.decodeFromString(bridge.uiSessionWorkSchedulerCompleteJson(handle, id))
    override fun close() = bridge.destroyUiSessionWorkScheduler(handle)
}

internal data class SessionWorkCompletion<T>(
    val request: UiSessionWorkRequest,
    val result: Result<T>,
    val action: UiSessionWorkResultAction,
    val startedAtMs: Long,
    val workerStartedAtMs: Long,
    val workerFinishedAtMs: Long,
    val schedulerMs: Long,
)

internal class SessionWorkTask<T>(
    val kind: UiSessionWorkKind,
    val key: String?,
    val run: () -> T,
    val land: (SessionWorkCompletion<T>) -> Unit,
    val dropped: (UiSessionWorkRequest, String) -> Unit,
)

// Core owns scheduling policy. This owner only serializes scheduler access and
// keeps UI delivery separate from synchronous work and its resource lifetime.
internal class SessionWorkQueue<T>(
    private val scheduler: SessionWorkScheduler,
    controllerDispatcher: CoroutineDispatcher = Dispatchers.IO.limitedParallelism(1),
    private val workerDispatcher: CoroutineDispatcher = Dispatchers.IO,
    private val deliveryDispatcher: CoroutineDispatcher = Dispatchers.Main,
    private val now: () -> Long = SystemClock::elapsedRealtime,
    private val event: (String, UiSessionWorkRequest, Map<String, Any>) -> Unit = { _, _, _ -> },
) : AutoCloseable {
    private val controller = CoroutineScope(SupervisorJob() + controllerDispatcher)
    private val delivery = CoroutineScope(SupervisorJob() + deliveryDispatcher)
    private val closed = AtomicBoolean()
    private val nextId = AtomicLong(1)
    private class Entry<T>(val request: UiSessionWorkRequest, val task: SessionWorkTask<T>) {
        val delivered = AtomicBoolean()
    }
    private val tasks = mutableMapOf<Long, Entry<T>>()
    private val active = mutableSetOf<Long>()

    fun request(task: SessionWorkTask<T>): Long {
        val request = UiSessionWorkRequest(id = nextId.getAndIncrement(), kind = task.kind,
            coalesceKey = task.key, requestedAtMs = now())
        if (closed.get()) {
            delivery.launch { task.dropped(request, "runner_closed") }
            return request.id
        }
        val accepted = AtomicBoolean()
        controller.launch {
            accepted.set(true)
            if (closed.get()) {
                delivery.launch { task.dropped(request, "runner_closed") }
                return@launch
            }
            tasks[request.id] = Entry(request, task)
            event("requested", request, mapOf("retained_count" to tasks.size))
            when (val decision = scheduler.request(request)) {
                is UiSessionWorkRequestDecision.Start -> start(decision.request)
                is UiSessionWorkRequestDecision.Queued -> {
                    event("queued", request, mapOf("replaced_request_id" to (decision.replacedRequestId ?: 0L)))
                    decision.replacedRequestId?.let { drop(it, "replaced_by_newer_pending") }
                }
            }
        }.invokeOnCompletion { failure ->
            if (failure != null && accepted.compareAndSet(false, true)) {
                delivery.launch { task.dropped(request, "runner_closed") }
            }
        }
        return request.id
    }

    fun cancel(id: Long) {
        controller.launch { drop(id, "caller_cancelled") }
    }

    private fun drop(id: Long, reason: String) {
        tasks.remove(id)?.let { entry ->
            if (entry.delivered.compareAndSet(false, true)) {
                delivery.launch { entry.task.dropped(entry.request, reason) }
            }
        }
    }

    private fun start(request: UiSessionWorkRequest) {
        if (closed.get()) return
        val entry = tasks[request.id]
        if (entry == null) {
            scheduler.complete(request.id).next?.let(::start)
            return
        }
        val started = now()
        active += request.id
        event("started", request, mapOf("queue_ms" to (started - request.requestedAtMs), "active_count" to active.size))
        controller.launch {
            var workerStarted = started
            var workerFinished = started
            val result = runCatching {
                withContext(workerDispatcher) {
                    workerStarted = now()
                    check(!closed.get()) { "session work runner is closed" }
                    try { entry.task.run() } finally { workerFinished = now() }
                }
            }
            if (closed.get()) return@launch
            val schedulerStarted = now()
            val completed = scheduler.complete(request.id)
            val schedulerMs = now() - schedulerStarted
            active -= request.id
            if (tasks[request.id] === entry) {
                // Queue delivery before dispatching the successor. Completed
                // progress lands FIFO, but a busy UI cannot retain the worker slot.
                delivery.launch {
                    if (!closed.get() && entry.delivered.compareAndSet(false, true)) {
                        try {
                            entry.task.land(SessionWorkCompletion(request, result, completed.resultAction,
                                started, workerStarted, workerFinished, schedulerMs))
                        } finally {
                            controller.launch { tasks.remove(request.id) }
                        }
                    }
                }
            }
            completed.next?.let(::start)
        }
    }

    override fun close() {
        if (!closed.compareAndSet(false, true)) return
        controller.launch {
            val remaining = tasks.values.toList()
            tasks.clear()
            active.clear()
            scheduler.close()
            delivery.launch {
                remaining.forEach { entry ->
                    if (entry.delivered.compareAndSet(false, true)) entry.task.dropped(entry.request, "runner_closed")
                }
            }
            controller.cancel()
        }
    }
}
