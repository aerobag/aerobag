// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app

import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.mutableStateOf
import androidx.compose.ui.Modifier
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.assertWidthIsEqualTo
import androidx.compose.ui.test.click
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performTouchInput
import androidx.compose.ui.unit.dp
import androidx.test.core.app.ApplicationProvider
import org.aerobag.app.domain.AirportNotamUiView
import org.aerobag.app.domain.NotamBadgeUiView
import org.aerobag.app.domain.NotamDetailUiView
import org.aerobag.app.domain.UiThemeLoader
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [34], qualifiers = "w600dp-h800dp")
class NotamBadgedControlTest {
    @get:Rule val compose = createComposeRule()

    @Test
    fun flightPlanBadgeKeepsTheFullWaypointWidthAndReceivesPhysicalTaps() {
        var rowClicks = 0
        val badge = NotamBadgeUiView("N", 1, "subject_notams:navaid:TWF", "TWF: 1 NOTAM",
            NotamDetailUiView("Navaid TWF NOTAMs", "Check official sources.", "None",
                listOf(AirportNotamUiView("one", "NAV", "TWF VOR U/S"))))
        val theme = UiThemeLoader.load(ApplicationProvider.getApplicationContext())
        compose.setContent {
            CompositionLocalProvider(LocalAerobagUiTheme provides theme) {
                FlightPlanDataRow(
                    row = FlightPlanDisplayRow("twf", "TWF", "waypoint", notamBadge = badge),
                    selected = false, dataScrollState = rememberScrollState(),
                    onWaypointClick = { rowClicks++ }, onDataCellAction = {},
                )
            }
        }
        compose.onNodeWithTag("parity:plan-row:twf").assertWidthIsEqualTo(PlanWaypointColumnWidth)
        compose.onNodeWithTag("parity:plate-notam:${badge.actionId}").performTouchInput { click() }
        compose.onNodeWithText("TWF VOR U/S").assertIsDisplayed()
        compose.runOnIdle { assertEquals(0, rowClicks) }
    }

    @Test
    fun physicalBadgeTapOpensReaderNotWaypointAndReaderTracksUpdateAndCancellation() {
        var rowClicks = 0
        val initial = NotamBadgeUiView("N", 1, "subject_notams:airway:V23", "V23: 1 NOTAM",
            NotamDetailUiView("Airway V23 NOTAMs", "Check affected segments.", "None",
                listOf(AirportNotamUiView("one", "FDC 4/0462", "V23 MEA 5400 NORTHBOUND"))))
        val badge = mutableStateOf<NotamBadgeUiView?>(initial)
        val theme = UiThemeLoader.load(ApplicationProvider.getApplicationContext())
        compose.setContent {
            CompositionLocalProvider(LocalAerobagUiTheme provides theme) {
                NotamBadgedControl(badge.value, Modifier.size(240.dp, 64.dp)) {
                    CompactSquareButton(label = "V23", testTag = "test:waypoint", onClick = { rowClicks++ })
                }
            }
        }
        compose.onNodeWithTag("parity:plate-notam:${initial.actionId}").performTouchInput { click() }
        compose.onNodeWithText("V23 MEA 5400 NORTHBOUND").assertIsDisplayed()
        compose.runOnIdle {
            assertEquals(0, rowClicks)
            badge.value = initial.copy(detail = initial.detail.copy(notams = listOf(AirportNotamUiView("one", "Updated", "MEA 6000"))))
        }
        compose.onNodeWithText("MEA 6000").assertIsDisplayed()
        compose.onNodeWithText("V23 MEA 5400 NORTHBOUND").assertDoesNotExist()
        compose.runOnIdle { badge.value = null }
        compose.onNodeWithTag("parity:procedure-notam-modal").assertDoesNotExist()
        compose.onNodeWithTag("parity:plate-notam:${initial.actionId}").assertDoesNotExist()
        compose.onNodeWithTag("test:waypoint").performTouchInput { click() }
        compose.runOnIdle { assertEquals(1, rowClicks) }
    }
}
