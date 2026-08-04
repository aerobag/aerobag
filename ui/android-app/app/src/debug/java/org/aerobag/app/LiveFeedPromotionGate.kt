// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app

import android.content.Context
import android.util.Log
import java.io.File
import kotlinx.coroutines.delay
import org.aerobag.app.domain.LiveFeedInstalledSummary

internal const val E2eLiveFeedPromotionPauseFile = "e2e-live-feed-promotion.pause"
internal const val E2eLiveFeedPromotionPauseMarker = "E2E live-feed promotion paused"

internal fun createInitialLiveFeedPromotionGate(context: Context): InitialLiveFeedPromotionGate {
    val sentinel = File(context.filesDir, E2eLiveFeedPromotionPauseFile)
    return InitialLiveFeedPromotionGate { installed: List<LiveFeedInstalledSummary> ->
        if (sentinel.exists() && installed.isNotEmpty()) {
            Log.i("AndroidLiveFeeds", E2eLiveFeedPromotionPauseMarker)
            while (sentinel.exists()) delay(50)
        }
    }
}
