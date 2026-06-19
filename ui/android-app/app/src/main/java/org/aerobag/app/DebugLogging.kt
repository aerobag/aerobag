package org.aerobag.app

import android.util.Log

internal const val VerbosePerfLogs = false
internal const val VerboseDiagnosticLogs = false

internal inline fun perfLogInfo(tag: String, message: () -> String) {
    if (VerbosePerfLogs) {
        Log.i(tag, message())
    }
}

internal inline fun diagnosticLogInfo(tag: String, message: () -> String) {
    if (VerboseDiagnosticLogs) {
        Log.i(tag, message())
    }
}
