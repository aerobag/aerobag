// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app

import org.junit.Assert.assertEquals
import org.junit.Test

class DisplayInactivityPolicyTest {
    @Test
    fun reportsTimeRemainingUntilTheLongIdleDeadline() {
        assertEquals(
            3_000L,
            remainingDisplayInactivityMs(
                nowElapsedMs = 12_000L,
                lastActivityElapsedMs = 5_000L,
                timeoutMs = 10_000L,
            ),
        )
    }

    @Test
    fun clampsElapsedDeadlinesAndClockRewinds() {
        assertEquals(
            0L,
            remainingDisplayInactivityMs(
                nowElapsedMs = 20_000L,
                lastActivityElapsedMs = 5_000L,
                timeoutMs = 10_000L,
            ),
        )
        assertEquals(
            10_000L,
            remainingDisplayInactivityMs(
                nowElapsedMs = 4_000L,
                lastActivityElapsedMs = 5_000L,
                timeoutMs = 10_000L,
            ),
        )
    }
}
