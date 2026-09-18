// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app

import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import org.aerobag.app.domain.MapViewportState

/** Gesture/key callbacks may outlive the composition that installed them. */
@Composable
internal fun rememberMapFollowViewportSync(
    following: Boolean,
    widthPx: Float,
    heightPx: Float,
    sync: (MapViewportState, Double, Double) -> Unit,
): (MapViewportState) -> Unit {
    val latestSync by rememberUpdatedState<(MapViewportState) -> Unit> { viewport ->
        if (following && widthPx > 0f && heightPx > 0f) {
            sync(viewport, widthPx.toDouble(), heightPx.toDouble())
        }
    }
    // A local function reference can retain pre-layout dimensions (0 x 0).
    // Keep one callback whose inputs and command sink follow Compose state.
    return remember { { viewport -> latestSync(viewport) } }
}
