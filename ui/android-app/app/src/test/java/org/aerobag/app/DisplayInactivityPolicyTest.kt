// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
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

    @Test
    fun idleBrightnessOverrideOnlyDimsABrighterDisplay() {
        assertEquals(
            0.05f,
            idleDisplayBrightnessOverride(
                currentBrightness = 0.30f,
                configuredIdleBrightness = 0.05f,
            ),
        )
        assertNull(
            idleDisplayBrightnessOverride(
                currentBrightness = 0.02f,
                configuredIdleBrightness = 0.05f,
            ),
        )
        assertNull(
            idleDisplayBrightnessOverride(
                currentBrightness = null,
                configuredIdleBrightness = 0.05f,
            ),
        )
    }

    @Test
    fun idleBrightnessHandlesDaylightEqualityAndInvalidReadings() {
        assertEquals(0.05f, idleDisplayBrightnessOverride(1.0f, 0.05f))
        assertNull(idleDisplayBrightnessOverride(0.05f, 0.05f))
        for (brightness in listOf(-1.0f, Float.NaN, Float.POSITIVE_INFINITY)) {
            assertNull(idleDisplayBrightnessOverride(brightness, 0.05f))
        }
        assertNull(idleDisplayBrightnessOverride(0.8f, Float.NaN))
    }
}
