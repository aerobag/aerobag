// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later
package org.aerobag.app

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.mutableStateOf
import androidx.compose.ui.Modifier
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.platform.LocalDensity
import androidx.core.graphics.Insets
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.assertIsFocused
import androidx.compose.ui.test.assertTextEquals
import androidx.compose.ui.test.click
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performTextReplacement
import androidx.compose.ui.test.performTextInput
import androidx.compose.ui.test.performScrollTo
import androidx.compose.ui.test.performTouchInput
import androidx.compose.ui.test.performImeAction
import androidx.compose.ui.test.ExperimentalTestApi
import androidx.compose.ui.test.performKeyInput
import androidx.compose.ui.test.pressKey
import androidx.compose.ui.unit.IntSize
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.isPopupLayout
import android.view.inspector.WindowInspector
import androidx.test.core.app.ApplicationProvider
import org.aerobag.app.domain.FlightDataBannerModel
import org.aerobag.app.domain.FlightDataCell
import org.aerobag.app.domain.FlightDataCellAction
import org.aerobag.app.domain.UiThemeLoader
import org.aerobag.app.generated.FlightDataCommand
import org.aerobag.app.generated.FlightDataEditor
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.shadows.ShadowToast

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [34])
@OptIn(ExperimentalTestApi::class)
class BarometerSettingTrayTest {
    @get:Rule val compose = createComposeRule()

    @Test fun stationOnlyNearestAndCoreInputCorrectionUseTheSharedEditor() {
        val editor = mutableStateOf(FlightDataEditor(id = "barometer", label = "Altimeter setting", unit = "inHg",
            input = "29.92", inputRevision = 0, dismissActionId = "close", closeLabel = "CLOSE",
            notice = "BARO ALT from device is cabin alt. Cross-check.",
            actionRows = listOf(listOf(org.aerobag.app.generated.FlightDataEditorAction(
                id = "nearest", label = "NEAREST", secondaryLabel = "KSMP 52min old", enabled = true, selected = false,
                weatherBadge = org.aerobag.app.generated.FlightPlanWeatherBadgeUiView("vfr", "clear"),
            )))))
        val commands = mutableListOf<FlightDataCommand>()
        val theme = UiThemeLoader.load(ApplicationProvider.getApplicationContext())
        compose.setContent {
            CompositionLocalProvider(LocalAerobagUiTheme provides theme) {
                FlightDataSettingTray(editor.value, commands::add)
            }
        }
        compose.onNodeWithText("KSMP 52min old", useUnmergedTree = true).assertIsDisplayed()
        compose.onNodeWithTag("barometer-nearest-symbol", useUnmergedTree = true).assertIsDisplayed()
        val buttonBounds = compose.onNodeWithTag("barometer-nearest").fetchSemanticsNode().boundsInRoot
        val noticeBounds = compose.onNodeWithText(editor.value.notice).fetchSemanticsNode().boundsInRoot
        assertTrue("cabin altitude notice belongs below the action", noticeBounds.top >= buttonBounds.bottom)
        compose.onNodeWithTag("barometer-setting").assertIsFocused()
        for (character in "3006") compose.onNodeWithTag("barometer-setting").performTextInput(character.toString())
        compose.runOnIdle {
            assertEquals(listOf("3", "30", "300", "3006"), commands.filterIsInstance<FlightDataCommand.SetInput>().map { it.input })
            editor.value = editor.value.copy(input = "30.06", inputCorrection =
                org.aerobag.app.generated.FlightDataInputCorrection(source = "3006", start = 2, end = 2, text = "."))
        }
        compose.onNodeWithTag("barometer-setting").assertTextEquals("30.06")
        // A core correction must preserve the caret, not reselect the entire value.
        compose.onNodeWithTag("barometer-setting").performTextInput("7")
        compose.onNodeWithTag("barometer-setting").assertTextEquals("30.067")
        compose.runOnIdle { editor.value = editor.value.copy(inputCorrection = null) }
        compose.runOnIdle { editor.value = editor.value.copy(inputCorrection =
            org.aerobag.app.generated.FlightDataInputCorrection(source = "3006", start = 2, end = 2, text = ".")) }
        compose.onNodeWithTag("barometer-setting").assertTextEquals("30.067")
        compose.onNodeWithTag("barometer-setting").performKeyInput { pressKey(Key.Enter) }
        compose.runOnIdle { assertEquals(FlightDataCommand.EditorAction(editorId = "barometer", actionId = "close"), commands.last()) }
        compose.onNodeWithTag("barometer-nearest").performTouchInput { click() }
        compose.runOnIdle { assertEquals(FlightDataCommand.EditorAction(editorId = "barometer", actionId = "nearest"), commands.last()) }
    }

