// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.background
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxScope
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.runtime.Composable
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.ui.Modifier
import androidx.compose.ui.composed
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.semantics.onClick
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.zIndex

/** Inline in the map's input tree; unlike Popup it can share the map gesture owner. */
@Composable
internal fun MapInspectionOverlay(
    detailOpen: Boolean,
    onDismiss: () -> Unit,
    content: @Composable BoxScope.() -> Unit,
) {
    TourAwareInlineOverlay {
    BackHandler(onBack = onDismiss)
    Box(Modifier.fillMaxSize().zIndex(OverlayPlaneModal)) {
        if (detailOpen) {
            Scrim(onDismiss = onDismiss)
        } else {
            Box(Modifier.fillMaxSize().background(Color(0x3D0A1014))
                .semantics { onClick("Close map selection") { onDismiss(); true } }
                .pointerInput(Unit) {
                    // Win hit testing over underlying buttons without consuming
                    // events: the parent map owns the entire drag/pinch/tap.
                    awaitEachGesture { awaitFirstDown(requireUnconsumed = false) }
                })
        }
        content()
    }
    }
}

/** Children get first use (buttons/scroll); nothing inside this pane reaches the map. */
internal fun Modifier.inspectorInputBoundary(onTouching: (Boolean) -> Unit): Modifier = composed {
    val touching = rememberUpdatedState(onTouching)
    pointerInput(Unit) {
        awaitEachGesture {
            val down = awaitFirstDown(requireUnconsumed = false)
            down.consume()
            touching.value(true)
            try {
                do {
                    val event = awaitPointerEvent()
                } while (event.changes.any { it.pressed })
            } finally {
                touching.value(false)
            }
        }
    }
}
