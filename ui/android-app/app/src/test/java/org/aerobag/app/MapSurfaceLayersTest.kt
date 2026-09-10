// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app

import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxScope
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.test.click
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performTouchInput
import androidx.compose.ui.unit.dp
import androidx.compose.ui.zIndex
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [34])
class MapSurfaceLayersTest {
    @get:Rule val compose = createComposeRule()

    // Mirrors the route editor's important input boundary: a full-screen,
    // high-z pointer handler that declines touches outside the route. No JNI,
    // nav database, network, browser, emulator, or mocked navigation is involved.
    @Composable
    private fun Editor(onDown: () -> Unit) {
        Box(Modifier.fillMaxSize().zIndex(1000f).testTag("editor").pointerInput(Unit) {
            awaitEachGesture {
                awaitFirstDown(requireUnconsumed = false)
                onDown()
            }
        })
    }

    @Composable
    private fun BoxScope.Controls(onClick: () -> Unit) {
        Box(Modifier.align(Alignment.BottomStart).size(48.dp).testTag("home").clickable(onClick = onClick))
        Box(Modifier.align(Alignment.TopEnd).size(48.dp).testTag("ctr").clickable(onClick = onClick))
    }

    @Test
    fun flatSiblingsReproduceTheNonConsumingEditorStealingNavigation() {
        var clicks = 0
        var editorDowns = 0
        compose.setContent {
            Box(Modifier.size(300.dp)) {
                Editor { editorDowns++ }
                Controls { clicks++ }
            }
        }
        // performClick would invoke semantics directly and hide this defect.
        compose.onNodeWithTag("home").performTouchInput { click() }
        compose.runOnIdle {
            assertEquals(0, clicks)
            assertEquals(1, editorDowns)
        }
    }

    @Test
    fun portraitControlsRemainTouchableAboveAnyLocalEditorZIndex() = assertIsolatedLayers(300, 500)

    @Test
    fun landscapeControlsRemainTouchableAboveAnyLocalEditorZIndex() = assertIsolatedLayers(500, 300)

    private fun assertIsolatedLayers(width: Int, height: Int) {
        var clicks = 0
        var editorDowns = 0
        compose.setContent {
            MapSurfaceLayers(
                modifier = Modifier.size(width.dp, height.dp),
                mapContent = { Editor { editorDowns++ } },
                controls = { Controls { clicks++ } },
            )
        }
        for (tag in listOf("home", "ctr")) {
            compose.onNodeWithTag(tag).performTouchInput { click() }
        }
        compose.runOnIdle {
            assertEquals(2, clicks)
            assertEquals(0, editorDowns)
        }
        // The controls' containing plane must not turn empty map space into a
        // full-screen input scrim. Use the rendered editor's center, not pixels.
        compose.onNodeWithTag("editor").performTouchInput { click() }
        compose.runOnIdle {
            assertEquals(2, clicks)
            assertEquals(1, editorDowns)
        }
    }
}