    @Test
    @Config(qualifiers = "w400dp-h800dp")
    fun keyboardInsetsKeepTheEditorAboveTheKeyboardAndActionsRemainReachable() {
        val editor = FlightDataEditor(id = "altitude_target", title = "Target altitude", label = "BARO target ft",
            unit = "ft", dismissActionId = "close",
            input = "2500", inputRevision = 0, notice = "Use the aircraft altimeter for assigned altitudes and altitude restrictions.",
            actionRows = listOf("gps", "baro", "decrease", "increase", "off").map {
                org.aerobag.app.generated.FlightDataEditorAction(id = it, label = it, enabled = true, selected = false)
            }.chunked(2), closeLabel = "CLOSE")
        checkKeyboardAvoidance(editor, "off")
    }

    @Test
    @Config(qualifiers = "w400dp-h800dp")
    fun barometerWarningAndControlsFitAboveTheKeyboard() {
        val editor = FlightDataEditor(id = "barometer", title = null, label = "Altimeter setting",
            unit = "inHg", dismissActionId = "close",
            input = "29.90", inputRevision = 0,
            notice = "BARO ALT from device is cabin alt. Cross-check.",
            warning = "Check baro setting:\nKBFI 30.07 66 min ago",
            actionRows = listOf(listOf(org.aerobag.app.generated.FlightDataEditorAction(
                id = "nearest", label = "NEAREST", secondaryLabel = "KBFI 66min old",
                enabled = true, selected = false))), closeLabel = "CLOSE")
        checkKeyboardAvoidance(editor, "nearest")
    }

    private fun checkKeyboardAvoidance(editor: FlightDataEditor, actionId: String) {
        val theme = UiThemeLoader.load(ApplicationProvider.getApplicationContext())
        lateinit var host: android.view.View
        lateinit var popup: android.view.View
        var keyboardHeight = 0
        val commands = mutableListOf<FlightDataCommand>()
        compose.setContent {
            host = LocalView.current
            keyboardHeight = with(LocalDensity.current) { 360.dp.roundToPx() }
            CompositionLocalProvider(LocalAerobagUiTheme provides theme) {
                Box(Modifier.fillMaxSize()) {
                    FlightDataSettingTray(editor, commands::add)
                }
            }
        }
        compose.onNodeWithTag("${editor.id}-setting").assertIsFocused()
        val before = compose.onNodeWithTag("${editor.id}-tray").fetchSemanticsNode().boundsInRoot
        compose.runOnIdle {
            popup = WindowInspector.getGlobalWindowViews().single { isPopupLayout(it) }
            // IME insets belong to the focused popup, not the activity behind it.
            ViewCompat.dispatchApplyWindowInsets(host, WindowInsetsCompat.Builder()
                .setInsets(WindowInsetsCompat.Type.ime(), Insets.NONE)
                .setVisible(WindowInsetsCompat.Type.ime(), false).build())
            ViewCompat.dispatchApplyWindowInsets(popup, WindowInsetsCompat.Builder()
                .setInsets(WindowInsetsCompat.Type.ime(), Insets.of(0, 0, 0, keyboardHeight))
                .setVisible(WindowInsetsCompat.Type.ime(), true).build())
        }
        val after = compose.onNodeWithTag("${editor.id}-tray").fetchSemanticsNode().boundsInRoot
        assertTrue("editor must move above the IME: before=$before after=$after", after.top < before.top)
        assertTrue("editor must fit in the non-keyboard space: $after, popup=${popup.height}, ime=$keyboardHeight",
            after.bottom <= popup.height - keyboardHeight)
        compose.onNodeWithTag("${editor.id}-setting").assertIsDisplayed()
        editor.warning?.let { compose.onNodeWithText(it).performScrollTo().assertIsDisplayed() }
        compose.onNodeWithTag("${editor.id}-$actionId").performScrollTo().performTouchInput { click() }
        compose.runOnIdle {
            assertEquals(listOf(FlightDataCommand.EditorAction(editorId = editor.id, actionId = actionId)), commands)
        }
    }

