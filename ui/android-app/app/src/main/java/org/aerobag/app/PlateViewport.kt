// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app

import androidx.compose.runtime.Composable
import androidx.compose.runtime.SideEffect
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.unit.IntSize
import org.aerobag.app.domain.ImageViewportState
import org.aerobag.app.domain.createInitialImageViewport

@Composable
internal fun rememberPlateViewport(
    bitmap: ImageBitmap?,
    surfaceSize: IntSize,
    viewport: ImageViewportState?,
    onViewportChange: (ImageViewportState?) -> Unit,
): ImageViewportState? {
    if (bitmap == null || surfaceSize.width <= 0 || surfaceSize.height <= 0) return null
    val current = viewport ?: createInitialImageViewport(
        imageWidthPx = bitmap.width.toFloat(),
        imageHeightPx = bitmap.height.toFloat(),
        viewportWidthPx = surfaceSize.width.toFloat(),
        viewportHeightPx = surfaceSize.height.toFloat(),
    )
    // Drawing and input use the default immediately. Persist it at this commit:
    // a keyed coroutine can miss initialization followed by reset in one frame
    // because its viewport key still compares null == null.
    SideEffect {
        if (viewport == null) onViewportChange(current)
    }
    return current
}
