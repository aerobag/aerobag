// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app

import android.location.Location
import androidx.core.location.LocationCompat
import org.aerobag.app.domain.LatLonPoint
import org.aerobag.app.domain.OwnshipSourceKind
import org.aerobag.app.domain.SituationSample

internal fun Location.toSituationSample(nowEpochMs: Long): SituationSample {
    // Location.altitude is WGS84 ellipsoid height, not MSL. The OS owns the
    // geoid correction; absent MSL data must not become a mislabeled altitude.
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
