// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.height
import androidx.compose.material3.Text
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.mutableStateOf
import androidx.compose.ui.Modifier
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.click
import androidx.compose.ui.test.longClick
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performTouchInput
import androidx.compose.ui.unit.dp
import androidx.test.core.app.ApplicationProvider
import org.aerobag.app.domain.FlightPlanWorkspaceUi
import org.aerobag.app.domain.UiThemeLoader
import org.aerobag.app.generated.*
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.Implementation
import org.robolectric.annotation.Implements

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [34], qualifiers = "w1000dp-h800dp", shadows = [SelectionMagnifierShadow::class])
class FlightPlanWorkspaceTest {
    @get:Rule val compose = createComposeRule()

    @Test fun narrowDividerReceivesPhysicalTaps() = exerciseDivider(320)
    @Test fun wideDividerReceivesPhysicalTaps() = exerciseDivider(960)

    private fun control(id: FlightPlanControlId, symbol: String) = FlightPlanControlUiView(
        id = id, symbolId = symbol, label = id.name, enabled = true, selected = false,
    )

    private fun exerciseDivider(width: Int) {
        val state = mutableStateOf(FlightPlanWorkspaceUi(
            openSlot = FlightPlanSlot.Active, activeLabel = "ACTIVE PLAN", standbyLabel = "STANDBY PLAN",
            activeSummary = "KRNT SEA KUAO", standbySummary = "KPAE KPLU", standbyPlan = null,
            controls = listOf(control(FlightPlanControlId.ToggleOpenPlan, "open_plan"),
                control(FlightPlanControlId.SwapPlans, "swap_plans"),
                control(FlightPlanControlId.CopyActiveToStandby, "copy_plan")),
            clearStandby = control(FlightPlanControlId.ClearStandby, "remove"),
            entryFocusRevision = 1, notice = null,
        ))
        val mounted = mutableStateOf(true)
        val actions = mutableListOf<FlightPlanControlId>()
        val theme = UiThemeLoader.load(ApplicationProvider.getApplicationContext())
        compose.setContent {
            CompositionLocalProvider(LocalAerobagUiTheme provides theme) {
                if (mounted.value) Box(Modifier.width(width.dp).fillMaxSize()) {
                    FlightPlanWorkspaceLayout(state.value.openSlot, Modifier.fillMaxSize(),
                        band = { FlightPlanWorkspaceBand(state.value, actions::add) }) {
                        Text("Open editor")
                    }
                }
            }
        }
        compose.onNodeWithText("STANDBY PLAN").assertIsDisplayed()
        for (id in listOf(FlightPlanControlId.ToggleOpenPlan, FlightPlanControlId.SwapPlans,
            FlightPlanControlId.CopyActiveToStandby)) {
            compose.onNodeWithTag("parity:plan-control:${id.name}").assertIsDisplayed()
                .performTouchInput { click() }
            compose.runOnIdle { assertEquals(id, actions.last()) }
        }
        compose.onNodeWithTag("parity:plan-control:ClearStandby").assertDoesNotExist()
        compose.runOnIdle { actions.clear() }
        compose.onNodeWithTag("parity:plan-collapsed-summary").performTouchInput { longClick() }
        compose.runOnIdle { assertTrue(actions.isEmpty()) }
        compose.onNodeWithTag("parity:plan-collapsed-open").performTouchInput { click() }
        compose.runOnIdle {
            assertEquals(FlightPlanControlId.OpenStandbyPlan, actions.last())
            state.value = state.value.copy(openSlot = FlightPlanSlot.Standby)
        }
        compose.onNodeWithText("ACTIVE PLAN").assertIsDisplayed()
        compose.onNodeWithTag("parity:plan-control:ClearStandby").assertDoesNotExist()
        compose.onNodeWithTag("parity:plan-collapsed-open").performTouchInput { click() }
        compose.runOnIdle {
            assertEquals(FlightPlanControlId.OpenActivePlan, actions.last())
            state.value = state.value.copy(controls = state.value.controls.map {
                if (it.id == FlightPlanControlId.SwapPlans) it.copy(enabled = false, disabledReason = "Empty Standby") else it
            })
            actions.clear()
        }
        compose.onNodeWithTag("parity:plan-control:SwapPlans").performTouchInput { click() }
        compose.runOnIdle { assertEquals(emptyList<FlightPlanControlId>(), actions); mounted.value = false }
        compose.onNodeWithTag("parity:plan-collapsed-summary").assertDoesNotExist()
        compose.runOnIdle { mounted.value = true }
        compose.onNodeWithText("ACTIVE PLAN").assertIsDisplayed()
        compose.runOnIdle { assertEquals(emptyList<FlightPlanControlId>(), actions) }
    }

    @Test fun gridsExpandAndCollapseWithTheDividerRatherThanJumpingBehindIt() {
        val slot = mutableStateOf(FlightPlanSlot.Active)
        val availableHeight = mutableStateOf(600.dp)
        val theme = UiThemeLoader.load(ApplicationProvider.getApplicationContext())
        compose.setContent {
            CompositionLocalProvider(LocalAerobagUiTheme provides theme) {
                FlightPlanWorkspaceLayout(slot.value, Modifier.width(320.dp).height(availableHeight.value), band = { Text("Summary") }) {
                    Text("${slot.value.name} editor")
                }
            }
        }
        fun top() = compose.onNodeWithTag("parity:plan-workspace-divider").fetchSemanticsNode().boundsInRoot.top
        fun gridHeight() = compose.onNodeWithTag("parity:plan-workspace-editor-clip").fetchSemanticsNode().boundsInRoot.height
        val start = top()
        val fullHeight = gridHeight()
        compose.mainClock.autoAdvance = false
        compose.runOnIdle { slot.value = FlightPlanSlot.Standby; availableHeight.value = 700.dp }
        val positions = (1..28).map {
            compose.mainClock.advanceTimeByFrame()
            compose.waitForIdle()
            top() to gridHeight()
        }
        val end = positions.last().first
        assertTrue("divider travels while incoming grid grows: $positions", positions.any { (line, height) ->
            end < line && line < start && height > 0 && height < fullHeight - 1
        })
        assertTrue("footer shrink must not first push the divider downward: $start -> $positions", positions.all { it.first <= start + 1 })
        assertTrue(positions.last().second > fullHeight)
        compose.onNodeWithText("Active editor").assertDoesNotExist()
        compose.onNodeWithText("Standby editor").assertIsDisplayed()
        compose.onNodeWithTag("parity:plan-workspace-transition:idle").assertExists()
    }

    @Test fun standbyRouteAndActiveNavigationLegUseDifferentSharedThemeColors() {
        val theme = UiThemeLoader.load(ApplicationProvider.getApplicationContext())
        assertEquals(theme.flightPlanRoute.standby, routeSegmentColor(theme, org.aerobag.app.domain.RouteSegmentStatus.Remaining, true))
        assertEquals(theme.flightPlanRoute.active, routeSegmentColor(theme, org.aerobag.app.domain.RouteSegmentStatus.Active, true))
    }
}

// Robolectric has no native Surface for the OS magnifier popup. Keep real
// Compose text selection and touch dispatch, replacing only that OS decoration.
@Implements(android.widget.Magnifier::class)
class SelectionMagnifierShadow {
    @Implementation fun show(sourceCenterX: Float, sourceCenterY: Float) {}
    @Implementation fun show(sourceCenterX: Float, sourceCenterY: Float, magnifierCenterX: Float, magnifierCenterY: Float) {}
    @Implementation fun update() {}
    @Implementation fun dismiss() {}
}
