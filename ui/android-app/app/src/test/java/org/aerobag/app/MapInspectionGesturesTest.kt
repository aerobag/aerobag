// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.runtime.mutableStateOf
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.test.click
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performTouchInput
import androidx.compose.ui.test.swipe
import androidx.compose.ui.unit.dp
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [34])
class MapInspectionGesturesTest {
    @get:Rule val compose = createComposeRule()
    private val open = mutableStateOf(true)
    private var gestures = 0
    private var transforms = 0
    private var taps = 0
    private var buttonClicks = 0
    private var scrolled = 0
    private val touching = mutableListOf<Boolean>()
    private val gestureLifecycle = mutableListOf<String>()

    private fun render() {
        compose.setContent {
            MapSurfaceLayers(
                modifier = Modifier.size(400.dp).testTag("map").mapGestureInput(
                    enabled = true,
                    onTap = { if (open.value) open.value = false else taps++ },
                    onGesture = { gestures++; open.value = false },
                    onTransform = { _, _ -> transforms++ },
                    onActiveChange = { gestureLifecycle += "active:$it" },
                    onFinished = { gestureLifecycle += "finished" },
                ),
                mapContent = {},
                controls = {
                    Box(Modifier.fillMaxSize().clickable { buttonClicks++ })
                    if (open.value) MapInspectionOverlay(false, { open.value = false }) {
                        val scroll = rememberScrollState()
                        scrolled = scroll.value
                        Column(Modifier.align(Alignment.TopEnd).size(150.dp).testTag("tray")
                            .inspectorInputBoundary { touching += it }
                            .verticalScroll(scroll)) {
                            Box(Modifier.size(100.dp, 40.dp).testTag("action").clickable { buttonClicks++ })
                            Box(Modifier.height(600.dp))
                        }
                    }
                },
            )
        }
    }

    @Test fun dragOnBackdropDismissesAndContinuesAfterOverlayDisappears() {
        render()
        compose.onNodeWithTag("map").performTouchInput {
            down(Offset(width * .2f, height * .8f))
            moveBy(Offset(0f, -80f))
        }
        compose.runOnIdle { assertFalse(open.value); assertTrue(transforms > 0) }
        val firstTransforms = transforms
        compose.onNodeWithTag("map").performTouchInput { moveBy(Offset(0f, -80f)); up() }
        compose.runOnIdle {
            assertTrue(transforms > firstTransforms)
            assertEquals(1, gestures)
            assertEquals(listOf("active:true", "finished", "active:false"), gestureLifecycle)
            assertEquals(0, taps)
            assertEquals(0, buttonClicks)
        }
    }

    @Test fun pinchOnBackdropDismissesAndDoesNotReopenAtRelease() {
        render()
        compose.onNodeWithTag("map").performTouchInput {
            down(0, Offset(width * .2f, height * .6f))
            down(1, Offset(width * .2f, height * .9f))
            moveTo(0, Offset(width * .2f, height * .4f))
            moveTo(1, Offset(width * .2f, height * .95f))
            up(0); up(1)
        }
        compose.runOnIdle {
            assertFalse(open.value)
            assertTrue(transforms > 0)
            assertEquals(0, taps)
            assertEquals(0, buttonClicks)
        }
    }

    @Test fun trayScrollAndButtonsNeverReachMapAndReportActivity() {
        render()
        compose.onNodeWithTag("action").performTouchInput { click() }
        compose.runOnIdle { assertEquals(1, buttonClicks) }
        compose.onNodeWithTag("tray").performTouchInput {
            swipe(Offset(width / 2f, height * .9f), Offset(width / 2f, height * .2f))
        }
        compose.runOnIdle {
            assertTrue(open.value)
            assertTrue(scrolled > 0)
            assertEquals(0, transforms)
            assertEquals(0, gestures)
            assertEquals(listOf(true, false, true, false), touching)
        }
    }

    @Test fun backdropTapDismissesWithoutActivatingUnderlyingButton() {
        render()
        compose.onNodeWithTag("map").performTouchInput { click(Offset(width * .2f, height * .8f)) }
        compose.runOnIdle {
            assertFalse(open.value)
            assertEquals(0, buttonClicks)
            assertEquals(0, transforms)
            assertEquals(0, taps)
        }
    }
}