    @Test fun openingAndReopeningTargetSelectsValueWithoutReselectingOnCoreEcho() {
        var editor = FlightDataEditor(id = "altitude_target", title = "Target altitude", label = "GPS target ft",
            unit = "ft", dismissActionId = "close",
            input = "2500", inputRevision = 0, notice = "Use the aircraft altimeter.", actionRows = emptyList(), closeLabel = "CLOSE")
        val banner = mutableStateOf(FlightDataBannerModel(cells = listOf(
            FlightDataCell(id = "altitude_target", label = "TGT GPS", value = "2500",
                action = FlightDataCellAction("altitude_target", "Set target altitude", symbolId = null)),
        )))
        val commands = mutableListOf<FlightDataCommand>()
        val theme = UiThemeLoader.load(ApplicationProvider.getApplicationContext())
        compose.setContent {
            CompositionLocalProvider(LocalAerobagUiTheme provides theme) {
                Box(Modifier.size(360.dp, 720.dp)) {
                    FlightDataBanner(banner.value, IntSize(360, 720), 0.dp, theme,
                        onCellActivated = { banner.value = banner.value.copy(editor = editor) },
                        onFlightDataCommand = { command ->
                            commands.add(command)
                            when (command) {
                                is FlightDataCommand.SetInput -> {
                                    editor = editor.copy(input = command.input)
                                    banner.value = banner.value.copy(editor = editor)
                                }
                                FlightDataCommand.EditorAction(editorId = editor.id, actionId = "close") ->
                                    banner.value = banner.value.copy(editor = null)
                                else -> Unit
                            }
                        })
                }
            }
        }
        compose.onNodeWithTag("flight-data-cell:altitude_target").performTouchInput { click() }
        compose.onNodeWithTag("altitude_target-setting").assertIsFocused()
        compose.runOnIdle { assertTrue("focus/selection must not send input commands", commands.isEmpty()) }
        compose.onNodeWithTag("altitude_target-setting").performTextInput("2")
        compose.onNodeWithTag("altitude_target-setting").assertTextEquals("2")
        compose.onNodeWithTag("altitude_target-setting").performTextInput("6")
        compose.onNodeWithTag("altitude_target-setting").assertTextEquals("26")
        compose.onNodeWithTag("altitude_target-setting").performKeyInput { pressKey(Key.Enter) }
        compose.onNodeWithTag("altitude_target-setting").assertDoesNotExist()
        compose.onNodeWithTag("flight-data-cell:altitude_target").performTouchInput { click() }
        compose.onNodeWithTag("altitude_target-setting").assertIsFocused()
        compose.onNodeWithTag("altitude_target-setting").performTextInput("3000")
        compose.onNodeWithTag("altitude_target-setting").assertTextEquals("3000")
        compose.runOnIdle {
            assertEquals(listOf("2", "26", "3000"), commands.filterIsInstance<FlightDataCommand.SetInput>().map { it.input })
        }
    }

