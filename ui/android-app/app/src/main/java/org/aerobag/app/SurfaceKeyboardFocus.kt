// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app

import androidx.compose.foundation.focusable
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.runtime.withFrameNanos
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.focus.onFocusChanged

@Composable
internal fun Modifier.surfaceKeyboardFocus(
    resource: Any?,
    enabled: Boolean,
    requester: FocusRequester = remember { FocusRequester() },
): Modifier {
    var hasFocus by remember { mutableStateOf(false) }
    LaunchedEffect(resource, enabled) {
        if (enabled) {
            withFrameNanos { }
            // A tap can focus a descendant while this effect awaits its frame.
            // Background keyboard navigation must not undo that user choice.
            if (!hasFocus) requester.requestFocus()
        }
    }
    return focusRequester(requester).onFocusChanged { hasFocus = it.hasFocus }.focusable()
}
