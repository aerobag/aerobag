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
