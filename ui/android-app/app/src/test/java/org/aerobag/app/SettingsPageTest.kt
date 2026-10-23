// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.mutableStateOf
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.assertIsNotSelected
import androidx.compose.ui.test.assertIsSelected
import androidx.compose.ui.test.assertIsOff
import androidx.compose.ui.test.assertIsOn
import androidx.compose.ui.test.assertIsNotEnabled
import androidx.compose.ui.test.click
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performScrollTo
import androidx.compose.ui.test.performTouchInput
import androidx.compose.ui.test.swipe
import androidx.compose.ui.unit.dp
import androidx.test.core.app.ApplicationProvider
import org.aerobag.app.domain.UiThemeLoader
import org.aerobag.app.generated.*
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [34], qualifiers = "w1000dp-h800dp")
class SettingsPageTest {
    @get:Rule val compose = createComposeRule()

    @Test fun wideAircraftCardsShareARowAndReceivePhysicalTaps() = aircraftLayout(960, true)
    @Test fun narrowAircraftCardsStackAndReceivePhysicalTaps() = aircraftLayout(320, false)

    @Test fun receiverCoreActionsReceivePhysicalTapsOnNarrowSettingsPage() {
        val actions = mutableListOf<String>()
        val panel = mutableStateOf(UiReceiverPanel(title = "ADS-B receiver", status = "Disconnected", detail = "Connect or pair a new Bluetooth transponder.",
            actions = emptyList(), chooser = UiReceiverChooser(
                devices = listOf(UiReceiverAction(actionId = "opaque-device-17", label = "GTX345  AA:BB", enabled = true)),
                showAll = UiReceiverAction(actionId = "opaque-filter", label = "Show all paired devices", enabled = true), showAllChecked = false,
                refresh = UiReceiverAction(actionId = "receiver:refresh", label = "Refresh paired devices", symbol = "\u21bb", enabled = true),
                pair = UiReceiverAction(actionId = "opaque-pair", label = "Pair a new device", enabled = true)),
            diagnostics = UiReceiverDiagnostics(title = "Diagnostics", detail = "Hidden counters", actions = emptyList())))
        val theme = UiThemeLoader.load(ApplicationProvider.getApplicationContext())
        compose.setContent {
            CompositionLocalProvider(LocalAerobagUiTheme provides theme,
                LocalNavigationPageOptions provides NavigationPagePolicy(emptyList(), 2, AppPage.Map)) {
                Box(Modifier.size(320.dp, 740.dp)) {
                    SettingsPage(page = AppPage.Settings,
                        state = UiSettingsPageState(title = "Settings", summary = "", blocks = listOf(UiSettingsPageBlock.Receiver(panel.value))),
                        navElement = null, mostRecentChartOrPlatePage = AppPage.Map,
                        onOpenPlan = {}, onOpenRecentChartOrPlate = {}, onSelectPage = {},
                        onSettingsAction = { _, _ -> }, onAircraftLibraryAction = { _, _ -> },
                        onReceiverAction = { actions.add(it) })
                }
            }
        }
        compose.onNodeWithText("Hidden counters").assertDoesNotExist()
        compose.onNodeWithTag("parity:receiver:opaque-filter").assertIsOff()
        for (id in listOf("receiver:refresh", "opaque-device-17", "opaque-filter", "opaque-pair")) {
            compose.onNodeWithTag("parity:receiver:$id").performScrollTo().assertIsDisplayed().performTouchInput { click() }
        }
        compose.runOnIdle {
            panel.value = panel.value.copy(chooser = panel.value.chooser!!.copy(showAllChecked = true))
        }
        compose.onNodeWithTag("parity:receiver:opaque-filter").performScrollTo().assertIsOn()
        compose.runOnIdle {
            val chooser = panel.value.chooser!!
            panel.value = panel.value.copy(chooser = chooser.copy(refresh = chooser.refresh.copy(
                enabled = false, disabledReason = "Waiting for paired devices")))
        }
        compose.onNodeWithTag("parity:receiver:receiver:refresh").performScrollTo().assertIsNotEnabled()
            .performTouchInput { click() }
        compose.runOnIdle {
            panel.value = panel.value.copy(status = "Receiving", chooser = null,
                activity = UiReceiverActivity(title = "Reception activity", axisStart = "60 minutes ago", axisEnd = "Now",
                    strips = listOf(UiReceiverActivityStrip(title = "Weather", scaleLabel = "0-4 messages/min",
                        rects = listOf(UiReceiverActivityRect(x = 59100, y = 500, width = 800, height = 500, colorKey = "metar")),
                        legend = listOf(UiReceiverActivityLegend(colorKey = "metar", text = "METAR: last received 3s ago"))))),
                detail = List(11) { "Weather family $it: 2 nonempty / 3 empty; 4000 bytes; last 10s ago; 0 decode errors.\n" }.joinToString(""),
                actions = listOf(UiReceiverAction(actionId = "receiver:stop", label = "Disconnect", enabled = true)))
        }
        compose.onNodeWithText("METAR: last received 3s ago").performScrollTo().assertIsDisplayed()
        compose.runOnIdle {
            val activity = panel.value.activity!!
            panel.value = panel.value.copy(activity = activity.copy(strips = activity.strips.map {
                it.copy(rects = emptyList(), legend = listOf(UiReceiverActivityLegend(colorKey = "metar", text = "METAR: not received")))
            }))
        }
        compose.onNodeWithText("METAR: not received").performScrollTo().assertIsDisplayed()
        compose.onNodeWithTag("parity:receiver:receiver:stop").performScrollTo().assertIsDisplayed().performTouchInput { click() }
        compose.runOnIdle { assertEquals(listOf("receiver:refresh", "opaque-device-17", "opaque-filter", "opaque-pair", "receiver:stop"), actions) }
    }

