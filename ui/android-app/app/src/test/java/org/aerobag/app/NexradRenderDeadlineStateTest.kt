// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class NexradRenderDeadlineStateTest {
    @Test
    fun successfulRenderSchedulesCoreOwnedDeadlineOnTheMonotonicClock() {
        val state = NexradRenderDeadlineState()

        state.renderCompleted(nowElapsedRealtimeMs = 10_000L, nowEpochMs = 100_000L, coreDeadlineEpochMs = 102_345L)

        assertEquals(12_345L, state.deadlineElapsedRealtimeMs)
        state.consumeWake()
        assertNull(state.deadlineElapsedRealtimeMs)
    }

    @Test
    fun expiredDeadlineQueriesImmediatelyAndHoldLatestRemovesTheTimer() {
        val state = NexradRenderDeadlineState()
        state.renderCompleted(10_000L, 100_000L, 99_000L)
        assertEquals(10_000L, state.deadlineElapsedRealtimeMs)
        state.renderCompleted(10_001L, 100_001L, null)
        assertNull(state.deadlineElapsedRealtimeMs)
    }

    @Test
    fun failedRenderSchedulesRecoveryInsteadOfOrphaningVisibleFrame() {
        val state = NexradRenderDeadlineState(failureRetryMs = 750L)

        state.renderFailed(nowElapsedRealtimeMs = 10_000L)

        assertEquals(10_750L, state.deadlineElapsedRealtimeMs)
    }
}
