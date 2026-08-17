// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app

import java.lang.reflect.Proxy
import java.util.concurrent.CountDownLatch
import java.util.concurrent.Executor
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicInteger
import org.aerobag.app.domain.NativeBridge
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class SessionSnapshotRefreshRunnerTest {
    @Test
    fun invalidationsDuringRefreshProduceOneLatestFollowup() {
        val refreshStarted = CountDownLatch(1)
        val releaseRefresh = CountDownLatch(1)
        val published = CountDownLatch(2)
        val refreshCount = AtomicInteger(0)
        val requestCount = AtomicInteger(0)
        val completionCount = AtomicInteger(0)
        val bridge = snapshotSchedulerBridge(
            request = {
                if (requestCount.incrementAndGet() == 1) {
                    """{"kind":"start","reason":"first"}"""
                } else {
                    """{"kind":"schedule","delay_ms":10000,"reason":"latest"}"""
                }
            },
            complete = {
                if (completionCount.incrementAndGet() == 1) {
                    """{"kind":"start","reason":"latest"}"""
                } else {
                    """{"kind":"idle"}"""
                }
            },
        )
        val runner = SessionSnapshotRefreshRunner(
            refresh = {
                val generation = refreshCount.incrementAndGet()
                if (generation == 1) {
                    refreshStarted.countDown()
                    assertTrue(releaseRefresh.await(2, TimeUnit.SECONDS))
                }
                generation
            },
            resultExecutor = Executor(Runnable::run),
            bridge = bridge,
            clockMs = { 1_000 },
        )
        runner.setListeners(
            onSnapshot = { published.countDown() },
            onFailure = { throw it },
        )

        runner.request(SessionSnapshotRefreshPriority.Timely, "first")
        assertTrue(refreshStarted.await(2, TimeUnit.SECONDS))
        repeat(20) {
            runner.request(SessionSnapshotRefreshPriority.LowPriority, "latest")
        }
        releaseRefresh.countDown()

        assertTrue(published.await(2, TimeUnit.SECONDS))
        assertEquals(2, refreshCount.get())
        runner.close()
    }

    @Test
    fun viewportGestureAndActivityUseTheSharedCoreScheduler() {
        val calls = mutableListOf<String>()
        val bridge = snapshotSchedulerBridge(
            request = { """{"kind":"idle"}""" },
            complete = { """{"kind":"idle"}""" },
            gesture = { active ->
                calls += "gesture:$active"
                """{"kind":"idle"}"""
            },
            activity = {
                calls += "activity"
                """{"kind":"idle"}"""
            },
        )
        val runner = SessionSnapshotRefreshRunner(
            refresh = { Unit },
            resultExecutor = Executor(Runnable::run),
            bridge = bridge,
            clockMs = { 1_000 },
        )

        runner.viewportGestureActiveChanged(true)
        runner.viewportActivity()
        runner.viewportGestureActiveChanged(false)

        assertEquals(listOf("gesture:true", "activity", "gesture:false"), calls)
        runner.close()
    }

    private fun snapshotSchedulerBridge(
        request: () -> String,
        complete: () -> String,
        gesture: (Boolean) -> String = { """{"kind":"idle"}""" },
        activity: () -> String = { """{"kind":"idle"}""" },
    ): NativeBridge = Proxy.newProxyInstance(
        NativeBridge::class.java.classLoader,
        arrayOf(NativeBridge::class.java),
    ) { _, method, arguments ->
        when (method.name) {
            "createSessionSnapshotRefreshScheduler" -> 1L
            "sessionSnapshotRefreshSchedulerRequestJson" -> request()
            "sessionSnapshotRefreshSchedulerRefreshCompletedJson" -> complete()
            "sessionSnapshotRefreshSchedulerViewportGestureActiveChangedJson" ->
                gesture(requireNotNull(arguments)[2] as Boolean)
            "sessionSnapshotRefreshSchedulerViewportActivityJson" -> activity()
            "sessionSnapshotRefreshSchedulerPollJson" -> """{"kind":"idle"}"""
            "destroySessionSnapshotRefreshScheduler" -> Unit
            "equals" -> false
            "hashCode" -> 0
            "toString" -> "SessionSnapshotRefreshRunnerTestBridge"
            else -> error("unexpected NativeBridge call: ${method.name}")
        }
    } as NativeBridge
}
