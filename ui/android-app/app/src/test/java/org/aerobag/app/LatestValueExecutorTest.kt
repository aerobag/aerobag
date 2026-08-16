// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app

import java.util.ArrayDeque
import java.util.concurrent.Executor
import org.junit.Assert.assertEquals
import org.junit.Test

class LatestValueExecutorTest {
    @Test
    fun slowConsumerReceivesOnlyNewestQueuedValue() {
        val executor = QueuedExecutor()
        val delivered = mutableListOf<Int>()
        val delivery = LatestValueExecutor(executor, delivered::add)

        delivery.submit(1)
        delivery.submit(2)
        delivery.submit(3)

        assertEquals(1, executor.size)
        executor.runNext()
        assertEquals(listOf(3), delivered)

        delivery.submit(4)
        executor.runNext()
        assertEquals(listOf(3, 4), delivered)
    }

    @Test
    fun closedDeliveryDropsQueuedValues() {
        val executor = QueuedExecutor()
        val delivered = mutableListOf<Int>()
        val delivery = LatestValueExecutor(executor, delivered::add)

        delivery.submit(1)
        delivery.close()
        executor.runNext()

        assertEquals(emptyList<Int>(), delivered)
    }

    @Test
    fun coalescerPreservesMetadataFromCollapsedValues() {
        val executor = QueuedExecutor()
        val delivered = mutableListOf<Set<String>>()
        val delivery = LatestValueExecutor(
            executor = executor,
            consume = delivered::add,
            coalesce = { previous, next -> previous + next },
        )

        delivery.submit(setOf("status"))
        delivery.submit(setOf("ownship"))
        executor.runNext()

        assertEquals(listOf(setOf("status", "ownship")), delivered)
    }

    private class QueuedExecutor : Executor {
        private val work = ArrayDeque<Runnable>()
        val size: Int get() = work.size

        override fun execute(command: Runnable) {
            work.addLast(command)
        }

        fun runNext() {
            work.removeFirst().run()
        }
    }
}
