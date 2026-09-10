// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app

import androidx.compose.runtime.mutableStateOf
import androidx.compose.ui.test.junit4.createComposeRule
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [34])
class AirwayRoutingNavigationEffectTest {
    @get:Rule val compose = createComposeRule()

    @Test
    fun openingANewEditorNavigatesOnceButPageReentryDoesNot() {
        val visible = mutableStateOf(true)
        val activeEdit = mutableStateOf<String?>(null)
        var navigations = 0
        compose.setContent {
            if (visible.value) {
                AirwayRoutingNavigationEffect(activeEdit.value) { navigations++ }
            }
        }
        compose.runOnIdle { activeEdit.value = "first-draft" }
        compose.runOnIdle { assertEquals(1, navigations); visible.value = false }
        // Mirrors Android's page switch: the page really leaves composition.
        compose.runOnIdle { visible.value = true }
        compose.runOnIdle { assertEquals(1, navigations) }
        compose.runOnIdle { activeEdit.value = "second-draft" }
        compose.runOnIdle { assertEquals(2, navigations) }
        compose.runOnIdle { activeEdit.value = null }
        compose.runOnIdle { activeEdit.value = "third-draft" }
        compose.runOnIdle { assertEquals(3, navigations) }
    }

    @Test
    fun existingDraftAtMountIsStateNotANavigationEvent() {
        var navigations = 0
        compose.setContent { AirwayRoutingNavigationEffect("existing-draft") { navigations++ } }
        compose.runOnIdle { assertEquals(0, navigations) }
    }
}