    @Test fun sliderDragPublishesNewIdentityAndUnmountRemovesIndexedControls() {
        val row = mutableStateOf(UiSettingsPageRow(
            id = "display_dim_timeout", title = "Display dims after...", helpText = "Dim the display",
            kind = UiSettingsRowKind.Slider, valueId = "2m", actionId = "opaque-dim",
            stops = listOf("10s", "30s", "1m", "2m", "5m", "never").map { UiSettingsSliderStop(it, it) },
        ))
        val mounted = mutableStateOf(true)
        val actions = mutableListOf<Pair<String, String>>()
        val theme = UiThemeLoader.load(ApplicationProvider.getApplicationContext())
        lateinit var renderView: android.view.View
        compose.setContent {
            renderView = LocalView.current
            CompositionLocalProvider(
                LocalAerobagUiTheme provides theme,
                LocalNavigationPageOptions provides NavigationPagePolicy(emptyList(), 2, AppPage.Map),
            ) {
                if (mounted.value) Box(Modifier.size(400.dp, 740.dp)) {
                    page(UiSettingsPageState(title = "Settings", summary = "", blocks = listOf(UiSettingsPageBlock.Controls(listOf(row.value)))),
                        settingsAction = { id, value -> actions.add(id to value); row.value = row.value.copy(valueId = value) })
                }
            }
        }
        val oldTag = "parity:settings-slider:display_dim_timeout:2m"
        val newTag = "parity:settings-slider:display_dim_timeout:10s"
        val helpTag = "parity:settings-help:display_dim_timeout"
        if (BuildConfig.AEROBAG_E2E_ENABLED) compose.runOnIdle {
            renderView.drawObservationFrame()
            assertNotNull("Visible slider must be in the provider index", E2eProjectionRegistry.read(oldTag)?.bounds)
            assertNotNull("Help uses the same Settings index", E2eProjectionRegistry.read(helpTag)?.bounds)
        }
        compose.onNodeWithTag(oldTag, useUnmergedTree = true).assertIsDisplayed().performTouchInput { swipe(center, centerLeft) }
        compose.onNodeWithTag(newTag, useUnmergedTree = true).assertIsDisplayed()
        compose.runOnIdle {
            assertEquals(listOf("opaque-dim" to "10s"), actions)
            if (BuildConfig.AEROBAG_E2E_ENABLED) {
                renderView.drawObservationFrame()
                assertNull(E2eProjectionRegistry.read(oldTag))
                assertNotNull(E2eProjectionRegistry.read(newTag)?.bounds)
            }
            mounted.value = false
        }
        compose.runOnIdle {
            assertNull(E2eProjectionRegistry.read(newTag))
            assertNull(E2eProjectionRegistry.read(helpTag))
        }
    }

