// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app.domain

import java.util.concurrent.Executor
import org.junit.Assert.assertEquals
import org.junit.Test

class AsyncSessionResourceEffectPumpTest {
    @Test
    fun requestDefersResourceIoAndCoalescesPendingWakeups() {
        val executor = QueuedExecutor()
        var pumpCalls = 0
        val published = mutableListOf<List<String>>()
        val pump = AsyncSessionResourceEffectPump(
            executor = executor,
            pump = {
                pumpCalls += 1
                listOf("session_snapshot")
            },
            publishInvalidations = published::add,
        )

        pump.request()
        pump.request()

        assertEquals(0, pumpCalls)
        assertEquals(1, executor.tasks.size)

        executor.runNext()

        assertEquals(1, pumpCalls)
        assertEquals(listOf(listOf("session_snapshot")), published)
        pump.close()
    }

    private class QueuedExecutor : Executor {
        val tasks = ArrayDeque<Runnable>()

        override fun execute(command: Runnable) {
            tasks.addLast(command)
        }

        fun runNext() {
            tasks.removeFirst().run()
        }
    }
}
