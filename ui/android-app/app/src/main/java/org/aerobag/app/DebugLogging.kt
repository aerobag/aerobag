// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app

import android.util.Log

internal const val VerbosePerfLogs = false
internal const val VerboseDiagnosticLogs = false

internal inline fun perfLogInfo(tag: String, message: () -> String) {
    // Preserve the gesture/follow handoff in E2E failure artifacts without
    // enabling expensive general performance logging in production.
    if (VerbosePerfLogs || (BuildConfig.AEROBAG_E2E_ENABLED && tag == MapViewportLogTag)) {
        Log.i(tag, message())
    }
}

internal inline fun diagnosticLogInfo(tag: String, message: () -> String) {
    if (VerboseDiagnosticLogs) {
        Log.i(tag, message())
    }
}
