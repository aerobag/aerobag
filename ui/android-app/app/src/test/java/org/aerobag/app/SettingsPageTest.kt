// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.mutableStateOf
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.assertIsNotSelected
import androidx.compose.ui.test.assertIsSelected
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
            renderView.viewTreeObserver.dispatchOnPreDraw()
            assertNotNull("Visible slider must be in the provider index", E2eProjectionRegistry.read(oldTag)?.bounds)
            assertNotNull("Help uses the same Settings index", E2eProjectionRegistry.read(helpTag)?.bounds)
        }
        compose.onNodeWithTag(oldTag).assertIsDisplayed().performTouchInput { swipe(center, centerLeft) }
        compose.onNodeWithTag(newTag).assertIsDisplayed()
        compose.runOnIdle {
            assertEquals(listOf("opaque-dim" to "10s"), actions)
            if (BuildConfig.AEROBAG_E2E_ENABLED) {
                renderView.viewTreeObserver.dispatchOnPreDraw()
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
            onSelectPage = {}, onSettingsAction = settingsAction, onAircraftLibraryAction = aircraftAction)
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
