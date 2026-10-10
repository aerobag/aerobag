// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app

import android.Manifest
import android.annotation.SuppressLint
import android.bluetooth.BluetoothManager
import android.bluetooth.BluetoothSocket
import android.content.Context
import android.content.pm.PackageManager
import android.os.Build
import androidx.core.content.ContextCompat
import java.io.Closeable
import java.io.IOException
import java.util.UUID
import java.util.concurrent.ArrayBlockingQueue
import java.util.concurrent.Executors
import java.util.concurrent.atomic.AtomicBoolean

// OS facts only. Device selection, service UUIDs, timing, retry, capture and
// aviation messages belong to core, not to this byte-stream adapter.
internal data class PairedBluetoothDevice(val address: String, val name: String?)

internal class AndroidRfcomm(private val context: Context) {
    fun hasPermission(): Boolean = Build.VERSION.SDK_INT < Build.VERSION_CODES.S ||
        ContextCompat.checkSelfPermission(context, Manifest.permission.BLUETOOTH_CONNECT) ==
        PackageManager.PERMISSION_GRANTED

    @SuppressLint("MissingPermission")
    fun pairedDevices(): List<PairedBluetoothDevice> {
        check(hasPermission()) { "Bluetooth permission is required" }
        val adapter = context.getSystemService(BluetoothManager::class.java)?.adapter ?: return emptyList()
        return adapter.bondedDevices.map { PairedBluetoothDevice(it.address, it.name) }
    }

    @SuppressLint("MissingPermission")
    fun socket(address: String, serviceUuid: String): BlockingDuplexStream {
        if (!hasPermission()) throw SecurityException("Bluetooth permission is required")
        val adapter = context.getSystemService(BluetoothManager::class.java)?.adapter
            ?: throw IOException("Bluetooth is unavailable")
        if (!adapter.isEnabled) throw IOException("Bluetooth is disabled")
        val device = adapter.bondedDevices.firstOrNull { it.address == address }
            ?: throw IOException("Bluetooth device is not paired")
        return BluetoothDuplexStream(device.createRfcommSocketToServiceRecord(UUID.fromString(serviceUuid)))
    }
}

/** close() must abort concurrent connect/read/write, as BluetoothSocket does. */
internal interface BlockingDuplexStream : Closeable {
    fun connect()
    fun read(buffer: ByteArray): Int
    fun write(bytes: ByteArray)
}

private class BluetoothDuplexStream(private val socket: BluetoothSocket) : BlockingDuplexStream {
    @SuppressLint("MissingPermission")
    override fun connect() = socket.connect()
    override fun read(buffer: ByteArray): Int = socket.inputStream.read(buffer)
    override fun write(bytes: ByteArray) = socket.outputStream.write(bytes)
    override fun close() = socket.close()
}

internal enum class StreamOperation { Connect, Read, Write }

internal interface DuplexStreamCallbacks {
    fun connected(connectionId: Long)
    // Owns bytes; a bounded consumer may block this IO thread for backpressure.
    fun received(connectionId: Long, bytes: ByteArray)
    fun written(connectionId: Long, writeId: Long)
    fun failed(connectionId: Long, operation: StreamOperation, permissionDenied: Boolean)
}

/**
 * One socket, at most one pending write, and two IO workers. No retry/sleep or
 * UI/session work here. All callbacks are tagged because cancellation can race
 * an already executing callback. Core must reject an obsolete connection ID.
 */
internal class DuplexStreamPump(
    private val connectionId: Long,
    private val stream: BlockingDuplexStream,
    private val callbacks: DuplexStreamCallbacks,
) : Closeable {
    private class PendingWrite(val id: Long, val bytes: ByteArray)
    private val closed = AtomicBoolean(false)
    private val started = AtomicBoolean(false)
    private val writePending = AtomicBoolean(false)
    private val writes = ArrayBlockingQueue<PendingWrite>(1)
    private val workers = Executors.newFixedThreadPool(2) { task ->
        Thread(task, "AerobagByteStream").apply { isDaemon = true }
    }

    @Synchronized
    fun start() {
        check(started.compareAndSet(false, true)) { "Stream already started" }
        if (closed.get()) return
        workers.execute {
            var operation = StreamOperation.Connect
            try {
                stream.connect()
                if (closed.get()) return@execute
                callbacks.connected(connectionId)
                operation = StreamOperation.Read
                val buffer = ByteArray(16 * 1024)
                while (!closed.get()) {
                    val count = stream.read(buffer)
                    if (count < 0) throw IOException("End of stream")
                    if (count == 0) throw IOException("Empty blocking read")
                    if (!closed.get()) callbacks.received(connectionId, buffer.copyOf(count))
                }
            } catch (error: Exception) {
                fail(operation, error)
            } finally {
                close()
            }
        }
        workers.execute {
            try {
                while (!closed.get()) {
                    val pending = writes.take()
                    stream.write(pending.bytes)
                    writePending.set(false)
                    if (!closed.get()) callbacks.written(connectionId, pending.id)
                }
            } catch (error: Exception) {
                fail(StreamOperation.Write, error)
            }
        }
    }

    fun write(writeId: Long, bytes: ByteArray) {
        if (closed.get()) return
        if (!writePending.compareAndSet(false, true)) {
            fail(StreamOperation.Write, IOException("Concurrent stream write"))
            return
        }
        // Core bounds messages; enforce the OS boundary too. Never log payloads.
        if (bytes.size > 65_535 || !writes.offer(PendingWrite(writeId, bytes.copyOf()))) {
            fail(StreamOperation.Write, IOException("Stream write limit"))
        }
    }

    private fun fail(operation: StreamOperation, error: Exception) {
        if (!closed.compareAndSet(false, true)) return
        runCatching { stream.close() }
        // A consumer failure must release the socket too, not escape an IO
        // worker and become an Android process-level uncaught exception.
        // shutdownNow interrupts this worker too; notify BEFORE that interrupt.
        try {
            callbacks.failed(connectionId, operation, error is SecurityException)
        } catch (_: Exception) {
            // The consumer boundary failed. Still release both workers below.
        } finally {
            shutdown()
        }
    }

    override fun close() {
        if (closed.compareAndSet(false, true)) shutdown()
    }

    @Synchronized
    private fun shutdown() {
        runCatching { stream.close() }
        writes.clear()
        workers.shutdownNow()
    }
}
