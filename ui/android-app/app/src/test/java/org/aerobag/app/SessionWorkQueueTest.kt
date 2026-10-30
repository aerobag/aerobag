// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app

import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.TestCoroutineScheduler
import org.aerobag.app.generated.*
import org.junit.Assert.*
import org.junit.Test

class SessionWorkQueueTest {
    private class Scheduler : SessionWorkScheduler {
        var running = false
        val pending = linkedMapOf<String, UiSessionWorkRequest>()
        var closes = 0
        override fun request(request: UiSessionWorkRequest): UiSessionWorkRequestDecision {
            check(closes == 0)
            if (!running) { running = true; return UiSessionWorkRequestDecision.Start(request) }
            val old = pending.put(request.coalesceKey!!, request)
            return UiSessionWorkRequestDecision.Queued(replacedRequestId = old?.id)
        }
        override fun complete(id: Long): UiSessionWorkCompletionDecision {
            check(closes == 0)
            val next = pending.values.firstOrNull()
            next?.let { pending.remove(it.coalesceKey) }
            running = next != null
            return UiSessionWorkCompletionDecision(resultAction = UiSessionWorkResultAction.Land, next = next)
        }
        override fun close() { closes++ }
    }

    private class Lab {
        val work = TestCoroutineScheduler()
        val ui = TestCoroutineScheduler()
        val scheduler = Scheduler()
        val executed = mutableListOf<String>()
        val landed = mutableListOf<String>()
        val dropped = mutableListOf<String>()
        val queue = SessionWorkQueue<String>(scheduler,
            StandardTestDispatcher(work), StandardTestDispatcher(work), StandardTestDispatcher(ui), { work.currentTime })
        fun request(name: String, key: String = name, fail: Boolean = false) = queue.request(SessionWorkTask(
            UiSessionWorkKind.MapOverlay, key,
            run = { executed += name; if (fail) error(name); name },
            land = { landed += it.result.fold({ value -> value }, { error -> "failed:${error.message}" }) },
            dropped = { _, reason -> dropped += "$name:$reason" },
        ))
        fun drain() { repeat(4) { work.runCurrent(); ui.runCurrent() } }
        fun close() { queue.close(); drain() }
    }

    @Test
    fun nextWorkStartsWhileUiDeliveryIsHeldAndResultsLandInOrder() {
        val lab = Lab()
        try {
            lab.request("vectors"); lab.request("radar")
            lab.work.runCurrent()
            assertEquals(listOf("vectors", "radar"), lab.executed)
            assertTrue(lab.landed.isEmpty())
            lab.drain()
            assertEquals(listOf("vectors", "radar"), lab.landed)
        } finally { lab.close() }
    }

    @Test
    fun pendingCoalescingStillExecutesOnlyNewestAndLandsCompletedProgress() {
        val lab = Lab()
        try {
            lab.request("first", "map"); lab.request("obsolete", "map"); lab.request("latest", "map")
            lab.drain()
            assertEquals(listOf("first", "latest"), lab.executed)
            assertEquals(listOf("first", "latest"), lab.landed)
            assertEquals(listOf("obsolete:replaced_by_newer_pending"), lab.dropped)
        } finally { lab.close() }
    }

    @Test
    fun failureReleasesWorkerBeforeUiCanHandleTheError() {
        val lab = Lab()
        try {
            lab.request("failed", fail = true); lab.request("next")
            lab.work.runCurrent()
            assertEquals(listOf("failed", "next"), lab.executed)
            lab.drain()
            assertEquals(listOf("failed:failed", "next"), lab.landed)
        } finally { lab.close() }
    }

    @Test
    fun cancellationAfterComputationButBeforeDeliveryCannotLand() {
        val lab = Lab()
        try {
            val id = lab.request("cancelled")
            lab.work.runCurrent()
            lab.queue.cancel(id)
            lab.work.runCurrent()
            lab.drain()
            assertTrue(lab.landed.isEmpty())
            assertEquals(listOf("cancelled:caller_cancelled"), lab.dropped)
        } finally { lab.close() }
    }

    @Test
    fun retirementDropsComputedUndeliveredResultsExactlyOnce() {
        val lab = Lab()
        lab.request("first"); lab.request("second")
        lab.work.runCurrent()
        lab.close(); lab.close()
        assertTrue(lab.landed.isEmpty())
        assertEquals(setOf("first:runner_closed", "second:runner_closed"), lab.dropped.toSet())
        assertEquals(2, lab.dropped.size)
        assertEquals(1, lab.scheduler.closes)
    }

    @Test
    fun cancelledPendingRequestDoesNotExecuteAndDoesNotStrandSuccessor() {
        val lab = Lab()
        try {
            lab.request("first")
            val cancelled = lab.request("cancelled")
            lab.request("last")
            lab.queue.cancel(cancelled)
            lab.drain()
            assertEquals(listOf("first", "last"), lab.executed)
            assertEquals(listOf("first", "last"), lab.landed)
            assertEquals(listOf("cancelled:caller_cancelled"), lab.dropped)
        } finally { lab.close() }
    }

    @Test
    fun retirementBeforeWorkerDispatchPreventsNativeEntryAndRejectsNewRequests() {
        val controller = TestCoroutineScheduler()
        val worker = TestCoroutineScheduler()
        val ui = TestCoroutineScheduler()
        val scheduler = Scheduler()
        val dropped = mutableListOf<String>()
        val queue = SessionWorkQueue<String>(scheduler, StandardTestDispatcher(controller),
            StandardTestDispatcher(worker), StandardTestDispatcher(ui), { 0L })
        val task = SessionWorkTask(UiSessionWorkKind.MapOverlay, "map",
            run = { error("retired native session must not be entered") },
            land = { _: SessionWorkCompletion<String> -> fail("retired result landed") },
            dropped = { _, reason -> dropped += reason })
        queue.request(task)
        controller.runCurrent()
        queue.close()
        controller.runCurrent()
        queue.request(task)
        worker.runCurrent(); controller.runCurrent(); ui.runCurrent()
        assertEquals(listOf("runner_closed", "runner_closed"), dropped)
        assertEquals(1, scheduler.closes)
    }
}