    @Test fun sparseSliderDragUsesEndpointWithCoreRefreshDuringGesture() = sparseSliderDrag(true)

    // ADB's timed swipe may end without a MOVE at the endpoint. Compose uses
    // the last MOVE, not the UP position; that gesture cannot select an exact stop.
    @Test fun endpointOnlyOnUpDoesNotSubstituteForTheLastMove() = sparseSliderDrag(false)

    private fun sparseSliderDrag(sendEndpointMove: Boolean) {
        val row = mutableStateOf(UiSettingsPageRow(
            id = "display_dim_timeout", title = "Display dims after...", helpText = "Dim the display",
            kind = UiSettingsRowKind.Slider, valueId = "2m", actionId = "opaque-dim",
            stops = listOf("10s", "30s", "1m", "2m", "5m", "never").map { UiSettingsSliderStop(it, it) },
        ))
        val actions = mutableListOf<Pair<String, String>>()
        val theme = UiThemeLoader.load(ApplicationProvider.getApplicationContext())
        compose.setContent {
            CompositionLocalProvider(LocalAerobagUiTheme provides theme,
                LocalNavigationPageOptions provides NavigationPagePolicy(emptyList(), 2, AppPage.Map)) {
                Box(Modifier.size(400.dp, 740.dp)) {
                    page(UiSettingsPageState(title = "Settings", summary = "", blocks = listOf(
                        UiSettingsPageBlock.Controls(listOf(row.value)))),
                        settingsAction = { id, value -> actions.add(id to value); row.value = row.value.copy(valueId = value) })
                }
            }
        }
        val slider = compose.onNodeWithTag("parity:settings-slider:display_dim_timeout:2m", useUnmergedTree = true)
        slider.performTouchInput {
            down(Offset(width * 0.75f, height * 0.72f))
            moveTo(Offset(width * 0.2f, height * 0.72f), 500)
        }
        compose.runOnIdle {
            assertTrue("No command before finger-up", actions.isEmpty())
            row.value = row.value.copy(title = "Refreshed display setting",
                stops = row.value.stops.map { it.copy() })
        }
        compose.onNodeWithText("Refreshed display setting").assertIsDisplayed()
        slider.performTouchInput {
            val endpoint = Offset(0.5f, height * 0.72f)
            if (sendEndpointMove) moveTo(endpoint, 150) else {
                advanceEventTime(150)
                updatePointerTo(0, endpoint)
            }
            up()
        }
        compose.runOnIdle { assertEquals(listOf("opaque-dim" to if (sendEndpointMove) "10s" else "30s"), actions) }
    }

