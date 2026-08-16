// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app

import java.util.concurrent.Executor

/** Delivers only the newest pending value when a slower executor catches up. */
internal class LatestValueExecutor<T>(
    private val executor: Executor,
    private val consume: (T) -> Unit,
    private val coalesce: (previous: T, next: T) -> T = { _, next -> next },
) : AutoCloseable {
    private val lock = Any()
    private var pending: T? = null
    private var scheduled = false
    private var closed = false

    fun submit(value: T) {
        val shouldSchedule = synchronized(lock) {
            if (closed) return
            pending = pending?.let { previous -> coalesce(previous, value) } ?: value
            if (scheduled) false else {
                scheduled = true
                true
            }
        }
        if (shouldSchedule) executor.execute(::deliver)
    }

    override fun close() {
        synchronized(lock) {
            closed = true
            pending = null
        }
    }

    private fun deliver() {
        val value = synchronized(lock) {
            if (closed) {
                scheduled = false
                return
            }
            pending.also { pending = null }
        }
        if (value != null) consume(value)
        val shouldSchedule = synchronized(lock) {
            if (closed || pending == null) {
                scheduled = false
                false
            } else {
                true
            }
        }
        if (shouldSchedule) executor.execute(::deliver)
    }
}