    @Test fun targetControlsDispatchCoreActionsAndKeepTheCoreNoticeVisible() {
        val editor = FlightDataEditor(
            id = "altitude_target", title = "Target altitude", label = "BARO target ft",
            unit = "ft", dismissActionId = "close",
            input = "1500", inputRevision = 0,
            notice = "Prediction uses this device's cabin-pressure altitude. Cross-check with the aircraft altimeter.",
            warning = "Approaching target altitude. Cross-check aircraft altimeter.",
            actionRows = listOf(
                org.aerobag.app.generated.FlightDataEditorAction(id = "gps", label = "GPS", enabled = true, selected = false),
                org.aerobag.app.generated.FlightDataEditorAction(id = "baro", label = "BARO", enabled = true, selected = true),
                org.aerobag.app.generated.FlightDataEditorAction(id = "increase", label = "+100", enabled = true, selected = false),
                org.aerobag.app.generated.FlightDataEditorAction(id = "off", label = "OFF", enabled = true, selected = false),
            ).chunked(2), closeLabel = "CLOSE",
        )
        val banner = mutableStateOf(FlightDataBannerModel(cells = listOf(
            FlightDataCell(id = "altitude_target", label = "TGT BARO", value = "1500",
                action = FlightDataCellAction("altitude_target", "Set target altitude", symbolId = null)),
        )))
        val commands = mutableListOf<FlightDataCommand>()
        val theme = UiThemeLoader.load(ApplicationProvider.getApplicationContext())
        compose.setContent {
            CompositionLocalProvider(LocalAerobagUiTheme provides theme) {
                Box(Modifier.size(360.dp, 720.dp)) {
                    FlightDataBanner(banner.value, IntSize(360, 720), 0.dp, theme,
                        onCellActivated = { banner.value = banner.value.copy(editor = editor) },
                        onFlightDataCommand = { command ->
                            commands.add(command)
                            when (command) {
                                FlightDataCommand.EditorAction(editorId = editor.id, actionId = "increase") ->
                                    banner.value = banner.value.copy(editor = editor.copy(input = "1600", inputRevision = 1))
                                FlightDataCommand.EditorAction(editorId = editor.id, actionId = "off") ->
                                    banner.value = banner.value.copy(editor = null)
                                else -> Unit
                            }
                        })
                }
            }
        }
        compose.onNodeWithTag("flight-data-cell:altitude_target").performTouchInput { click() }
        compose.onNodeWithText(editor.notice).assertIsDisplayed()
        compose.onNodeWithText(requireNotNull(editor.warning)).assertIsDisplayed()
        compose.onNodeWithTag("altitude_target-close").assertDoesNotExist()
        val gpsBounds = compose.onNodeWithTag("altitude_target-gps").fetchSemanticsNode().boundsInRoot
        val baroBounds = compose.onNodeWithTag("altitude_target-baro").fetchSemanticsNode().boundsInRoot
        val increaseBounds = compose.onNodeWithTag("altitude_target-increase").fetchSemanticsNode().boundsInRoot
        assertTrue("button columns need a gutter", baroBounds.left > gpsBounds.right)
        assertTrue("button rows need a gutter", increaseBounds.top > gpsBounds.bottom)
        compose.onNodeWithTag("altitude_target-gps").performTouchInput { click() }
        compose.onNodeWithTag("altitude_target-increase").performTouchInput { click() }
        compose.onNodeWithTag("altitude_target-setting").assertTextEquals("1600")
        compose.onNodeWithTag("altitude_target-off").performTouchInput { click() }
        compose.runOnIdle {
            assertEquals(listOf("gps", "increase", "off").map {
                FlightDataCommand.EditorAction(editorId = editor.id, actionId = it)
            }, commands)
        }
        compose.onNodeWithTag("altitude_target-setting").assertDoesNotExist()
    }

