// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.*
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import org.aerobag.app.generated.UiReceiverActivity

@OptIn(ExperimentalLayoutApi::class)
@Composable
internal fun ReceiverActivityView(state: UiReceiverActivity) {
    val theme = LocalAerobagUiTheme.current
    val palette = theme.receiverActivity
    Column(Modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(12.dp)) {
        Text(state.title, color = theme.controls.panelFg, fontWeight = FontWeight.Bold)
        state.strips.forEach { strip ->
            Column(Modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                FlowRow(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceBetween) {
                    Text(strip.title, color = theme.controls.panelFg, fontWeight = FontWeight.Bold)
                    Text(strip.scaleLabel, color = theme.controls.panelFg)
                }
                Canvas(Modifier.fillMaxWidth().height(88.dp).background(theme.controls.textInputBg)
                    .semantics { contentDescription = "${strip.title}: ${strip.scaleLabel}" }) {
                    val sx = size.width / 60000f
                    val sy = size.height / 1000f
                    strip.rects.forEach { rect ->
                        drawRect(palette.getValue(rect.colorKey), Offset(rect.x.toFloat() * sx, rect.y.toFloat() * sy),
                            Size(rect.width.toFloat() * sx, rect.height.toFloat() * sy))
                    }
                    drawLine(theme.controls.panelMuted, Offset(0f, size.height), Offset(size.width, size.height))
                }
                Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceBetween) {
                    Text(state.axisStart, color = theme.controls.panelFg)
                    Text(state.axisEnd, color = theme.controls.panelFg)
                }
                FlowRow(horizontalArrangement = Arrangement.spacedBy(16.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                    strip.legend.forEach { item ->
                        Row(verticalAlignment = androidx.compose.ui.Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                            Box(Modifier.size(10.dp).background(palette.getValue(item.colorKey)))
                            Text(item.text, color = theme.controls.panelFg)
                        }
                    }
                }
            }
        }
        Text(state.detail, color = theme.controls.panelMuted)
    }
}
