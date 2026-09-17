// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app

import androidx.compose.foundation.background
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.ime
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.setValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.platform.LocalSoftwareKeyboardController
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.input.key.onPreviewKeyEvent
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.TextFieldValue
import androidx.compose.ui.text.TextRange
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.window.Popup
import androidx.compose.ui.window.PopupProperties
import org.aerobag.app.generated.FlightDataCommand
import org.aerobag.app.generated.FlightDataEditor

@Composable
internal fun FlightDataSettingTray(editor: FlightDataEditor, onCommand: (FlightDataCommand) -> Unit) {
    val theme = LocalAerobagUiTheme.current
    // Keep the native edit buffer responsive while core processes the raw input.
    // The editor's lifetime, parsing, calibration and errors remain core-owned.
    var input by remember(editor.id) {
        mutableStateOf(TextFieldValue(editor.input, selection = TextRange(0, editor.input.length)))
    }
    LaunchedEffect(editor.id, editor.inputRevision) {
        input = TextFieldValue(editor.input, selection = TextRange(0, editor.input.length))
    }
    LaunchedEffect(editor.id, editor.inputCorrection) {
        editor.inputCorrection?.let { edit ->
            if (input.text == edit.source) {
                val start = edit.start.toInt()
                val end = edit.end.toInt()
                fun correctedPosition(position: Int): Int = when {
                    position <= start -> position
                    position >= end -> position + edit.text.length - (end - start)
                    else -> start + edit.text.length
                }
                input = TextFieldValue(input.text.replaceRange(start, end, edit.text),
                    selection = TextRange(correctedPosition(input.selection.start), correctedPosition(input.selection.end)))
            }
        }
    }
    val focusRequester = remember(editor.id) { FocusRequester() }
    val action = { id: String -> onCommand(FlightDataCommand.EditorAction(editorId = editor.id, actionId = id)) }
    val close = { action(editor.dismissActionId) }
    Popup(onDismissRequest = close, properties = PopupProperties(focusable = true)) {
        // The popup owns keyboard focus. Its IME insets can differ from those of
        // the activity underneath, which deliberately keeps the map unresized.
        val imeInsets = WindowInsets.ime
        val keyboardController = LocalSoftwareKeyboardController.current
        Box(Modifier.fillMaxSize()) {
            Scrim(onDismiss = close)
            BoxWithConstraints(Modifier.fillMaxSize().windowInsetsPadding(imeInsets).padding(ThumbGap)) {
                LaunchedEffect(editor.id) {
                    focusRequester.requestFocus()
                    keyboardController?.show()
                }
                val availableHeight = maxHeight
                MenuPanel(modifier = Modifier.align(Alignment.Center).testTag("${editor.id}-tray"), width = ThumbSize * 3.8f) {
                    Column(Modifier.heightIn(max = availableHeight).verticalScroll(rememberScrollState())) {
                        editor.title?.let { MenuPanelHeader(it, false, ThumbSize * 2.8f) }
                        Row(verticalAlignment = Alignment.CenterVertically,
                            horizontalArrangement = Arrangement.spacedBy(ThumbGap)) {
                            BasicTextField(
                                value = input,
                                onValueChange = { next ->
                                    val textChanged = next.text != input.text
                                    input = next
                                    if (textChanged) onCommand(FlightDataCommand.SetInput(editorId = editor.id, input = next.text))
                                },
                                singleLine = true,
                                keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Decimal, imeAction = ImeAction.Done),
                                keyboardActions = KeyboardActions(onDone = { close() }),
                                textStyle = MaterialTheme.typography.headlineMedium.copy(color = theme.controls.panelFg),
                                modifier = Modifier.weight(1f).height(ThumbSize)
                                    .semantics { contentDescription = editor.label }
                                    .background(theme.controls.textInputBg)
                                    .e2eIndexedControl(semanticTag = "${editor.id}-setting", enabled = true)
                                    .focusRequester(focusRequester)
                                    .onPreviewKeyEvent { event ->
                                        if (event.nativeKeyEvent.keyCode != android.view.KeyEvent.KEYCODE_ENTER &&
                                            event.nativeKeyEvent.keyCode != android.view.KeyEvent.KEYCODE_NUMPAD_ENTER) {
                                            return@onPreviewKeyEvent false
                                        }
                                        if (event.nativeKeyEvent.action == android.view.KeyEvent.ACTION_UP) close()
                                        true
                                    }
                                    .padding(ThumbGap),
                            )
                            Text(editor.unit, modifier = Modifier.padding(end = ThumbGap))
                        }
                        editor.error?.let { Text(it, color = theme.controls.dataStatusWarningStroke, modifier = Modifier.padding(ThumbGap)) }
                        editor.warning?.let { Text(it, color = theme.controls.panelFg,
                            modifier = Modifier.fillMaxWidth().background(theme.controls.dataStatusCautionBg).padding(ThumbGap)) }
                        Column(modifier = Modifier.padding(top = ThumbGap), verticalArrangement = Arrangement.spacedBy(ThumbGap)) {
                            editor.actionRows.forEach { row ->
                                Row(horizontalArrangement = Arrangement.spacedBy(ThumbGap)) {
                                    row.forEach { item ->
                                        MenuPanelRow(item.label, active = item.selected, enabled = item.enabled,
                                            secondaryLabel = item.secondaryLabel,
                                            trailingContent = if (item.symbolFeature != null || item.weatherBadge != null) {
                                                { PlanWaypointSymbol(item.symbolFeature, weatherBadge = item.weatherBadge,
                                                    modifier = Modifier.testTag("${editor.id}-${item.id}-symbol")) }
                                            } else null,
                                            disabledReason = item.disabledReason,
                                            modifier = Modifier.weight(1f), testTag = "${editor.id}-${item.id}",
                                            onSelect = { action(item.id) })
                                    }
                                }
                            }
                        }
                        Text(editor.notice, modifier = Modifier.padding(ThumbGap))
                        editor.detail?.let { Text(it, modifier = Modifier.padding(ThumbGap)) }
                    }
                }
            }
        }
    }
}
