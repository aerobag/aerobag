// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app

import androidx.compose.foundation.layout.size
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performTouchInput
import androidx.compose.ui.unit.IntSize
import androidx.compose.ui.unit.dp
import org.aerobag.app.domain.MapViewportState
import org.aerobag.app.domain.dragViewport
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [34])
class MapFollowViewportSyncTest {
    @get:Rule val compose = createComposeRule()

    @Test fun retainedGestureCompletionUsesMeasuredSizeLatestFollowStateAndCommandSink() {
        val initial = MapViewportState(centerWorldX = 128.0, centerWorldY = 128.0, zoom = 10.0)
        val viewport = mutableStateOf(initial)
        val size = mutableStateOf(IntSize.Zero)
        val following = mutableStateOf(true)
        val generation = mutableStateOf(0)
        val calls = mutableListOf<Triple<Int, MapViewportState, IntSize>>()
        var preLayoutCallbackInstalled = false
        var released = false
        compose.setContent {
            val sinkGeneration = generation.value
            val sync = rememberMapFollowViewportSync(
                following.value, size.value.width.toFloat(), size.value.height.toFloat(),
            ) { next, width, height ->
                calls += Triple(sinkGeneration, next, IntSize(width.toInt(), height.toInt()))
            }
            // Retain the callback installed before layout, just as a long-lived
            // input owner does. Recomposition must update its inputs, not rely
            // on replacing the callback before the final pointer-up arrives.
            val complete = remember {
                preLayoutCallbackInstalled = size.value == IntSize.Zero
                { sync(viewport.value) }
            }
            MapSurfaceLayers(
                modifier = Modifier.size(400.dp).testTag("map")
                    .onSizeChanged { size.value = it }
                    .mapGestureInput(
                        enabled = true,
                        onTap = {}, onGesture = {},
                        onTransform = { before, after ->
                            viewport.value = dragViewport(viewport.value,
                                after[0].x - before[0].x, after[0].y - before[0].y)
                        },
                        onActiveChange = { active -> if (!active) released = true },
                        onFinished = complete,
                    ),
                mapContent = {}, controls = {},
            )
        }
        compose.onNodeWithTag("map").performTouchInput {
            down(Offset(100f, 100f)); moveBy(Offset(100f, 0f))
        }
        compose.runOnIdle {
            assertTrue(preLayoutCallbackInstalled)
            assertTrue(size.value.width > 0)
            assertTrue(calls.isEmpty())
            generation.value = 1
        }
        compose.onNodeWithTag("map").performTouchInput { up() }
        compose.runOnIdle {
            assertTrue(released)
            assertTrue(viewport.value != initial)
            assertEquals(listOf(Triple(1, viewport.value, size.value)), calls)
            following.value = false
        }
        compose.onNodeWithTag("map").performTouchInput {
            down(Offset(100f, 100f)); moveBy(Offset(100f, 0f)); up()
        }
        compose.runOnIdle { assertEquals("CTR off must not synchronize an offset", 1, calls.size) }
    }
}
