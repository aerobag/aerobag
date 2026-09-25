// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app

import android.view.View
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.mutableStateOf
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.test.click
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performTouchInput
import androidx.compose.ui.unit.dp
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [34], qualifiers = "w400dp-h800dp")
class PlaybackInputTest {
    @get:Rule val compose = createComposeRule()

    @Test fun timelinePublishesTheCurrentTraceRangeAndPhysicalScrubsReachThatRange() {
        val duration = mutableStateOf(10.0)
        var cursor = 0.0
        var finished = false
        lateinit var window: View
        compose.setContent {
            window = LocalView.current
            PlaybackOverview(org.aerobag.app.domain.PlaybackUiState(), cursor, duration.value,
                "parity:timeline-test") { value, done -> cursor = value; finished = done }
        }
        for (seconds in listOf(10.0, 100.0)) {
            compose.runOnIdle { duration.value = seconds }
            compose.runOnIdle {
                window.drawObservationFrame()
                assertTrue(E2eProjectionRegistry.read("parity:timeline-test")!!.state.contains(":max:${seconds.toFloat()}:"))
            }
            compose.onNodeWithTag("parity:timeline-test").performTouchInput { click(Offset(width * 0.8f, height / 2f)) }
            compose.runOnIdle {
                assertEquals(seconds * 0.8, cursor, 0.01)
                assertTrue(finished)
            }
        }
    }

    @Test fun publishedRateMappingMatchesPhysicalInputAndSurvivesRemount() {
        val mounted = mutableStateOf(true)
        val enabled = mutableStateOf(true)
        val rate = mutableStateOf(1f)
        val tag = "parity:rate-test"
        lateinit var window: View
        compose.setContent {
            window = LocalView.current
            if (mounted.value) PlaybackRateRail(rate.value, enabled.value,
                modifier = Modifier.size(300.dp, 40.dp), testTag = tag) { rate.value = it }
        }
        repeat(2) {
            for (desired in listOf(0.25f, 2f, 5f, 11f)) {
                compose.runOnIdle {
                    window.drawObservationFrame()
                    val fields = E2eProjectionRegistry.read(tag)!!.state.split(':').chunked(2).associate { it[0] to it[1] }
                    assertEquals("horizontal-progress", fields["kind"])
                    assertEquals("0.25", fields["min"])
                    assertEquals("11.0", fields["max"])
                }
                compose.onNodeWithTag(tag).performTouchInput {
                    val x = (width * (desired - 0.25f) / 10.75f).coerceIn(0.5f, width - 0.5f)
                    click(Offset(x, height / 2f))
                }
                compose.runOnIdle { assertEquals(desired, rate.value, 0.001f) }
            }
            compose.runOnIdle { mounted.value = false }
            compose.runOnIdle { assertNull(E2eProjectionRegistry.read(tag)); mounted.value = true }
        }
        compose.runOnIdle { enabled.value = false }
        compose.onNodeWithTag(tag).performTouchInput { click() }
        compose.runOnIdle { assertEquals(11f, rate.value, 0.001f) }
    }
}
