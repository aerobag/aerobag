// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later
package org.aerobag.app

import android.content.Context
import android.hardware.Sensor
import android.hardware.SensorManager
import androidx.test.core.app.ApplicationProvider
import kotlinx.coroutines.cancelAndJoin
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import org.aerobag.app.generated.BarometerCommand
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import org.robolectric.shadows.ShadowSensor

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [34], manifest = Config.NONE)
class AndroidBarometerSourceTest {
    private fun manager() = ApplicationProvider.getApplicationContext<Context>().getSystemService(SensorManager::class.java)

    @Test fun sensorlessDevicesReportUnsupportedWithoutRegisteringAnything() = runBlocking {
        withTimeout(2_000) {
            val manager = manager()
            val events = Channel<BarometerCommand.Observe>(Channel.UNLIMITED)
            val job = launch { AndroidBarometerSource(manager).observations().collect { events.send(it) } }
            assertFalse(events.receive().available)
            assertTrue(shadowOf(manager).listeners.isEmpty())
            job.cancelAndJoin()
        }
    }

    @Test fun rawPressurePreservesAcquisitionTimeAndListenerStopsWithCollector() = runBlocking {
        withTimeout(2_000) {
            val manager = manager()
            val shadow = shadowOf(manager)
            shadow.addSensor(ShadowSensor.newInstance(Sensor.TYPE_PRESSURE))
            val events = Channel<BarometerCommand.Observe>(Channel.UNLIMITED)
            val job = launch {
                AndroidBarometerSource(manager, now = { 10_000L }, elapsedNanos = { 2_000_000_000L })
                    .observations().collect { events.send(it) }
            }
            assertTrue(events.receive().available)
            assertEquals(1, shadow.listeners.size)
            val event = org.robolectric.util.ReflectionHelpers.callConstructor(android.hardware.SensorEvent::class.java,
                org.robolectric.util.ReflectionHelpers.ClassParameter.from(Int::class.javaPrimitiveType, 1))
            event.sensor = manager.getDefaultSensor(Sensor.TYPE_PRESSURE)
            event.timestamp = 1_500_000_000L
            event.values[0] = 1002.64f
            shadow.sendSensorEventToListeners(event)
            val observation = events.receive()
            assertEquals(1002.64, observation.pressureHpa!!, 0.001)
            assertEquals(9_500L, observation.observedEpochMs)
            assertEquals(10_000L, observation.receivedEpochMs)
            job.cancelAndJoin()
            assertTrue(shadow.listeners.isEmpty())
        }
    }
}
