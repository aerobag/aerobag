// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app

import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.Intent
import android.content.pm.ServiceInfo
import android.os.IBinder
import android.os.PowerManager
import androidx.core.app.NotificationCompat
import androidx.core.app.ServiceCompat

/** Android process-lifetime plumbing only. No receiver or retry policy. */
class ReceiverCaptureService : Service() {
    private var wakeLock: PowerManager.WakeLock? = null
    override fun onBind(intent: Intent?): IBinder? = null
    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        val title = intent?.getStringExtra("title")
        val text = intent?.getStringExtra("text")
        if (title == null || text == null) { stopSelf(); return START_NOT_STICKY }
        val channel = "receiver-capture"
        getSystemService(NotificationManager::class.java).createNotificationChannel(
            NotificationChannel(channel, title, NotificationManager.IMPORTANCE_LOW))
        val launch = PendingIntent.getActivity(this, 0, Intent(this, MainActivity::class.java), PendingIntent.FLAG_IMMUTABLE)
        try {
            ServiceCompat.startForeground(this, 345,
                NotificationCompat.Builder(this, channel)
                    .setSmallIcon(R.mipmap.ic_launcher).setContentTitle(title).setContentText(text)
                    .setContentIntent(launch).setOngoing(true).build(),
                ServiceInfo.FOREGROUND_SERVICE_TYPE_CONNECTED_DEVICE)
            if (wakeLock == null) {
                wakeLock = getSystemService(PowerManager::class.java)
                    .newWakeLock(PowerManager.PARTIAL_WAKE_LOCK, "Aerobag:ReceiverCapture")
                    .apply { setReferenceCounted(false); acquire() }
            }
        } catch (_: Exception) {
            AndroidReceiverRuntime.get(this).hostStopped()
            stopSelf()
        }
        return START_NOT_STICKY
    }
    override fun onDestroy() {
        wakeLock?.let { if (it.isHeld) it.release() }
        wakeLock = null
        super.onDestroy()
    }
}
