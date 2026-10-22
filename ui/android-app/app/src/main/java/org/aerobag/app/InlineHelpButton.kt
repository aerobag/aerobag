// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later
package org.aerobag.app

import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp

@Composable
internal fun InlineHelpButton(text: String, tag: String, foreground: Color) {
    val context = LocalContext.current
    Box(Modifier.size(26.dp).clip(CircleShape).border(1.dp, foreground, CircleShape)
        .clickable(interactionSource = remember { MutableInteractionSource() }, indication = null, role = Role.Button) {
            showActionToast(context, text, long = true)
        }.e2eIndexedControl(semanticTag = tag, enabled = true, text = text), contentAlignment = Alignment.Center) {
        Text("?", style = MaterialTheme.typography.labelMedium, fontWeight = FontWeight.ExtraBold, color = foreground)
    }
}
