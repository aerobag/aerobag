// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app

import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.mutableStateOf
import androidx.compose.ui.test.assertIsNotEnabled
import androidx.compose.ui.test.assertTextEquals
import androidx.compose.ui.test.click
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performTouchInput
import androidx.test.core.app.ApplicationProvider
import org.aerobag.app.domain.AltitudePlannerDepartureEditorUiView
import org.aerobag.app.domain.UiThemeLoader
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [34], qualifiers = "w900dp-h800dp")
class AltitudePlannerDepartureInputTest {
    @get:Rule val compose = createComposeRule()

    @Test
    fun physicalTapsExplainLockedFieldsAndNowWithoutDispatchingEdits() {
        val reason = "Core says: active navigation uses NOW."
        val state = mutableStateOf(departure().copy(
            enabled = false, disabledReason = reason, timeValue = "1200", whenValue = "NOW", whenIsPast = false,
        ))
        val actions = mutableListOf<String>()
        val explanations = mutableListOf<String>()
        val theme = UiThemeLoader.load(ApplicationProvider.getApplicationContext())
        compose.setContent {
            CompositionLocalProvider(LocalAerobagUiTheme provides theme) {
                DepartureEditorRow(
                    departure = state.value, timeValue = state.value.timeValue, whenValue = state.value.whenValue,
                    onTimeValueChange = { actions.add("time:$it") }, onWhenValueChange = { actions.add("when:$it") },
                    onTimeFocusChange = {}, onWhenFocusChange = {}, onDone = {},
                    onToggleBasis = { actions.add("basis") },
                    onNow = { actions.add(state.value.nowActionUid) }, interactionEnabled = true,
                    onDisabledClick = { explanations.add(state.value.disabledReason!!) },
                )
            }
        }
        for (field in listOf("time", "when", "now")) {
            compose.onNodeWithTag("parity:altitude-planner-departure-$field").assertIsNotEnabled()
                .performTouchInput { click() }
        }
        compose.runOnIdle {
            assertEquals(listOf(reason, reason, reason), explanations)
            assertEquals(emptyList<String>(), actions)
        }
        compose.onNodeWithTag("parity:altitude-planner-departure-basis").performTouchInput { click() }
        compose.runOnIdle { assertEquals(listOf("basis"), actions) }

        compose.runOnIdle { state.value = departure() }
        compose.onNodeWithTag("parity:altitude-planner-departure-now").performTouchInput { click() }
        compose.runOnIdle {
            assertEquals(listOf("basis", "opaque-now-action"), actions)
            state.value = departure().copy(timeValue = "1200", whenValue = "now", whenIsPast = false)
        }
        compose.onNodeWithTag("parity:altitude-planner-departure-time").assertTextEquals("1200")
        compose.onNodeWithTag("parity:altitude-planner-departure-when").assertTextEquals("now")
    }

    private fun departure() = AltitudePlannerDepartureEditorUiView(
        title = "Depart:", timeLabel = "", timeValue = "0400", basisLabel = "Z",
        timeDisplayActionId = "opaque-basis", whenLabel = "=", whenValue = "-8h", whenSuffix = "from now",
        whenIsPast = true, nowLabel = "NOW", nowActionUid = "opaque-now-action", enabled = true, disabledReason = null,
    )
}
