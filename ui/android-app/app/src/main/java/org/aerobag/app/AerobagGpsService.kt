// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app

import android.Manifest
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.location.Location
import android.os.Build
import android.os.IBinder
import android.os.Looper
import android.util.Log
import androidx.core.app.NotificationCompat
import androidx.core.app.ServiceCompat
import androidx.core.content.ContextCompat
import com.google.android.gms.location.FusedLocationProviderClient
import com.google.android.gms.location.Granularity
import com.google.android.gms.location.LocationCallback
import com.google.android.gms.location.LocationRequest
import com.google.android.gms.location.LocationResult
import com.google.android.gms.location.LocationServices
import com.google.android.gms.location.Priority
import org.aerobag.app.domain.LatLonPoint
import org.aerobag.app.domain.OwnshipSourceKind
import org.aerobag.app.domain.OwnshipSourcePowerState
import org.aerobag.app.domain.SituationSample

class AerobagGpsService : Service() {
    private lateinit var fusedLocationClient: FusedLocationProviderClient
    private var finalStatusPublished = false

    private val locationCallback = object : LocationCallback() {
        override fun onLocationResult(result: LocationResult) {
            result.locations.forEach(::publishLocation)
        }
    }

    override fun onCreate() {
        super.onCreate()
        fusedLocationClient = LocationServices.getFusedLocationProviderClient(this)
        ensureNotificationChannel()
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        when (intent?.action) {
            ActionApplyPausedState -> {
                pauseFromCore()
                return START_NOT_STICKY
            }
        }

        if (AndroidGpsPower.isGpsPaused(this)) {
            publishFinalStatus(AndroidGpsSource.pausedStatus())
            stopSelf()
            return START_NOT_STICKY
        }

        if (!hasPreciseLocationPermission()) {
            publishFinalStatus(AndroidGpsSource.unavailableStatus("Precise location required"))
            stopSelf()
            return START_NOT_STICKY
        }

        AndroidGpsSource.publishStatus(AndroidGpsSource.searchingStatus())
        ServiceCompat.startForeground(
            this,
            NotificationId,
            buildActiveNotification(),
            android.content.pm.ServiceInfo.FOREGROUND_SERVICE_TYPE_LOCATION,
        )
        requestHighAccuracyUpdates()
        return START_STICKY
    }

    override fun onDestroy() {
        fusedLocationClient.removeLocationUpdates(locationCallback)
        if (!finalStatusPublished) {
            AndroidGpsSource.publishStatus(AndroidGpsSource.pausedStatus())
        }
        super.onDestroy()
    }

    override fun onBind(intent: Intent?): IBinder? = null

    private fun requestHighAccuracyUpdates() {
        fusedLocationClient.removeLocationUpdates(locationCallback)
        val request = LocationRequest.Builder(Priority.PRIORITY_HIGH_ACCURACY, UpdateIntervalMs)
            .setMinUpdateIntervalMillis(FastestUpdateIntervalMs)
            .setMinUpdateDistanceMeters(0f)
            .setMaxUpdateDelayMillis(0L)
            .setGranularity(Granularity.GRANULARITY_FINE)
            .build()

        try {
            fusedLocationClient.requestLocationUpdates(request, locationCallback, Looper.getMainLooper())
                .addOnFailureListener { error ->
                    Log.e(LogTag, "Failed to request GPS updates", error)
                    stopAfterTerminalFailure(AndroidGpsSource.failedStatus("GPS request failed"))
                }
        } catch (error: SecurityException) {
            Log.e(LogTag, "Location permission was revoked before GPS updates started", error)
            stopAfterTerminalFailure(AndroidGpsSource.unavailableStatus("Location permission required"))
        }
    }

    private fun stopAfterTerminalFailure(status: org.aerobag.app.domain.OwnshipSourceStatusUpdate) {
        fusedLocationClient.removeLocationUpdates(locationCallback)
        publishFinalStatus(status)
        ServiceCompat.stopForeground(this, ServiceCompat.STOP_FOREGROUND_REMOVE)
        stopSelf()
    }

    private fun pauseFromCore() {
        AndroidGpsPower.markGpsPaused(this)
        fusedLocationClient.removeLocationUpdates(locationCallback)
        publishFinalStatus(AndroidGpsSource.pausedStatus())
        ServiceCompat.stopForeground(this, ServiceCompat.STOP_FOREGROUND_REMOVE)
        postPausedNotification()
        stopSelf()
    }

    private fun publishFinalStatus(status: org.aerobag.app.domain.OwnshipSourceStatusUpdate) {
        finalStatusPublished = true
        AndroidGpsSource.publishStatus(status)
    }

    private fun publishLocation(location: Location) {
        val now = System.currentTimeMillis()
        val accuracyLabel = if (location.hasAccuracy()) {
            "GPS fix ${location.accuracy.toInt()} m"
        } else {
            "GPS fix"
        }
        AndroidGpsSource.publishStatus(AndroidGpsSource.connectedStatus(accuracyLabel))
        AndroidGpsSource.publishSample(
            SituationSample(
                sourceId = AndroidGpsSource.SourceId,
                sourceKind = OwnshipSourceKind.DeviceGps,
                eventTimeEpochMs = location.time.takeIf { it > 0L } ?: now,
                receivedTimeEpochMs = now,
                position = LatLonPoint(lat = location.latitude, lon = location.longitude),
                horizontalAccuracyM = location.horizontalAccuracyMIfPresent(),
                verticalAccuracyM = location.verticalAccuracyMIfPresent(),
                trackDegTrue = location.bearingIfPresent(),
                headingDegTrue = location.bearingIfPresent(),
                groundSpeedKt = location.speedKtIfPresent(),
                altitudeMslFt = location.altitudeFtIfPresent(),
                pressureAltitudeFt = null,
            ),
        )
    }

