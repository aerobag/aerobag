// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app.domain

import kotlin.math.abs
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class ImageViewportTest {
    @Test
    fun `initial viewport fits and centers image`() {
        val viewport = createInitialImageViewport(
            imageWidthPx = 1200f,
            imageHeightPx = 800f,
            viewportWidthPx = 900f,
            viewportHeightPx = 700f,
        )
        assertEquals(1f, viewport.zoom)
        assertEquals(0f, viewport.leftPx, 0.001f)
        assertEquals(50f, viewport.topPx, 0.001f)
    }

    @Test
    fun `zoom around point preserves anchor`() {
        val start = createInitialImageViewport(
            imageWidthPx = 1200f,
            imageHeightPx = 800f,
            viewportWidthPx = 900f,
            viewportHeightPx = 700f,
        )
        val anchorX = 300f
        val anchorY = 250f
        val beforeLocalX = anchorX - start.leftPx
        val beforeLocalY = anchorY - start.topPx
        val next = zoomImageAroundPoint(
            state = start,
            anchorX = anchorX,
            anchorY = anchorY,
            nextZoom = 2f,
            imageWidthPx = 1200f,
            imageHeightPx = 800f,
            viewportWidthPx = 900f,
            viewportHeightPx = 700f,
            overscrollPx = 64f,
        )
        val afterLocalX = (anchorX - next.leftPx) / next.zoom
        val afterLocalY = (anchorY - next.topPx) / next.zoom
        assertTrue(abs(afterLocalX - beforeLocalX) < 0.01f)
        assertTrue(abs(afterLocalY - beforeLocalY) < 0.01f)
    }

    @Test
    fun `overscroll is limited to one thumb`() {
        val start = createInitialImageViewport(
            imageWidthPx = 1200f,
            imageHeightPx = 800f,
            viewportWidthPx = 900f,
            viewportHeightPx = 700f,
        )
        val dragged = dragImageViewport(
            state = start,
            dxPx = 600f,
            dyPx = 500f,
            imageWidthPx = 1200f,
            imageHeightPx = 800f,
            viewportWidthPx = 900f,
            viewportHeightPx = 700f,
            overscrollPx = 64f,
        )
        assertTrue(dragged.leftPx <= 64f)
        assertTrue(dragged.topPx <= 64f)
    }
}
