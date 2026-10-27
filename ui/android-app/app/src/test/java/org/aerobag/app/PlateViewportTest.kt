// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.SideEffect
import androidx.compose.runtime.mutableStateOf
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.test.click
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performTouchInput
import androidx.compose.ui.unit.IntSize
import androidx.compose.ui.unit.dp
import org.aerobag.app.domain.ImageViewportState
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [34])
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class PlateViewportTest {
    @get:Rule val compose = createComposeRule()

    @Test fun selectingTheLoadedFolderTileBeforeTheNextFrameCannotLeaveThePlateBlank() {
        val bitmap = ImageBitmap(200, 300)
        val saved = mutableStateOf<ImageViewportState?>(null)
        val folder = mutableStateOf(true)
        var rendered: ImageViewportState? = null
        var pendingInitialization: ImageViewportState? = null
        var holdInitialization = true
        compose.setContent {
            val viewport = rememberPlateViewport(bitmap, IntSize(400, 600), saved.value) {
                if (holdInitialization) pendingInitialization = it else saved.value = it
            }
            SideEffect { if (!folder.value) rendered = viewport }
            Box(Modifier.size(100.dp).testTag("folder-tile").clickable {
                // Initialization and selection land in the same frame, before
                // Compose can observe the intermediate non-null parent value.
                saved.value = pendingInitialization
                holdInitialization = false
                saved.value = null
                folder.value = false
            })
        }
        compose.runOnIdle { assertNotNull("initializer ran", pendingInitialization) }
        compose.onNodeWithTag("folder-tile").performTouchInput { click() }
        compose.runOnIdle {
            assertNotNull("loaded image needs a viewport after selection", rendered)
            assertEquals("default must also be retained by the parent", rendered, saved.value)
        }
    }

    @Test fun imageAndSurfaceCanArriveInEitherOrderAndSelectionCanResetAfterInitialization() {
        val image = ImageBitmap(200, 300)
        val bitmap = mutableStateOf<ImageBitmap?>(null)
        val size = mutableStateOf(IntSize.Zero)
        val saved = mutableStateOf<ImageViewportState?>(null)
        var rendered: ImageViewportState? = null
        compose.setContent {
            val viewport = rememberPlateViewport(bitmap.value, size.value, saved.value) { saved.value = it }
            SideEffect { rendered = viewport }
        }
        compose.runOnIdle { bitmap.value = image }
        compose.runOnIdle { assertNull(rendered); assertNull(saved.value); size.value = IntSize(400, 600) }
        val fit = ImageViewportState(0f, 0f, 1f)
        compose.runOnIdle { assertEquals(fit, rendered); assertEquals(fit, saved.value); saved.value = null }
        compose.runOnIdle { assertEquals(fit, rendered); assertEquals(fit, saved.value); bitmap.value = null; saved.value = null }
        compose.runOnIdle { assertNull(rendered); assertNull(saved.value); bitmap.value = image }
        compose.runOnIdle { assertEquals(fit, rendered); assertEquals(fit, saved.value) }
    }

    @Test fun retainedPanAndZoomSurviveRemountWithoutPublishingAReplacementDefault() {
        val image = ImageBitmap(200, 300)
        val panned = ImageViewportState(-80f, -120f, 3f)
        val saved = mutableStateOf<ImageViewportState?>(panned)
        val mounted = mutableStateOf(true)
        val published = mutableListOf<ImageViewportState?>()
        val commits = mutableListOf<ImageViewportState?>()
        compose.setContent {
            if (mounted.value) {
                val viewport = rememberPlateViewport(image, IntSize(400, 600), saved.value) {
                    published.add(it)
                    saved.value = it
                }
                SideEffect { commits.add(viewport) }
            }
        }
        compose.runOnIdle { assertEquals(panned, commits.last()); mounted.value = false }
        compose.runOnIdle { mounted.value = true }
        compose.runOnIdle {
            assertEquals(listOf(panned, panned), commits)
            assertEquals(emptyList<ImageViewportState?>(), published)
        }
    }
}
