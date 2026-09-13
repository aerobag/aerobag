// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app

import android.content.Context
import android.hardware.Sensor
import android.hardware.SensorEvent
import android.hardware.SensorEventListener
import android.hardware.SensorManager
import android.os.SystemClock
import kotlinx.coroutines.channels.awaitClose
import kotlinx.coroutines.flow.callbackFlow
import kotlinx.coroutines.flow.conflate
import org.aerobag.app.generated.BarometerCommand

/** Transport only: pressure, capability and sample time. Core owns altitude and freshness. */
internal class AndroidBarometerSource(
    private val manager: SensorManager,
    private val now: () -> Long = System::currentTimeMillis,
    private val elapsedNanos: () -> Long = SystemClock::elapsedRealtimeNanos,
) {
    constructor(context: Context) : this(context.getSystemService(SensorManager::class.java))

    fun observations() = callbackFlow {
        val sensor = manager.getDefaultSensor(Sensor.TYPE_PRESSURE)
        val started = now()
        trySend(BarometerCommand.Observe(sensor != null, started, null, started))
        val listener = object : SensorEventListener {
            private var lastEmittedNanos: Long? = null

            override fun onSensorChanged(event: SensorEvent) {
                // Sampling periods are hints. Bound bridge traffic even on faster hardware.
                val last = lastEmittedNanos
                if (last != null && event.timestamp - last < 1_000_000_000L) return
                lastEmittedNanos = event.timestamp
                val received = now()
                val observed = received - ((elapsedNanos() - event.timestamp).coerceAtLeast(0L) / 1_000_000L)
                trySend(BarometerCommand.Observe(true, observed, event.values.firstOrNull()?.toDouble(), received))
            }

            override fun onAccuracyChanged(sensor: Sensor?, accuracy: Int) = Unit
        }
        if (sensor != null) {
            manager.registerListener(listener, sensor, 1_000_000)
        }
        awaitClose { manager.unregisterListener(listener) }
    }.conflate()
}
