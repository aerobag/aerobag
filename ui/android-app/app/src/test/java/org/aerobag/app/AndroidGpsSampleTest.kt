// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app

import android.location.Location
import androidx.core.location.LocationCompat
import org.aerobag.app.domain.LatLonPoint
import org.aerobag.app.domain.OwnshipSourceKind
import org.aerobag.app.domain.SituationSample
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [28, 34], manifest = Config.NONE)
class AndroidGpsSampleTest {
    @Test
    fun exportedAltitudeIsMslNotEllipsoidHeight() {
        // Pixel 8 readings that exposed the bug: 255 ft ellipsoid, 329 ft MSL.
        val sample = sample(ellipsoidM = 77.6, mslM = 100.34946, mslAccuracyM = 0.64f)
        assertEquals(329.2305, requireNotNull(sample.altitudeMslFt), 0.001)
        assertEquals(0.64, requireNotNull(sample.verticalAccuracyM), 0.0001)
        assertNull(sample.pressureAltitudeFt)
    }

    @Test
    fun geoidSeparationIsNotAConstantSeattleOffset() {
        val sample = sample(ellipsoidM = 130.0, mslM = 100.0)
        assertEquals(328.08399, requireNotNull(sample.altitudeMslFt), 0.001)
    }

    @Test
    fun ellipsoidOnlyFixKeepsPositionButDoesNotInventMslAltitude() {
        val sample = sample(ellipsoidM = 77.6, mslM = null, mslAccuracyM = 0.64f)
        assertNull(sample.altitudeMslFt)
        assertNull(sample.verticalAccuracyM)
        assertEquals(LatLonPoint(47.67845, -122.30912), sample.position)
    }

    @Test
    fun missingMslAccuracyDoesNotReuseEllipsoidAccuracy() {
        val sample = sample(mslM = 100.0)
        assertEquals(328.08399, requireNotNull(sample.altitudeMslFt), 0.001)
        assertNull(sample.verticalAccuracyM)
    }

    @Test
    fun zeroAndBelowSeaLevelAltitudesAreValid() {
        for (altitude in listOf(0.0, -100.0)) {
            val sample = sample(mslM = altitude)
            assertEquals(altitude / 0.3048, requireNotNull(sample.altitudeMslFt), 0.001)
        }
    }

    @Test
    fun suppliedMslDoesNotRequireEllipsoidAltitude() {
        val sample = sample(ellipsoidM = null, mslM = 100.0, mslAccuracyM = 2f)
        assertEquals(328.08399, requireNotNull(sample.altitudeMslFt), 0.001)
        assertEquals(2.0, requireNotNull(sample.verticalAccuracyM), 0.0)
    }

    @Test
    fun missingAltitudeDoesNotDiscardOtherNavigationMeasurements() {
        val sample = sample(ellipsoidM = null, mslM = null)
        assertNull(sample.altitudeMslFt)
        assertNull(sample.verticalAccuracyM)
        assertEquals(AndroidGpsSource.SourceId, sample.sourceId)
        assertEquals(OwnshipSourceKind.DeviceGps, sample.sourceKind)
        assertEquals(1_000L, sample.eventTimeEpochMs)
        assertEquals(1_100L, sample.receivedTimeEpochMs)
        assertEquals(LatLonPoint(47.67845, -122.30912), sample.position)
        assertEquals(5.0, requireNotNull(sample.horizontalAccuracyM), 0.0)
        assertEquals(42.0, requireNotNull(sample.trackDegTrue), 0.0)
        assertEquals(42.0, requireNotNull(sample.headingDegTrue), 0.0)
        assertEquals(19.43844, requireNotNull(sample.groundSpeedKt), 0.0001)
    }

    private fun sample(
        ellipsoidM: Double? = 77.6,
        mslM: Double?,
        mslAccuracyM: Float? = null,
    ): SituationSample {
        val location = Location("fused").apply {
            time = 1_000L
            latitude = 47.67845
            longitude = -122.30912
            accuracy = 5f
            bearing = 42f
            speed = 10f
            if (ellipsoidM != null) altitude = ellipsoidM
            verticalAccuracyMeters = 0.58f
            // API 34 has dedicated fields; older Android uses AndroidX extras.
            if (mslM != null) LocationCompat.setMslAltitudeMeters(this, mslM)
            if (mslAccuracyM != null) LocationCompat.setMslAltitudeAccuracyMeters(this, mslAccuracyM)
        }
        return location.toSituationSample(1_100L)
    }
}
