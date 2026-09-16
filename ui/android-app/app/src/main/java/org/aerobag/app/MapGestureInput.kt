// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app

import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.gestures.calculateCentroidSize
import androidx.compose.foundation.gestures.calculatePan
import androidx.compose.foundation.gestures.calculateZoom
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.ui.Modifier
import androidx.compose.ui.composed
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.input.pointer.pointerInput
import kotlin.math.abs

/** One gesture owner for the map and an inspector's non-consuming backdrop. */
internal fun Modifier.mapGestureInput(
    enabled: Boolean,
    onTap: (Offset) -> Unit,
    onGesture: () -> Unit,
    onTransform: (previous: List<Offset>, current: List<Offset>) -> Unit,
    onActiveChange: (Boolean) -> Unit,
    onFinished: () -> Unit,
): Modifier = composed {
    val currentEnabled = rememberUpdatedState(enabled)
    val tap = rememberUpdatedState(onTap)
    val gesture = rememberUpdatedState(onGesture)
    val transform = rememberUpdatedState(onTransform)
    val active = rememberUpdatedState(onActiveChange)
    val finished = rememberUpdatedState(onFinished)
    // Changing selection or its dismissal must not cancel a captured gesture.
    pointerInput(Unit) {
        awaitEachGesture {
            val down = awaitFirstDown(requireUnconsumed = false)
            if (down.isConsumed || !currentEnabled.value) return@awaitEachGesture
            var moved = false
            var cancelled = false
            var multiplePointers = false
            var totalPan = Offset.Zero
            var totalZoom = 1f
            active.value(true)
            try {
                do {
                    val event = awaitPointerEvent()
                    if (event.changes.any { it.isConsumed }) {
                        cancelled = true
                        break
                    }
                    multiplePointers = multiplePointers || event.changes.count { it.pressed } > 1
                    val pan = event.calculatePan()
                    val zoom = event.calculateZoom()
                    totalPan += pan
                    totalZoom *= zoom
                    val crossedSlop = totalPan.getDistance() > viewConfiguration.touchSlop ||
                        abs(1 - totalZoom) * event.calculateCentroidSize(useCurrent = false) > viewConfiguration.touchSlop
                    if (!moved && crossedSlop) {
                        moved = true
                        gesture.value()
                    }
                    if (moved && (pan != Offset.Zero || zoom != 1f)) {
                        val moving = event.changes.filter { it.previousPressed }
                        transform.value(moving.map { it.previousPosition }, moving.map { it.position })
                    }
                    event.changes.forEach { it.consume() }
                } while (event.changes.any { it.pressed })
                if (!moved && !cancelled && !multiplePointers) tap.value(down.position)
            } finally {
                if (moved) finished.value()
                active.value(false)
            }
        }
    }
}
