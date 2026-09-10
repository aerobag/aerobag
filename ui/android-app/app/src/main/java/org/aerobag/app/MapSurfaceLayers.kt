// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxScope
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.zIndex

/**
 * Geographic overlays own ordering within the map, never above screen controls.
 * Separate parents contain an editor's local zIndex and full-surface pointer
 * handler. Declining a pointer in that handler does not pass it to a sibling
 * underneath it in Compose's hit-test chain.
 */
@Composable
internal fun MapSurfaceLayers(
    modifier: Modifier = Modifier,
    mapContent: @Composable BoxScope.() -> Unit,
    controls: @Composable BoxScope.() -> Unit,
) {
    Box(modifier) {
        Box(Modifier.matchParentSize(), content = mapContent)
        Box(Modifier.matchParentSize().zIndex(OverlayPlaneControls), content = controls)
    }
}
