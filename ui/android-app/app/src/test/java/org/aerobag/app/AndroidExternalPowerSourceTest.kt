// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app

import android.content.Context
import android.content.Intent
import android.os.BatteryManager
import android.os.Looper
import androidx.test.core.app.ApplicationProvider
import kotlinx.coroutines.cancelAndJoin
import kotlinx.coroutines.launch
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import kotlinx.coroutines.ExperimentalCoroutinesApi
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config

@OptIn(ExperimentalCoroutinesApi::class)
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [34], manifest = Config.NONE)
class AndroidExternalPowerSourceTest {
    private fun batteryIntent(plugged: Int, status: Int = BatteryManager.BATTERY_STATUS_CHARGING) =
        Intent(Intent.ACTION_BATTERY_CHANGED)
            .putExtra(BatteryManager.EXTRA_PLUGGED, plugged)
            .putExtra(BatteryManager.EXTRA_STATUS, status)

    @Test
    fun pluggedInDoesNotDependOnWhetherTheBatteryIsActivelyCharging() {
        for (plug in listOf(BatteryManager.BATTERY_PLUGGED_AC, BatteryManager.BATTERY_PLUGGED_USB,
            BatteryManager.BATTERY_PLUGGED_WIRELESS, BatteryManager.BATTERY_PLUGGED_DOCK)) {
            for (status in listOf(BatteryManager.BATTERY_STATUS_CHARGING,
                BatteryManager.BATTERY_STATUS_FULL, BatteryManager.BATTERY_STATUS_NOT_CHARGING)) {
                assertTrue(externalPowerConnected(batteryIntent(plug, status)))
            }
        }
        assertFalse(externalPowerConnected(batteryIntent(0, BatteryManager.BATTERY_STATUS_FULL)))
        assertFalse(externalPowerConnected(Intent(Intent.ACTION_BATTERY_CHANGED)))
        assertFalse(externalPowerConnected(null))
    }

    @Test
    fun observesInitialPowerAndChangesAndRefreshesAfterCollectorRestarts() = runTest {
        val context = ApplicationProvider.getApplicationContext<Context>()
        context.sendStickyBroadcast(batteryIntent(BatteryManager.BATTERY_PLUGGED_USB))
        val source = AndroidExternalPowerSource(context)
        val observed = mutableListOf<Boolean>()
        var job = launch { source.observations().collect { observed += it } }
        runCurrent()
        shadowOf(Looper.getMainLooper()).idle()
        runCurrent()
        assertEquals(listOf(true), observed)

        fun broadcast(plug: Int) {
            context.sendStickyBroadcast(batteryIntent(plug))
            shadowOf(Looper.getMainLooper()).idle()
        }
        broadcast(BatteryManager.BATTERY_PLUGGED_USB)
        runCurrent()
        assertEquals(listOf(true), observed)
        broadcast(0)
        runCurrent()
        assertEquals(listOf(true, false), observed)
        job.cancelAndJoin()
        broadcast(BatteryManager.BATTERY_PLUGGED_AC)
        runCurrent()
        assertEquals(listOf(true, false), observed)

        job = launch { source.observations().collect { observed += it } }
        runCurrent()
        shadowOf(Looper.getMainLooper()).idle()
        runCurrent()
        assertEquals(listOf(true, false, true), observed)
        job.cancelAndJoin()
    }
}
