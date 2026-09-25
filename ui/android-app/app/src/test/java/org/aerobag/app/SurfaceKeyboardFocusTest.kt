// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableStateOf
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.test.assertIsFocused
import androidx.compose.ui.test.click
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performTouchInput
import androidx.compose.ui.unit.dp
import androidx.test.core.app.ApplicationProvider
import org.aerobag.app.domain.UiThemeLoader
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [34])
class SurfaceKeyboardFocusTest {
    @get:Rule val compose = createComposeRule()

    @Test fun delayedSurfaceFocusCannotStealAnEditorsPhysicalTap() {
        val resource = mutableStateOf("first")
        exerciseFocusRace(resource = { resource.value }) { resource.value = "second" }
    }

    @Test fun closingAMenuCannotStealAnEditorsPhysicalTap() {
        val enabled = mutableStateOf(false)
        exerciseFocusRace(enabled = { enabled.value }) { enabled.value = true }
    }

    @Test fun remountedSurfaceCannotStealAnEditorsPhysicalTap() {
        val resource = mutableStateOf("first")
        exerciseFocusRace(resource = { resource.value }, remount = true) { resource.value = "second" }
    }

    private fun exerciseFocusRace(
        resource: () -> Any? = { "chart" },
        enabled: () -> Boolean = { true },
        remount: Boolean = false,
        restartEffect: () -> Unit,
    ) {
        val theme = UiThemeLoader.load(ApplicationProvider.getApplicationContext())
        val lifetime = mutableStateOf(0)
        compose.setContent {
            CompositionLocalProvider(LocalAerobagUiTheme provides theme) {
                key(lifetime.value) {
                    Box(Modifier.size(300.dp).testTag("surface").surfaceKeyboardFocus(resource(), enabled())) {
                        AndroidChartSearchBox("", false, false, null, emptyList(), {}, {}, {}, {})
                    }
                }
            }
        }
        if (remount) {
            compose.runOnIdle { lifetime.value++ }
            compose.waitForIdle()
        }
        compose.mainClock.autoAdvance = false
        compose.runOnIdle(restartEffect)
        // Recompose the surface, leaving its delayed focus effect at the next frame.
        compose.mainClock.advanceTimeByFrame()
        compose.onNodeWithTag("parity:chart-search-input").performTouchInput { click() }
        compose.onNodeWithTag("parity:chart-search-input").assertIsFocused()
        compose.mainClock.advanceTimeByFrame()
        compose.onNodeWithTag("parity:chart-search-input").assertIsFocused()
    }
}
