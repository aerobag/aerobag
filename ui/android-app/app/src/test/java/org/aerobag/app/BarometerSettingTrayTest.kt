// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later
package org.aerobag.app

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.mutableStateOf
import androidx.compose.ui.Modifier
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.assertIsNotEnabled
import androidx.compose.ui.test.assertTextEquals
import androidx.compose.ui.test.click
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performTextReplacement
import androidx.compose.ui.test.performTouchInput
import androidx.compose.ui.unit.IntSize
import androidx.compose.ui.unit.dp
import androidx.test.core.app.ApplicationProvider
import org.aerobag.app.domain.FlightDataBannerModel
import org.aerobag.app.domain.FlightDataCell
import org.aerobag.app.domain.FlightDataCellAction
import org.aerobag.app.domain.UiThemeLoader
import org.aerobag.app.generated.BarometerCommand
import org.aerobag.app.generated.BarometerEditor
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [34])
class BarometerSettingTrayTest {
    @get:Rule val compose = createComposeRule()

    @Test fun bannerTapOpensCoreEditorAndTextAndCloseReachCore() {
        val editor = BarometerEditor(title = "BARO", label = "Altimeter inHg", input = "29.92", inputRevision = 0,
            nearestLabel = "NEAREST", nearestEnabled = false, closeLabel = "CLOSE")
        val banner = mutableStateOf(FlightDataBannerModel(cells = listOf(
            FlightDataCell(id = "barometer", label = "BARO ft", value = "336",
                action = FlightDataCellAction("barometer", "Set barometric altimeter", symbolId = null)),
        )))
        val commands = mutableListOf<BarometerCommand>()
        val theme = UiThemeLoader.load(ApplicationProvider.getApplicationContext())
        compose.setContent {
            CompositionLocalProvider(LocalAerobagUiTheme provides theme) {
                Box(Modifier.size(360.dp, 720.dp)) {
                    FlightDataBanner(banner.value, IntSize(360, 720), 0.dp, theme,
                        onCellActivated = {
                            assertEquals("barometer", it)
                            banner.value = banner.value.copy(barometerEditor = editor)
                        },
                        onBarometerCommand = { command ->
                            commands.add(command)
                            if (command == BarometerCommand.UseNearest) {
                                banner.value = banner.value.copy(barometerEditor = editor.copy(input = "30.01", inputRevision = 1))
                            }
                            if (command == BarometerCommand.CloseEditor) banner.value = banner.value.copy(barometerEditor = null)
                        })
                }
            }
        }
        compose.onNodeWithTag("flight-data-cell:barometer").performTouchInput { click() }
        compose.onNodeWithText("Altimeter inHg").assertIsDisplayed()
        compose.onNodeWithTag("barometer-setting").performTouchInput { click() }
        compose.onNodeWithTag("barometer-setting").performTextReplacement("29.97")
        compose.onNodeWithTag("barometer-nearest").assertIsNotEnabled()
        compose.runOnIdle { banner.value = banner.value.copy(barometerEditor = editor.copy(nearestEnabled = true)) }
        compose.onNodeWithTag("barometer-nearest").performTouchInput { click() }
        compose.onNodeWithTag("barometer-setting").assertTextEquals("30.01")
        compose.onNodeWithTag("barometer-close").performTouchInput { click() }
        compose.runOnIdle { assertEquals(listOf(BarometerCommand.SetSetting("29.97"), BarometerCommand.UseNearest, BarometerCommand.CloseEditor), commands) }
        compose.onNodeWithText("Altimeter inHg").assertDoesNotExist()
    }
}
