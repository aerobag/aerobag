// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app

import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.CoroutineStart
import kotlinx.coroutines.Deferred
import kotlinx.coroutines.Job
import kotlinx.coroutines.async
import kotlinx.coroutines.cancel
import kotlinx.coroutines.launch

/** Owns startup work independently of the Compose caller waiting for it. */
internal class RetainedRuntime<T : Any>(
    private val scope: CoroutineScope,
    private val dispose: (T) -> Unit,
) {
    private val lock = Any()
    private var generation = 0L
    private var preparation: Deferred<Result<T>>? = null
    private var tail: Job? = null
    private var adoptionId: String? = null
    private var closed = false
    @Volatile var result: Result<T>? = null
        private set

    fun reset() {
        synchronized(lock) {
            check(!closed)
            adoptionId = null
            resetLocked()
        }
    }

    fun resetFor(adoptionId: String) {
        synchronized(lock) {
            check(!closed)
            if (this.adoptionId == adoptionId) return
            this.adoptionId = adoptionId
            resetLocked()
        }
    }

    private fun resetLocked() {
        generation += 1
        val current = result?.getOrNull()
        preparation = null
        result = null
        if (current != null) {
            val previous = tail
            tail = scope.launch {
                previous?.join()
                dispose(current)
            }
        }
        // Do not cancel the tail: native work cannot be interrupted, and canceling
        // a queued successor would let the next startup bypass its predecessor.
    }

    fun begin(load: suspend () -> T): Deferred<Result<T>> {
        val pending = synchronized(lock) {
            check(!closed)
            result?.let { return CompletableDeferred(it) }
            preparation?.let { return it }
            val startedGeneration = generation
            val previous = tail
            scope.async(start = CoroutineStart.LAZY) {
                previous?.join()
                synchronized(lock) {
                    if (generation != startedGeneration) throw CancellationException("Startup superseded")
                }
                val loaded = try {
                    Result.success(load())
                } catch (error: Throwable) {
                    if (error is CancellationException) throw error
                    Result.failure(error)
                }
                val published = synchronized(lock) {
                    if (generation != startedGeneration) false else {
                        result = loaded
                        preparation = null
                        true
                    }
                }
                if (!published) {
                    loaded.getOrNull()?.let(dispose)
                    throw CancellationException("Startup superseded")
                }
                loaded
            }.also {
                preparation = it
                tail = it
            }
        }
        pending.start()
        return pending
    }

    suspend fun await(load: suspend () -> T): Result<T> {
        return begin(load).await()
    }

    fun close() {
        val last = synchronized(lock) {
            if (closed) return
            closed = true
            resetLocked()
            tail
        }
        // Retire even a still-running native startup before canceling its scope.
        if (last == null) scope.cancel() else last.invokeOnCompletion { scope.cancel() }
    }
}
