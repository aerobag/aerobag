// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app

/** Owns the right to publish one asynchronous chart-search inspection result. */
internal class ChartSearchInspectionGate {
    private var revision = 0L

    fun begin(): Long {
        revision += 1
        return revision
    }

    fun invalidate() {
        revision += 1
    }

    fun owns(token: Long): Boolean = token == revision
}
