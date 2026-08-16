// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app

import android.content.Context
import android.content.Intent
import org.aerobag.app.domain.SituationControlInput

internal object AndroidGpsPower {
    const val PauseAction = "org.aerobag.app.action.PAUSE_GPS"
    const val ResumeAction = "org.aerobag.app.action.RESUME_GPS"

    fun isGpsPaused(context: Context): Boolean =
        prefs(context).getBoolean(PausedKey, false)

    fun markGpsPaused(context: Context) {
        prefs(context).edit()
            .putBoolean(PausedKey, true)
            .apply()
    }

    fun markGpsActive(context: Context) {
        prefs(context).edit()
            .putBoolean(PausedKey, false)
            .apply()
    }

    fun setPendingControl(context: Context, input: SituationControlInput) {
        prefs(context).edit()
            .putString(PendingControlKey, input.name)
            .apply()
    }

    fun clearPendingControl(context: Context) {
        prefs(context).edit()
            .remove(PendingControlKey)
            .apply()
    }

    fun consumePendingControl(context: Context): SituationControlInput? {
        val sharedPrefs = prefs(context)
        val input = sharedPrefs.getString(PendingControlKey, null)
            ?.let { value -> runCatching { SituationControlInput.valueOf(value) }.getOrNull() }
        if (input != null) {
            sharedPrefs.edit()
                .remove(PendingControlKey)
                .apply()
        }
        return input
    }

    fun controlInput(intent: Intent?): SituationControlInput? = when (intent?.action) {
        PauseAction -> SituationControlInput.Pause
        ResumeAction -> SituationControlInput.Resume
        else -> null
    }

    private fun prefs(context: Context) =
        context.applicationContext.getSharedPreferences(UiPrefsName, Context.MODE_PRIVATE)

    private const val PausedKey = "android_gps_paused"
    private const val PendingControlKey = "pending_gps_power_control"
}
