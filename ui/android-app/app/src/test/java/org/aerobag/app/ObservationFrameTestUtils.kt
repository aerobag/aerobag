// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app

import android.graphics.Bitmap
import android.graphics.Canvas
import android.view.View

/** Exercise the same pre-draw/content/overlay order as the Android window. */
internal fun View.drawObservationFrame() {
    viewTreeObserver.dispatchOnPreDraw()
    val bitmap = Bitmap.createBitmap(width.coerceAtLeast(1), height.coerceAtLeast(1), Bitmap.Config.ARGB_8888)
    try { draw(Canvas(bitmap)) } finally { bitmap.recycle() }
}

/** Include popup windows, which have their own Compose layout/draw lifetime. */
internal fun drawObservationWindows() {
    android.view.inspector.WindowInspector.getGlobalWindowViews().forEach { it.drawObservationFrame() }
}
