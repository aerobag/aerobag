// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.setValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.window.Popup
import androidx.compose.ui.window.PopupProperties
import org.aerobag.app.generated.BarometerCommand
import org.aerobag.app.generated.BarometerEditor

@Composable
internal fun BarometerSettingTray(editor: BarometerEditor, onCommand: (BarometerCommand) -> Unit) {
    val theme = LocalAerobagUiTheme.current
    // Keep the native edit buffer responsive while core processes the raw input.
    // The editor's lifetime, parsing, calibration and errors remain core-owned.
    var input by remember { mutableStateOf(editor.input) }
    LaunchedEffect(editor.inputRevision) { input = editor.input }
    val close = { onCommand(BarometerCommand.CloseEditor) }
    Popup(onDismissRequest = close, properties = PopupProperties(focusable = true)) {
        Box(Modifier.fillMaxSize()) {
            Scrim(onDismiss = close)
            MenuPanel(modifier = Modifier.align(Alignment.Center), width = ThumbSize * 3f) {
                MenuPanelHeader(editor.title, false, ThumbSize * 2.8f)
                Text(editor.label, modifier = Modifier.padding(ThumbGap))
                BasicTextField(
                    value = input,
                    onValueChange = { input = it; onCommand(BarometerCommand.SetSetting(it)) },
                    singleLine = true,
                    keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Decimal),
                    textStyle = MaterialTheme.typography.headlineMedium.copy(color = theme.controls.panelFg),
                    modifier = Modifier.fillMaxWidth().height(ThumbSize)
                        .background(theme.controls.textInputBg)
                        .e2eIndexedControl(semanticTag = "barometer-setting", enabled = true)
                        .padding(ThumbGap),
                )
                editor.error?.let { Text(it, color = theme.controls.dataStatusWarningStroke, modifier = Modifier.padding(ThumbGap)) }
                MenuPanelRow(editor.nearestLabel, active = false, enabled = editor.nearestEnabled,
                    testTag = "barometer-nearest", onSelect = { onCommand(BarometerCommand.UseNearest) })
                editor.nearestDetail?.let { Text(it, modifier = Modifier.padding(ThumbGap)) }
                MenuPanelRow(editor.closeLabel, active = false, enabled = true,
                    testTag = "barometer-close", onSelect = close)
            }
        }
    }
}
