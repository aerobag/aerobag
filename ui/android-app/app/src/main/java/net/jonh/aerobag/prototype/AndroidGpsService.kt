package net.jonh.aerobag.prototype

import android.Manifest
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
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
import net.jonh.aerobag.prototype.domain.LatLonPoint
import net.jonh.aerobag.prototype.domain.OwnshipSourceKind
import net.jonh.aerobag.prototype.domain.SituationSample

class AndroidGpsService : Service() {
    private lateinit var fusedLocationClient: FusedLocationProviderClient

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
        if (!hasPreciseLocationPermission()) {
            AndroidGpsSource.publishStatus(AndroidGpsSource.unavailableStatus("Precise location required"))
            stopSelf()
            return START_NOT_STICKY
        }

        AndroidGpsSource.publishStatus(AndroidGpsSource.searchingStatus())
        ServiceCompat.startForeground(
            this,
            NotificationId,
            buildNotification("Searching for GPS"),
            android.content.pm.ServiceInfo.FOREGROUND_SERVICE_TYPE_LOCATION,
        )
        requestHighAccuracyUpdates()
        return START_STICKY
    }

    override fun onDestroy() {
        fusedLocationClient.removeLocationUpdates(locationCallback)
        AndroidGpsSource.publishStatus(AndroidGpsSource.unavailableStatus("GPS stopped"))
        super.onDestroy()
    }

    override fun onBind(intent: Intent?): IBinder? = null

    private fun requestHighAccuracyUpdates() {
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
                    AndroidGpsSource.publishStatus(AndroidGpsSource.failedStatus("GPS request failed"))
                }
        } catch (error: SecurityException) {
            Log.e(LogTag, "Location permission was revoked before GPS updates started", error)
            AndroidGpsSource.publishStatus(AndroidGpsSource.unavailableStatus("Location permission required"))
            stopSelf()
        }
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

    private fun buildNotification(text: String) =
        NotificationCompat.Builder(this, NotificationChannelId)
            .setSmallIcon(R.drawable.plan_view_icon)
            .setContentTitle("Aerobag GPS")
            .setContentText(text)
            .setOngoing(true)
            .setContentIntent(
                PendingIntent.getActivity(
                    this,
                    0,
                    Intent(this, MainActivity::class.java),
                    PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
                ),
            )
            .build()

    private fun Location.bearingIfPresent(): Double? =
        if (hasBearing()) bearing.toDouble() else null

    private fun Location.speedKtIfPresent(): Double? =
        if (hasSpeed()) speed.toDouble() * MetersPerSecondToKnots else null

    private fun Location.altitudeFtIfPresent(): Double? =
        if (hasAltitude()) altitude * MetersToFeet else null

    private companion object {
        const val LogTag = "AerobagGps"
        const val NotificationChannelId = "aerobag_gps"
        const val NotificationId = 1001
        const val UpdateIntervalMs = 1_000L
        const val FastestUpdateIntervalMs = 500L
        const val MetersToFeet = 3.280839895
        const val MetersPerSecondToKnots = 1.943844492
    }
}
