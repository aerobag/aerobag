// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app

import android.content.Context
import android.location.Location
import android.util.Log
import androidx.annotation.WorkerThread
import androidx.core.location.LocationCompat
import androidx.core.location.altitude.AltitudeConverterCompat
import java.io.IOException
import org.aerobag.app.domain.LatLonPoint
import org.aerobag.app.domain.OwnshipSourceKind
import org.aerobag.app.domain.SituationSample

@WorkerThread
internal fun Location.toSituationSample(context: Context, nowEpochMs: Long): SituationSample {
    // Play Services can omit MSL even when the OS location cache has it. Use
    // AndroidX's bundled geoid model, never ellipsoid height or a fixed offset.
    if (!LocationCompat.hasMslAltitude(this) && hasAltitude() && altitude.isFinite() &&
        latitude in -90.0..90.0 && longitude in -180.0..180.0
    ) {
        try {
            AltitudeConverterCompat.addMslAltitudeToLocation(context, this)
        } catch (error: IOException) {
            // A failed model read must not discard the horizontal GPS fix.
            Log.w("AerobagGps", "GPS geoid model unavailable", error)
        }
    }
    val mslAltitudeM = if (LocationCompat.hasMslAltitude(this)) {
        LocationCompat.getMslAltitudeMeters(this)
    } else {
        null
    }
    return SituationSample(
        sourceId = AndroidGpsSource.SourceId,
        sourceKind = OwnshipSourceKind.DeviceGps,
        eventTimeEpochMs = time.takeIf { it > 0L } ?: nowEpochMs,
        receivedTimeEpochMs = nowEpochMs,
        position = LatLonPoint(lat = latitude, lon = longitude),
        horizontalAccuracyM = if (hasAccuracy()) accuracy.toDouble() else null,
        verticalAccuracyM = if (mslAltitudeM != null && LocationCompat.hasMslAltitudeAccuracy(this)) {
            LocationCompat.getMslAltitudeAccuracyMeters(this).toDouble()
        } else {
            null
        },
        trackDegTrue = if (hasBearing()) bearing.toDouble() else null,
        headingDegTrue = if (hasBearing()) bearing.toDouble() else null,
        groundSpeedKt = if (hasSpeed()) speed.toDouble() * MetersPerSecondToKnots else null,
        altitudeMslFt = mslAltitudeM?.times(MetersToFeet),
        pressureAltitudeFt = null,
    )
}

private const val MetersToFeet = 3.280839895
private const val MetersPerSecondToKnots = 1.943844492