    private fun hasPreciseLocationPermission(): Boolean =
        ContextCompat.checkSelfPermission(this, Manifest.permission.ACCESS_FINE_LOCATION) == PackageManager.PERMISSION_GRANTED

    private fun ensureNotificationChannel() {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.O) return
        val manager = getSystemService(NotificationManager::class.java)
        val channel = NotificationChannel(
            NotificationChannelId,
            "GPS",
            NotificationManager.IMPORTANCE_LOW,
        )
        channel.description = "High accuracy location while Aerobag is running"
        manager.createNotificationChannel(channel)
    }

    private fun buildActiveNotification() =
        NotificationCompat.Builder(this, NotificationChannelId)
            .setSmallIcon(R.drawable.notification_aircraft)
            .setContentTitle("Aerobag GPS")
            .setContentText("High-precision GPS active")
            .setOngoing(true)
            .setOnlyAlertOnce(true)
            .addAction(
                R.drawable.notification_aircraft,
                "Pause GPS",
                activityIntent(AndroidGpsPower.PauseAction, 1),
            )
            .setContentIntent(
                PendingIntent.getActivity(
                    this,
                    0,
                    Intent(this, MainActivity::class.java),
                    PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
                ),
            )
            .build()

    private fun buildPausedNotification() =
        NotificationCompat.Builder(this, NotificationChannelId)
            .setSmallIcon(R.drawable.notification_aircraft)
            .setContentTitle("GPS paused")
            .setContentText("GPS remains selected. Tap Resume GPS to continue.")
            .setAutoCancel(true)
            .setOnlyAlertOnce(true)
            .addAction(
                R.drawable.notification_aircraft,
                "Resume GPS",
                activityIntent(AndroidGpsPower.ResumeAction, 2),
            )
            .setContentIntent(
                PendingIntent.getActivity(
                    this,
                    0,
                    Intent(this, MainActivity::class.java),
                    PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
                ),
            )
            .build()

    private fun activityIntent(action: String, requestCode: Int): PendingIntent =
        PendingIntent.getActivity(
            this,
            requestCode,
            Intent(this, MainActivity::class.java)
                .setAction(action)
                .addFlags(Intent.FLAG_ACTIVITY_CLEAR_TOP or Intent.FLAG_ACTIVITY_SINGLE_TOP),
            PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
        )

    private fun postPausedNotification() {
        if (!hasNotificationPermission()) return
        runCatching {
            getSystemService(NotificationManager::class.java)
                .notify(PausedNotificationId, buildPausedNotification())
        }.onFailure { error ->
            Log.w(LogTag, "Failed to post paused GPS notification", error)
        }
    }

    private fun hasNotificationPermission(): Boolean =
        Build.VERSION.SDK_INT < Build.VERSION_CODES.TIRAMISU ||
            ContextCompat.checkSelfPermission(this, Manifest.permission.POST_NOTIFICATIONS) == PackageManager.PERMISSION_GRANTED

    private fun Location.bearingIfPresent(): Double? =
        if (hasBearing()) bearing.toDouble() else null

    private fun Location.speedKtIfPresent(): Double? =
        if (hasSpeed()) speed.toDouble() * MetersPerSecondToKnots else null

    private fun Location.altitudeFtIfPresent(): Double? =
        if (hasAltitude()) altitude * MetersToFeet else null

    private fun Location.horizontalAccuracyMIfPresent(): Double? =
        if (hasAccuracy()) accuracy.toDouble() else null

    private fun Location.verticalAccuracyMIfPresent(): Double? =
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O && hasVerticalAccuracy()) {
            verticalAccuracyMeters.toDouble()
        } else {
            null
        }

    companion object {
        private const val LogTag = "AerobagGps"
        private const val NotificationChannelId = "aerobag_gps"
        private const val NotificationId = 1001
        private const val PausedNotificationId = 1002
        private const val ActionApplyPausedState = "org.aerobag.app.action.APPLY_PAUSED_GPS_STATE"
        private const val UpdateIntervalMs = 1_000L
        private const val FastestUpdateIntervalMs = 500L
        private const val MetersToFeet = 3.280839895
        private const val MetersPerSecondToKnots = 1.943844492

        fun startHighPrecisionGps(context: Context) {
            AndroidGpsPower.markGpsActive(context)
            AndroidGpsSource.publishStatus(AndroidGpsSource.searchingStatus())
            context.getSystemService(NotificationManager::class.java).cancel(PausedNotificationId)
            ContextCompat.startForegroundService(
                context,
                Intent(context, AerobagGpsService::class.java),
            )
        }

        fun applyCorePowerState(context: Context, powerState: OwnshipSourcePowerState) {
            when (powerState) {
                OwnshipSourcePowerState.Running -> startHighPrecisionGps(context)
                OwnshipSourcePowerState.Paused -> {
                    AndroidGpsPower.markGpsPaused(context)
                    AndroidGpsSource.publishStatus(AndroidGpsSource.pausedStatus())
                    context.startService(
                        Intent(context, AerobagGpsService::class.java).setAction(ActionApplyPausedState),
                    )
                }
                OwnshipSourcePowerState.Sleeping -> {
                    AndroidGpsPower.markGpsActive(context)
                    AndroidGpsSource.publishStatus(AndroidGpsSource.pausedStatus("Sleeping"))
                    context.getSystemService(NotificationManager::class.java).cancel(PausedNotificationId)
                    context.stopService(Intent(context, AerobagGpsService::class.java))
                }
            }
        }
    }
}
