// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app.domain

import android.util.Log
import java.util.concurrent.Executor
import java.util.concurrent.ExecutorService
import java.util.concurrent.atomic.AtomicBoolean

internal class AsyncSessionResourceEffectPump(
    private val executor: Executor,
    private val pump: () -> List<String>,
    private val publishInvalidations: (List<String>) -> Unit,
) : AutoCloseable {
    private val requested = AtomicBoolean(false)
    private val running = AtomicBoolean(false)
    private val closed = AtomicBoolean(false)

    fun request() {
        if (closed.get()) return
        requested.set(true)
        if (!running.compareAndSet(false, true)) return
        executor.execute(::run)
    }

    private fun run() {
        try {
            while (!closed.get() && requested.getAndSet(false)) {
                try {
                    publishInvalidations(pump())
                } catch (error: Throwable) {
                    Log.w("AerobagSessionEffect", "background session resource effect failed", error)
                }
            }
        } finally {
            running.set(false)
            if (!closed.get() && requested.get()) {
                request()
            }
        }
    }

    override fun close() {
        closed.set(true)
        (executor as? ExecutorService)?.shutdownNow()
    }
}
