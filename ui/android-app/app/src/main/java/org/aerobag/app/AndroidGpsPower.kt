// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app

import android.content.Context

// Must match DIRECT_SITUATION_SOURCE_ID in app-core's session source registration.
internal const val PlanPreviewOwnshipSourceId = "__direct_situation__"

internal object AndroidGpsPower {
    fun shouldRunHighPrecisionGpsForSource(sourceId: String): Boolean =
        sourceId == AndroidGpsSource.SourceId

    fun batterySavingFallbackSourceId(): String = PlanPreviewOwnshipSourceId

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

    fun setPendingOwnshipSource(context: Context, sourceId: String) {
        prefs(context).edit()
            .putString(PendingOwnshipSourceKey, sourceId)
            .apply()
    }

    fun clearPendingOwnshipSource(context: Context) {
        prefs(context).edit()
            .remove(PendingOwnshipSourceKey)
            .apply()
    }

    fun consumePendingOwnshipSource(context: Context): String? {
        val sharedPrefs = prefs(context)
        val sourceId = sharedPrefs.getString(PendingOwnshipSourceKey, null)?.takeIf { it.isNotBlank() }
        if (sourceId != null) {
            sharedPrefs.edit()
                .remove(PendingOwnshipSourceKey)
                .apply()
        }
        return sourceId
    }

    private fun prefs(context: Context) =
        context.applicationContext.getSharedPreferences(UiPrefsName, Context.MODE_PRIVATE)

    private const val PausedKey = "android_gps_paused"
    private const val PendingOwnshipSourceKey = "pending_ownship_source_id"
}
