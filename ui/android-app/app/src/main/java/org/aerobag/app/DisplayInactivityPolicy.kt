// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app

internal fun remainingDisplayInactivityMs(
    nowElapsedMs: Long,
    lastActivityElapsedMs: Long,
    timeoutMs: Long,
): Long {
    val elapsedMs = (nowElapsedMs - lastActivityElapsedMs).coerceAtLeast(0L)
    return (timeoutMs.coerceAtLeast(0L) - elapsedMs).coerceAtLeast(0L)
}

internal fun idleDisplayBrightnessOverride(
    currentBrightness: Float?,
    configuredIdleBrightness: Float,
): Float? {
    val current = currentBrightness?.takeIf { it.isFinite() && it >= 0.0f } ?: return null
    val target = configuredIdleBrightness
        .takeIf(Float::isFinite)
        ?.coerceIn(0.0f, 1.0f)
        ?: return null
    return target.takeIf { it < current }
}
