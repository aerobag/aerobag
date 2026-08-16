// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app.domain

import java.util.concurrent.Callable
import java.util.concurrent.Executors
import java.util.concurrent.ThreadFactory
import java.util.concurrent.atomic.AtomicInteger

internal data class ResourceFrontierLoadOutcome(
    val bytes: ByteArray?,
    val error: Throwable?,
    val elapsedUs: Long,
)

internal data class ResourceFrontierLoadBatch(
    val outcomes: List<ResourceFrontierLoadOutcome>,
    val wallUs: Long,
    val maxConcurrency: Int,
)

internal class ResourceFrontierLoader(
    maxParallelism: Int = DefaultMaxParallelism,
) : AutoCloseable {
    private val threadSequence = AtomicInteger(1)
    private val executor = Executors.newFixedThreadPool(
        maxParallelism.coerceAtLeast(1),
        ThreadFactory { runnable ->
            Thread(runnable, "AerobagResource-${threadSequence.getAndIncrement()}").apply {
                isDaemon = true
            }
        },
    )

    fun load(loads: List<() -> ByteArray>): ResourceFrontierLoadBatch {
        if (loads.isEmpty()) {
            return ResourceFrontierLoadBatch(emptyList(), wallUs = 0L, maxConcurrency = 0)
        }
        val active = AtomicInteger(0)
        val peak = AtomicInteger(0)
        val batchStartedNs = System.nanoTime()
        val futures = loads.map { load ->
            executor.submit(Callable {
                val concurrency = active.incrementAndGet()
                peak.updateAndGet { previous -> maxOf(previous, concurrency) }
                val startedNs = System.nanoTime()
                try {
                    val bytes = load()
                    ResourceFrontierLoadOutcome(
                        bytes = bytes,
                        error = null,
                        elapsedUs = elapsedUs(startedNs),
                    )
                } catch (error: Throwable) {
                    ResourceFrontierLoadOutcome(
                        bytes = null,
                        error = error,
                        elapsedUs = elapsedUs(startedNs),
                    )
                } finally {
                    active.decrementAndGet()
                }
            })
        }
        val outcomes = futures.map { it.get() }
        return ResourceFrontierLoadBatch(
            outcomes = outcomes,
            wallUs = elapsedUs(batchStartedNs),
            maxConcurrency = peak.get(),
        )
    }

    override fun close() {
        executor.shutdownNow()
    }

    private fun elapsedUs(startedNs: Long): Long =
        ((System.nanoTime() - startedNs).coerceAtLeast(0L) + 500L) / 1_000L

    companion object {
        private const val DefaultMaxParallelism = 16
    }
}