    @Test fun bannerTapOpensCoreEditorAndTextAndImeDoneReachCore() {
        val disabledReason = "No recent nearby METAR altimeter setting is available for the current position."
        val editor = FlightDataEditor(id = "barometer", title = null, label = "Altimeter setting", input = "29.92", inputRevision = 0,
            unit = "inHg", dismissActionId = "close",
            notice = "BARO ALT from device is cabin alt. Cross-check.",
            actionRows = listOf(listOf(org.aerobag.app.generated.FlightDataEditorAction(id = "nearest", label = "NEAREST", enabled = false, selected = false, disabledReason = disabledReason))), closeLabel = "CLOSE")
        val banner = mutableStateOf(FlightDataBannerModel(cells = listOf(
            FlightDataCell(id = "barometer", label = "BARO ft", value = "336",
                action = FlightDataCellAction("barometer", "Set barometric altimeter", symbolId = null)),
        )))
        val commands = mutableListOf<FlightDataCommand>()
        val theme = UiThemeLoader.load(ApplicationProvider.getApplicationContext())
        compose.setContent {
            CompositionLocalProvider(LocalAerobagUiTheme provides theme) {
                Box(Modifier.size(360.dp, 720.dp)) {
                    FlightDataBanner(banner.value, IntSize(360, 720), 0.dp, theme,
                        onCellActivated = {
                            assertEquals("barometer", it)
                            banner.value = banner.value.copy(editor = editor)
                        },
                        onFlightDataCommand = { command ->
                            commands.add(command)
                            if (command == FlightDataCommand.EditorAction(editorId = "barometer", actionId = "nearest")) {
                                banner.value = banner.value.copy(editor = editor.copy(input = "30.01", inputRevision = 1))
                            }
                            if (command == FlightDataCommand.EditorAction(editorId = "barometer", actionId = "close")) banner.value = banner.value.copy(editor = null)
                        })
                }
            }
        }
        compose.onNodeWithTag("flight-data-cell:barometer").performTouchInput { click() }
        compose.onNodeWithText("inHg").assertIsDisplayed()
        compose.onNodeWithText("Altimeter setting").assertDoesNotExist()
        compose.onNodeWithText("BARO").assertDoesNotExist()
        compose.onNodeWithTag("barometer-setting").performTouchInput { click() }
        compose.onNodeWithTag("barometer-setting").performTextReplacement("29.97")
        compose.onNodeWithTag("barometer-nearest").performTouchInput { click() }
        compose.runOnIdle {
            assertEquals(disabledReason, ShadowToast.getTextOfLatestToast())
            assertEquals(listOf(FlightDataCommand.SetInput(editorId = "barometer", input = "29.97")), commands)
        }
        compose.onNodeWithText(editor.notice).assertIsDisplayed()
        compose.runOnIdle { banner.value = banner.value.copy(editor = editor.copy(actionRows = editor.actionRows.map { row -> row.map { it.copy(enabled = true) } })) }
        compose.onNodeWithTag("barometer-nearest").performTouchInput { click() }
        compose.runOnIdle { assertEquals(FlightDataCommand.EditorAction(editorId = "barometer", actionId = "nearest"), commands.last()) }
        compose.onNodeWithTag("barometer-setting").assertTextEquals("30.01")
        compose.onNodeWithTag("barometer-setting").performImeAction()
        compose.runOnIdle { assertEquals(listOf(FlightDataCommand.SetInput(editorId = "barometer", input = "29.97"), FlightDataCommand.EditorAction(editorId = "barometer", actionId = "nearest"), FlightDataCommand.EditorAction(editorId = "barometer", actionId = "close")), commands) }
        compose.onNodeWithText("inHg").assertDoesNotExist()
    }

    @Test fun targetUnavailableBaroExplainsAndPhysicalEnterCloses() {
        val reason = "This device does not provide a barometric pressure sensor."
        val editor = mutableStateOf<FlightDataEditor?>(FlightDataEditor(
            id = "altitude_target", title = "Target altitude", label = "GPS target ft", input = "2500", inputRevision = 0,
            unit = "ft", dismissActionId = "close",
            notice = "Use the aircraft altimeter.", closeLabel = "CLOSE",
            actionRows = listOf(listOf(org.aerobag.app.generated.FlightDataEditorAction(
                id = "baro", label = "BARO", enabled = false, selected = false, disabledReason = reason))),
        ))
        val commands = mutableListOf<FlightDataCommand>()
        val theme = UiThemeLoader.load(ApplicationProvider.getApplicationContext())
        compose.setContent {
            CompositionLocalProvider(LocalAerobagUiTheme provides theme) {
                editor.value?.let { FlightDataSettingTray(it) { command ->
                    commands.add(command)
                    if (command == FlightDataCommand.EditorAction(editorId = "altitude_target", actionId = "close")) editor.value = null
                } }
            }
        }
        compose.onNodeWithTag("altitude_target-baro").performTouchInput { click() }
        compose.runOnIdle {
            assertEquals(reason, ShadowToast.getTextOfLatestToast())
            assertTrue(commands.isEmpty())
        }
        compose.onNodeWithTag("altitude_target-setting").performTouchInput { click() }
        compose.onNodeWithTag("altitude_target-setting").performKeyInput { pressKey(Key.Enter) }
        compose.runOnIdle {
            assertEquals(listOf(FlightDataCommand.EditorAction(editorId = "altitude_target", actionId = "close")), commands)
        }
        compose.onNodeWithTag("altitude_target-setting").assertDoesNotExist()
    }
}