    @Test fun publishedSliderRangeSelectsEveryStopByPhysicalTapAcrossRefreshAndRemount() {
        val row = mutableStateOf(UiSettingsPageRow(
            id = "display_dim_timeout", title = "Display dims after...", helpText = "Dim the display",
            kind = UiSettingsRowKind.Slider, valueId = "2m", actionId = "opaque-dim",
            stops = listOf("10s", "30s", "1m", "2m", "5m", "never").map { UiSettingsSliderStop(it, it) },
        ))
        val mounted = mutableStateOf(true)
        val actions = mutableListOf<Pair<String, String>>()
        val theme = UiThemeLoader.load(ApplicationProvider.getApplicationContext())
        lateinit var window: android.view.View
        compose.setContent {
            window = LocalView.current
            CompositionLocalProvider(LocalAerobagUiTheme provides theme,
                LocalNavigationPageOptions provides NavigationPagePolicy(emptyList(), 2, AppPage.Map)) {
                if (mounted.value) Box(Modifier.size(320.dp, 740.dp)) {
                    page(UiSettingsPageState(title = "Settings", summary = "", blocks = listOf(
                        UiSettingsPageBlock.Controls(listOf(row.value)))),
                        settingsAction = { id, value -> actions.add(id to value); row.value = row.value.copy(valueId = value) })
                }
            }
        }
        for (stops in listOf(listOf("10s", "30s", "1m", "2m", "5m", "never"), listOf("30m", "1h"))) {
            compose.runOnIdle { row.value = row.value.copy(stops = stops.map { UiSettingsSliderStop(it, it) }, valueId = stops.last()) }
            for ((index, stop) in stops.withIndex()) {
                val tag = "parity:settings-slider:display_dim_timeout:${row.value.valueId}"
                compose.runOnIdle {
                    window.drawObservationFrame()
                    val fields = E2eProjectionRegistry.read(tag)!!.state.split(':').chunked(2).associate { it[0] to it[1] }
                    assertEquals("horizontal-progress", fields["kind"])
                    assertEquals("0.0", fields["min"])
                    assertEquals(stops.lastIndex.toFloat().toString(), fields["max"])
                }
                val slider = compose.onNodeWithTag(tag, useUnmergedTree = true)
                slider.performTouchInput {
                    down(Offset((width * index / stops.lastIndex.toFloat()).coerceIn(0.5f, width - 0.5f), height / 2f))
                }
                compose.runOnIdle { row.value = row.value.copy(title = "Updated label for $stop") }
                slider.performTouchInput { up() }
                compose.runOnIdle {
                    assertEquals(stop, row.value.valueId)
                    assertEquals("opaque-dim" to stop, actions.last())
                    assertEquals(index + 1, actions.size)
                    window.drawObservationFrame()
                    assertNull(E2eProjectionRegistry.read(tag))
                }
            }
            val tag = "parity:settings-slider:display_dim_timeout:${stops.last()}"
            compose.runOnIdle { mounted.value = false }
            compose.runOnIdle {
                window.drawObservationFrame()
                assertNull(E2eProjectionRegistry.read(tag))
                actions.clear()
                mounted.value = true
            }
        }
    }

    private fun aircraftLayout(width: Int, multiColumn: Boolean) {
        val actions = mutableListOf<Pair<String, String>>()
        mount(UiSettingsPageState(title = "Settings", summary = "", blocks = listOf(
            UiSettingsPageBlock.Controls(emptyList()), UiSettingsPageBlock.AircraftLibrary(library()),
        )), width, aircraftAction = { id, source -> actions.add(id to source) })
        val first = compose.onNodeWithTag("settings-aircraft-entry-a").fetchSemanticsNode().boundsInRoot
        val second = compose.onNodeWithTag("settings-aircraft-entry-b").fetchSemanticsNode().boundsInRoot
        if (multiColumn) {
            assertEquals(first.top, second.top, 0.5f)
            assertTrue(second.left > first.right)
        } else {
            assertEquals(first.left, second.left, 0.5f)
            assertTrue(second.top > first.bottom)
        }
        compose.onNodeWithTag("parity:settings-aircraft:toggle-a").assertIsDisplayed().performTouchInput { click() }
        // The second card is hidden/dimmed; its Show and Edit buttons must still work.
        compose.onNodeWithTag("parity:settings-aircraft:toggle-b").performScrollTo().assertIsDisplayed().performTouchInput { click() }
        compose.onNodeWithTag("parity:settings-aircraft:edit-b").performScrollTo().assertIsDisplayed().performTouchInput { click() }
        compose.runOnIdle { assertEquals(listOf("toggle-a" to "", "toggle-b" to "", "edit-b" to ""), actions) }
    }

