// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.mutableStateOf
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.assertWidthIsEqualTo
import androidx.compose.ui.test.click
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performScrollTo
import androidx.compose.ui.test.performTouchInput
import androidx.compose.ui.unit.dp
import androidx.test.core.app.ApplicationProvider
import org.aerobag.app.domain.*
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [34], qualifiers = "w600dp-h800dp")
class MapSelectionTrayInputTest {
    @get:Rule val compose = createComposeRule()

    @Test fun unbadgedInspectorItemsRemainVisibleAndReceivePhysicalTaps() = inspect(false)
    @Test fun badgedInspectorItemsAndNotamButtonsReceiveSeparatePhysicalTaps() = inspect(true)

    private fun inspect(withBadge: Boolean) {
        val action = MapSelectionAction("tfr_text", "TEXT", true, false, null, false, null, null)
        val badge = NotamBadgeUiView("N", 1, "test-tfr", "TFR: 1 NOTAM",
            NotamDetailUiView("TFR NOTAMs", "Check official sources.", "None",
                listOf(AirportNotamUiView("one", "AIRSPACE", "Temporary restriction"))))
        val items = (0..7).map { index ->
            MapSelectionItem(
                id = "tfr-$index", label = "TFR$index", sublabel = "Airspace", description = null,
                distance = null, distanceTarget = null, secondaryDescription = null, detailText = null,
                highlight = MapSelectionHighlight.FeatureRef("tfr-$index"), navRef = null,
                symbolFeature = null, metarFeature = null, weatherDetail = null, automaticActionUid = null,
                pirepFeature = null, airspaceIcon = null, actions = listOf(action),
                notamBadge = badge.takeIf { withBadge && index == 0 },
            )
        }
        val selected = mutableStateOf<MapSelectionItem?>(null)
        val selections = mutableListOf<String>()
        val actions = mutableListOf<String>()
        val theme = UiThemeLoader.load(ApplicationProvider.getApplicationContext())
        compose.setContent {
            CompositionLocalProvider(LocalAerobagUiTheme provides theme) {
                Box(Modifier.size(400.dp, 700.dp)) {
                    MapSelectionTray(
                        state = MapSelectionUiState(Offset.Zero,
                            MapSelectionQueryResult(0.0, 0.0, null,
                                listOf(MapSelectionCategory("airspace", "Airspace", items))), selected.value),
                        modifier = Modifier,
                        onSelectItem = { selected.value = it; selections.add(it.id) },
                        onSelectAction = { item, choice -> actions.add("${item.id}:${choice.id}") },
                    )
                }
            }
        }
        val first = compose.onNodeWithTag("parity:map-selection-item:airspace-TFR0")
        first.assertWidthIsEqualTo(ThumbSize).assertIsDisplayed().performTouchInput { click() }
        compose.onNodeWithTag("parity:map-selection-action:tfr_text").performTouchInput { click() }
        compose.runOnIdle {
            assertEquals(listOf("tfr-0"), selections)
            assertEquals(listOf("tfr-0:tfr_text"), actions)
        }
        if (withBadge) {
            compose.onNodeWithTag("parity:plate-notam:${badge.actionId}").performTouchInput { click() }
            compose.onNodeWithText("Temporary restriction").assertIsDisplayed()
            compose.runOnIdle { assertEquals(listOf("tfr-0"), selections) }
        } else {
            // The real tray owns horizontal scrolling; an offscreen item keeps
            // its size and can still be selected after it is brought into view.
            compose.onNodeWithTag("parity:map-selection-item:airspace-TFR7")
                .performScrollTo().assertWidthIsEqualTo(ThumbSize).assertIsDisplayed()
                .performTouchInput { click() }
            compose.runOnIdle { assertEquals(listOf("tfr-0", "tfr-7"), selections) }
        }
    }
}
