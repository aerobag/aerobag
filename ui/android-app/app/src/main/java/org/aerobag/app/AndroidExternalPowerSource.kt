// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.os.BatteryManager
import androidx.core.content.ContextCompat
import kotlinx.coroutines.channels.awaitClose
import kotlinx.coroutines.flow.callbackFlow
import kotlinx.coroutines.flow.conflate
import kotlinx.coroutines.flow.distinctUntilChanged

/** Reports OS power state only; core decides whether to dim. */
internal class AndroidExternalPowerSource(private val context: Context) {
    fun observations() = callbackFlow {
        val receiver = object : BroadcastReceiver() {
            override fun onReceive(context: Context, intent: Intent) {
                if (intent.action == Intent.ACTION_BATTERY_CHANGED) {
                    trySend(externalPowerConnected(intent))
                }
            }
        }
        val initial = ContextCompat.registerReceiver(
            context,
            receiver,
            IntentFilter(Intent.ACTION_BATTERY_CHANGED),
            ContextCompat.RECEIVER_NOT_EXPORTED,
        )
        trySend(externalPowerConnected(initial))
        awaitClose { context.unregisterReceiver(receiver) }
    }.conflate().distinctUntilChanged()
}

internal fun externalPowerConnected(intent: Intent?): Boolean =
    // Full batteries (and charging limits) can stop charging while still plugged in.
    (intent?.getIntExtra(BatteryManager.EXTRA_PLUGGED, -1) ?: -1) > 0
