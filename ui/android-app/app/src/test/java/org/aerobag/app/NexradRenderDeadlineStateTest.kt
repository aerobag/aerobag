// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class NexradRenderDeadlineStateTest {
    @Test
    fun successfulRenderPreservesCoreOwnedAnimationDeadline() {
        val state = NexradRenderDeadlineState()

        state.renderCompleted(12_345L)

        assertEquals(12_345L, state.deadlineEpochMs)
        state.consumeWake()
        assertNull(state.deadlineEpochMs)
    }

    @Test
    fun failedRenderSchedulesRecoveryInsteadOfOrphaningVisibleFrame() {
        val state = NexradRenderDeadlineState(failureRetryMs = 750L)

        state.renderFailed(nowEpochMs = 10_000L)

        assertEquals(10_750L, state.deadlineEpochMs)
    }
}