    @Test fun sectionExpansionAndOrderComeOnlyFromCore() {
        val section = UiSettingsPageSection(
            id = "debug", title = "Diagnostics", expanded = false,
            toggleAction = UiSettingsAction(actionId = "opaque-expand", valueId = "next"),
            rows = listOf(UiSettingsPageRow(id = "flag", title = "Diagnostic flag",
                kind = UiSettingsRowKind.Toggle, valueId = "off", actionId = "opaque-flag", stops = emptyList())),
        )
        val state = mutableStateOf(UiSettingsPageState(title = "Settings", summary = "", blocks = listOf(
            UiSettingsPageBlock.Section(section), UiSettingsPageBlock.AircraftLibrary(library()),
        )))
        val actions = mutableListOf<Pair<String, String>>()
        val theme = UiThemeLoader.load(ApplicationProvider.getApplicationContext())
        compose.setContent {
            CompositionLocalProvider(
                LocalAerobagUiTheme provides theme,
                LocalNavigationPageOptions provides NavigationPagePolicy(emptyList(), 2, AppPage.Map),
            ) {
                Box(Modifier.size(960.dp, 740.dp)) { page(state.value, { id, value -> actions.add(id to value) }) }
            }
        }
        val toggle = compose.onNodeWithTag("parity:settings-section:debug")
        val header = toggle.fetchSemanticsNode().boundsInRoot
        val card = compose.onNodeWithTag("settings-aircraft-entry-a").fetchSemanticsNode().boundsInRoot
        assertTrue(header.bottom < card.top)
        toggle.assertIsNotSelected().performTouchInput { click() }
        toggle.assertIsNotSelected()
        compose.onNodeWithText("Diagnostic flag").assertDoesNotExist()
        compose.runOnIdle {
            assertEquals(listOf("opaque-expand" to "next"), actions)
            state.value = state.value.copy(blocks = listOf(
                UiSettingsPageBlock.Section(section.copy(expanded = true)),
                UiSettingsPageBlock.AircraftLibrary(library()),
            ))
        }
        toggle.assertIsSelected()
        compose.onNodeWithText("Diagnostic flag").assertIsDisplayed()
    }

    @Test fun toggleSurvivesCorePublicationBetweenFingerDownAndUp() {
        val section = UiSettingsPageSection(
            id = "debug", title = "Diagnostics", expanded = true,
            toggleAction = UiSettingsAction(actionId = "expand", valueId = "next"),
            rows = listOf(UiSettingsPageRow(id = "flag", title = "Diagnostic flag",
                kind = UiSettingsRowKind.Toggle, valueId = "off", actionId = "flag", stops = emptyList())),
        )
        val state = mutableStateOf(UiSettingsPageState(title = "Settings", summary = "", blocks = listOf(
            UiSettingsPageBlock.Section(section),
        )))
        val actions = mutableListOf<Pair<String, String>>()
        val theme = UiThemeLoader.load(ApplicationProvider.getApplicationContext())
        compose.setContent {
            CompositionLocalProvider(
                LocalAerobagUiTheme provides theme,
                LocalNavigationPageOptions provides NavigationPagePolicy(emptyList(), 2, AppPage.Map),
            ) {
                Box(Modifier.size(400.dp, 740.dp)) { page(state.value, { id, value -> actions.add(id to value) }) }
            }
        }
        val toggle = compose.onNodeWithTag("parity:settings-toggle:flag")
        toggle.assertIsDisplayed().performTouchInput { down(center) }
        compose.runOnIdle {
            state.value = state.value.copy(blocks = listOf(UiSettingsPageBlock.Section(
                section.copy(title = "Diagnostics updated", rows = section.rows.map { it.copy(title = "Updated flag") }),
            )))
        }
        compose.onNodeWithText("Updated flag").assertIsDisplayed()
        toggle.performTouchInput { advanceEventTime(80); up() }
        compose.runOnIdle { assertEquals(listOf("flag" to "on"), actions) }
    }

