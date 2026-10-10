// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app

import android.content.Context
import android.content.Intent
import android.os.SystemClock
import android.util.Base64
import android.util.Log
import androidx.core.content.ContextCompat
import java.io.File
import java.util.concurrent.Executors
import kotlinx.coroutines.*
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.channels.ClosedSendChannelException
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.Json
import org.aerobag.app.domain.NativeBindings
import org.aerobag.app.generated.*

/** Process lifetime, not activity lifetime. Only OS operations and generated wire dispatch here. */
internal class AndroidReceiverRuntime private constructor(private val context: Context) {
    private val dispatcher = Executors.newSingleThreadExecutor { task ->
        Thread(task, "AerobagReceiver").apply { isDaemon = true }
    }.asCoroutineDispatcher()
    private val scope = CoroutineScope(SupervisorJob() + dispatcher)
    private data class Input(val event: ReceiverHostEvent, val bytes: ByteArray = byteArrayOf())
    private val inputs = ClosingInputChannel<Input>(32)
    private val rfcomm = AndroidRfcomm(context)
    private val sockets = mutableMapOf<Long, DuplexStreamPump>()
    private var wake: Job? = null
    private var keepingAlive = false
    private var foregroundSummary: Pair<String, String>? = null
    val permissionRequests = MutableSharedFlow<Unit>(extraBufferCapacity = 1)
    val fileRequests = MutableSharedFlow<Unit>(extraBufferCapacity = 1)
    val shareRequests = MutableSharedFlow<ReceiverHostEffect.ShareFile>(extraBufferCapacity = 1)
    @Volatile private var pendingFile: ReceiverHostEffect.ReadPrivateFile? = null
    val deliveries = MutableSharedFlow<Unit>(replay = 1, extraBufferCapacity = 1)

    init {
        scope.launch {
            try {
                NativeBindings.initializeReceiver(File(context.filesDir, "receiver").absolutePath)
                inputs.send(Input(ReceiverHostEvent.RefreshInventory))
                for (input in inputs) {
                    // Sample clocks here, not on callback producer threads: enqueue order can differ.
                    val output = Json.decodeFromString<ReceiverHostOutput>(NativeBindings.receiverHostEventJson(
                        Json.encodeToString(input.event), input.bytes,
                        SystemClock.elapsedRealtime(), System.currentTimeMillis(),
                    ))
                    wake?.cancel()
                    val summary = output.panel.title to output.panel.status
                    if (output.keepAlive != keepingAlive || (output.keepAlive && foregroundSummary != summary)) {
                        val starting = !keepingAlive
                        keepingAlive = output.keepAlive
                        foregroundSummary = summary
                        try {
                            if (keepingAlive) {
                                val intent = Intent(context, ReceiverCaptureService::class.java)
                                    .putExtra("title", output.panel.title).putExtra("text", output.panel.status)
                                if (starting) ContextCompat.startForegroundService(context, intent) else context.startService(intent)
                            } else context.stopService(Intent(context, ReceiverCaptureService::class.java))
                        } catch (_: Exception) {
                            send(ReceiverHostEvent.HostUnavailable)
                        }
                    }
                    for (effect in output.effects) execute(effect)
                    if (output.deliveryReady) deliveries.tryEmit(Unit)
                    output.nextWakeMonotonicMs?.let { deadline ->
                        wake = scope.launch {
                            delay((deadline - SystemClock.elapsedRealtime()).coerceAtLeast(0))
                            inputs.send(Input(ReceiverHostEvent.Tick))
                        }
                    }
                }
            } catch (error: CancellationException) {
                throw error
            } catch (error: Exception) {
                // A serialization error may include private wire bytes in its message.
                Log.e("AerobagReceiver", "Receiver worker failed (${error.javaClass.simpleName}); closing sockets")
                // Do not keep a notification claiming to capture after the worker has failed.
                sockets.values.forEach { it.close() }
                sockets.clear()
                inputs.close()
                wake?.cancel()
                runCatching {
                    NativeBindings.receiverHostEventJson(Json.encodeToString<ReceiverHostEvent>(ReceiverHostEvent.HostFailed),
                        byteArrayOf(), SystemClock.elapsedRealtime(), System.currentTimeMillis())
                    deliveries.tryEmit(Unit)
                }
                runCatching { context.stopService(Intent(context, ReceiverCaptureService::class.java)) }
            }
        }
    }

    fun action(actionId: String) = send(ReceiverHostEvent.Action(actionId))
    fun refreshInventory() = send(ReceiverHostEvent.RefreshInventory)
    fun hostStopped() = send(ReceiverHostEvent.HostUnavailable)

    fun fileSelected(uri: android.net.Uri?) {
        val request = pendingFile ?: return
        pendingFile = null
        scope.launch {
            val bytes = withContext(Dispatchers.IO) {
                runCatching {
                    uri?.let { context.contentResolver.openInputStream(it)?.use { stream ->
                        readResourceBytes(stream, request.maxBytes.toLong() + 1)
                    } }
                }.getOrNull()
            }
            inputs.send(Input(ReceiverHostEvent.FileRead(requestId = request.requestId, succeeded = bytes != null), bytes ?: byteArrayOf()))
        }
    }

