// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app

import android.content.Context
import android.content.Intent
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.receiveAsFlow
import org.aerobag.app.domain.SituationControlInput

internal object AndroidGpsPower {
    const val PauseAction = "org.aerobag.app.action.PAUSE_GPS"
    const val ResumeAction = "org.aerobag.app.action.RESUME_GPS"
    const val NotificationDismissedAction =
        "org.aerobag.app.action.GPS_NOTIFICATION_DISMISSED"

    private val controlRequestChannel = Channel<Unit>(Channel.CONFLATED)
    val controlRequests: Flow<Unit> = controlRequestChannel.receiveAsFlow()

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

    fun requestControl(context: Context, input: SituationControlInput) {
        prefs(context).edit()
            .putString(PendingControlKey, input.name)
            .apply()
        controlRequestChannel.trySend(Unit)
    }

    fun acknowledgeControl(context: Context, input: SituationControlInput) {
        if (pendingControl(context) != input) return
        prefs(context).edit()
            .remove(PendingControlKey)
            .apply()
    }

    fun pendingControl(context: Context): SituationControlInput? =
        prefs(context).getString(PendingControlKey, null)
            ?.let { value -> runCatching { SituationControlInput.valueOf(value) }.getOrNull() }

    fun controlInput(intent: Intent?): SituationControlInput? = controlInput(intent?.action)

    fun controlInput(action: String?): SituationControlInput? = when (action) {
        PauseAction -> SituationControlInput.Pause
        ResumeAction -> SituationControlInput.Resume
        NotificationDismissedAction -> SituationControlInput.Pause
        else -> null
    }

    private fun prefs(context: Context) =
        context.applicationContext.getSharedPreferences(UiPrefsName, Context.MODE_PRIVATE)

    private const val PausedKey = "android_gps_paused"
    private const val PendingControlKey = "pending_gps_power_control"
}
