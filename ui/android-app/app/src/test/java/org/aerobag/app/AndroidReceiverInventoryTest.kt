// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app

import android.Manifest
import android.app.Application
import android.bluetooth.BluetoothAdapter
import android.bluetooth.BluetoothManager
import androidx.test.core.app.ApplicationProvider
import java.io.IOException
import java.util.concurrent.CancellationException
import org.aerobag.app.generated.ReceiverDevice
import org.aerobag.app.generated.ReceiverInventoryResult
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import org.robolectric.shadows.ShadowBluetoothAdapter

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [34], manifest = Config.NONE)
class AndroidReceiverInventoryTest {
    private val app = ApplicationProvider.getApplicationContext<Application>()

    @Test fun permissionAndBluetoothOffAreNotEmptySuccesses() {
        shadowOf(app).denyPermissions(Manifest.permission.BLUETOOTH_CONNECT)
        val transport = AndroidRfcomm(app)
        assertEquals(ReceiverInventoryResult.PermissionRequired, transport.inventory())
        shadowOf(app).grantPermissions(Manifest.permission.BLUETOOTH_CONNECT)
        val adapter = app.getSystemService(BluetoothManager::class.java).adapter
        shadowOf(adapter).setState(BluetoothAdapter.STATE_OFF)
        assertEquals(ReceiverInventoryResult.BluetoothOff, transport.inventory())
        shadowOf(adapter).setState(BluetoothAdapter.STATE_ON)
        assertEquals(ReceiverInventoryResult.Ready(emptyList()), transport.inventory())
    }

    @Test fun unavailableHardwareIsNotAnEmptyList() {
        shadowOf(app).grantPermissions(Manifest.permission.BLUETOOTH_CONNECT)
        ShadowBluetoothAdapter.setIsBluetoothSupported(false)
        assertEquals(ReceiverInventoryResult.Unavailable, AndroidRfcomm(app).inventory())
    }

    @Test fun bondedDevicesAreReturnedWithoutRequiringAConnection() {
        shadowOf(app).grantPermissions(Manifest.permission.BLUETOOTH_CONNECT)
        val adapter = app.getSystemService(BluetoothManager::class.java).adapter
        shadowOf(adapter).setState(BluetoothAdapter.STATE_ON)
        val device = adapter.getRemoteDevice("00:00:00:00:00:01")
        shadowOf(device).setName("GTX test")
        shadowOf(adapter).setBondedDevices(setOf(device))
        assertEquals(ReceiverInventoryResult.Ready(listOf(ReceiverDevice(device.address, "GTX test"))),
            AndroidRfcomm(app).inventory())
    }

    @Test fun osFailuresCrossTheBoundaryAsFailuresWithoutPrivateExceptionText() {
        assertEquals(ReceiverInventoryResult.PermissionRequired,
            readBluetoothInventory { throw SecurityException("permission changed during query") })
        assertEquals(ReceiverInventoryResult.Failed,
            readBluetoothInventory { throw IOException("private OS detail") })
    }

    @Test fun coroutineCancellationIsNotReportedAsAnInventoryFailure() {
        val cancelled = CancellationException("stopping")
        assertSame(cancelled, assertThrows(CancellationException::class.java) {
            readBluetoothInventory { throw cancelled }
        })
    }
}