    private fun send(event: ReceiverHostEvent) {
        scope.launch { inputs.send(Input(event)) }
    }

    private fun execute(effect: ReceiverHostEffect) {
        when (effect) {
            is ReceiverHostEffect.ShareFile -> shareRequests.tryEmit(effect)
            is ReceiverHostEffect.ReadPrivateFile -> { pendingFile = effect; fileRequests.tryEmit(Unit) }
            ReceiverHostEffect.RequestPermission -> permissionRequests.tryEmit(Unit)
            is ReceiverHostEffect.RefreshInventory -> scope.launch {
                // Binder work must not block core's progress publication or refresh deadline.
                val result = withContext(Dispatchers.IO) { rfcomm.inventory() }
                send(ReceiverHostEvent.Inventory(requestId = effect.requestId, result = result))
            }
            is ReceiverHostEffect.Close -> sockets.remove(effect.connectionId)?.close()
            is ReceiverHostEffect.Connect -> {
                try {
                    val stream = rfcomm.socket(effect.device, effect.serviceUuid)
                    val callbacks = object : DuplexStreamCallbacks {
                        private fun deliver(input: Input) {
                            // Backpressure stays on socket workers, never UI or session workers.
                            runBlocking { inputs.send(input) }
                        }
                        override fun connected(connectionId: Long) = deliver(Input(ReceiverHostEvent.Connected(connectionId)))
                        override fun received(connectionId: Long, bytes: ByteArray) = deliver(Input(ReceiverHostEvent.Received(connectionId), bytes))
                        override fun written(connectionId: Long, writeId: Long) = deliver(Input(ReceiverHostEvent.Written(connectionId, writeId)))
                        override fun failed(connectionId: Long, operation: StreamOperation, permissionDenied: Boolean) {
                            // write() can reject synchronously on this actor; never block it on itself.
                            send(ReceiverHostEvent.Failed(connectionId, if (permissionDenied) ReceiverIoFailure.Permission else when (operation) {
                                StreamOperation.Connect -> ReceiverIoFailure.Connection
                                StreamOperation.Read -> ReceiverIoFailure.Read
                                StreamOperation.Write -> ReceiverIoFailure.Write
                            }))
                        }
                    }
                    sockets[effect.connectionId] = DuplexStreamPump(effect.connectionId, stream, callbacks).also { it.start() }
                } catch (error: Exception) {
                    send(ReceiverHostEvent.Failed(effect.connectionId,
                        if (error is SecurityException) ReceiverIoFailure.Permission else ReceiverIoFailure.Connection))
                }
            }
            is ReceiverHostEffect.Write -> sockets[effect.connectionId]?.write(effect.writeId, Base64.decode(effect.bytesBase64, Base64.DEFAULT))
        }
    }

    companion object {
        @Volatile private var instance: AndroidReceiverRuntime? = null
        fun get(context: Context): AndroidReceiverRuntime = instance ?: synchronized(this) {
            instance ?: AndroidReceiverRuntime(context.applicationContext).also { instance = it }
        }
    }
}

/** One outstanding session delivery. Recording continues if projection/resource paging stalls. */
internal class ReceiverSessionPublisher(runtime: AndroidReceiverRuntime, runner: UiSessionWorkRunner) : AutoCloseable {
    // Pure core ID allocation, no locks or IO. Order is construction, not coroutine scheduling.
    private val consumer = NativeBindings.newReceiverConsumer()
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
    init {
        scope.launch {
            var attached = false
            runtime.deliveries.collect {
                while (isActive) {
                    try {
                        if (!attached) {
                            NativeBindings.attachReceiverSession(consumer)
                            attached = true
                        }
                        val id = NativeBindings.beginReceiverDelivery(consumer)
                        if (id == 0L) break
                        runner.applyReceiverDelivery(id)
                        NativeBindings.acknowledgeReceiverDelivery(id)
                    } catch (error: CancellationException) { throw error }
                    catch (error: Exception) {
                        Log.w("AerobagReceiver", "Receiver UI delivery retained for retry", error)
                        break
                    }
                }
            }
        }
    }
    override fun close() { scope.cancel() }
}

/** Closing an IO owner is normal, including while producers are backpressured. */
internal class ClosingInputChannel<T>(capacity: Int) {
    private val channel = Channel<T>(capacity)
    @Volatile private var closed = false
    suspend fun send(value: T): Boolean {
        return try { channel.send(value); true }
        catch (_: ClosedSendChannelException) { false }
        catch (error: CancellationException) {
            currentCoroutineContext().ensureActive()
            if (!closed) throw error
            false
        }
    }
    operator fun iterator() = channel.iterator()
    fun close() { closed = true; channel.cancel() }
}