    @Test fun gridReplacementBetweenFingerDownAndUpCannotToggleAnotherSetting() {
        fun item(id: String) = UiSettingsGridItem(
            cell = FlightDataCell(id = id, label = id, value = null, action = null,
                tone = FlightDataCellTone.Planned, estimateKind = FlightEstimateKind.Basic, attention = null),
            enabled = true,
        )
        val row = mutableStateOf(UiSettingsPageRow(id = "flight_data_visibility", title = "Grid",
            kind = UiSettingsRowKind.GridChoices, valueId = "", actionId = "visibility", stops = emptyList(),
            items = listOf(item("altitude"), item("target"), item("agl"))))
        val actions = mutableListOf<Pair<String, String>>()
        val theme = UiThemeLoader.load(ApplicationProvider.getApplicationContext())
        compose.setContent {
            CompositionLocalProvider(LocalAerobagUiTheme provides theme,
                LocalNavigationPageOptions provides NavigationPagePolicy(emptyList(), 2, AppPage.Map)) {
                Box(Modifier.size(400.dp, 740.dp)) {
                    page(UiSettingsPageState(title = "Settings", summary = "", blocks = listOf(
                        UiSettingsPageBlock.Controls(listOf(row.value)))),
                        settingsAction = { id, value -> actions.add(id to value) })
                }
            }
        }
        val tag = "parity:settings-choice:flight_data_visibility:agl"
        val choice = compose.onNodeWithTag(tag)
        choice.performTouchInput { down(center) }
        compose.runOnIdle { row.value = row.value.copy(items = listOf(item("barometer")) + row.value.items) }
        // Release at the old screen location, now occupied by a different ID.
        choice.performTouchInput { advanceEventTime(80); up() }
        compose.runOnIdle { assertTrue("Must never retarget a held gesture: $actions",
            actions.all { it.second == "agl" }) }
        compose.runOnIdle { actions.clear() }
        choice.performTouchInput { click() }
        compose.runOnIdle { assertEquals(listOf("visibility" to "agl"), actions) }
    }

    private fun mount(state: UiSettingsPageState, width: Int, aircraftAction: (String, String) -> Unit) {
        val theme = UiThemeLoader.load(ApplicationProvider.getApplicationContext())
        compose.setContent {
            CompositionLocalProvider(
                LocalAerobagUiTheme provides theme,
                LocalNavigationPageOptions provides NavigationPagePolicy(emptyList(), 2, AppPage.Map),
            ) {
                Box(Modifier.size(width.dp, 740.dp)) { page(state, aircraftAction = aircraftAction) }
            }
        }
    }

    @androidx.compose.runtime.Composable
    private fun page(state: UiSettingsPageState, settingsAction: (String, String) -> Unit = { _, _ -> }, aircraftAction: (String, String) -> Unit = { _, _ -> }) {
        SettingsPage(page = AppPage.Settings, state = state, navElement = null,
            mostRecentChartOrPlatePage = AppPage.Map, onOpenPlan = {}, onOpenRecentChartOrPlate = {},
            onSelectPage = {}, onSettingsAction = settingsAction, onAircraftLibraryAction = aircraftAction, onReceiverAction = {})
    }

    private fun action(id: String, label: String) = UiAircraftLibraryAction(actionId = id, label = label, enabled = true)

    private fun library() = UiAircraftLibraryState(
        title = "Aircraft library", summary = "Choose aircraft", columnMinWidthThumbs = 4.2, columnGapThumbs = 0.1,
        addAction = action("add", "Add aircraft"), entries = listOf("a", "b").map { id ->
            UiAircraftLibraryEntry(definitionHash = id, label = "Airplane $id", sourceLabel = "USER", included = id == "a",
                symbol = UiAircraftSymbol(pathData = "M -10 10 L 0 -10 L 10 10 Z", rotationDegrees = -45),
                toggleAction = action("toggle-$id", if (id == "a") "Hide" else "Show"), editAction = action("edit-$id", "Edit"))
        },
    )
}
