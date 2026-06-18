package org.aerobag.app

import android.util.Log

internal const val VerbosePerfLogs = false

internal inline fun perfLogInfo(tag: String, message: () -> String) {
    if (VerbosePerfLogs) {
        Log.i(tag, message())
    }
}
