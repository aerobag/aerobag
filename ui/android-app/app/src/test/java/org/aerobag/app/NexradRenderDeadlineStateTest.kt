// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class NexradRenderDeadlineStateTest {
    @Test
    fun successfulRenderSchedulesCoreOwnedDelayOnTheMonotonicClock() {
        val state = NexradRenderDeadlineState()

        state.renderCompleted(nowElapsedRealtimeMs = 10_000L, coreDelayMs = 2_345L)

        assertEquals(12_345L, state.deadlineElapsedRealtimeMs)
        state.consumeWake()
        assertNull(state.deadlineElapsedRealtimeMs)
    }

    @Test
    fun failedRenderSchedulesRecoveryInsteadOfOrphaningVisibleFrame() {
        val state = NexradRenderDeadlineState(failureRetryMs = 750L)

        state.renderFailed(nowElapsedRealtimeMs = 10_000L)

        assertEquals(10_750L, state.deadlineElapsedRealtimeMs)
    }
}
