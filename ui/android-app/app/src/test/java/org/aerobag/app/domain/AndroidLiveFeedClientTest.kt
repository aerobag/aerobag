// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app.domain

import androidx.test.core.app.ApplicationProvider
import java.lang.reflect.Proxy
import java.net.ServerSocket
import java.net.Socket
import java.util.Collections
import java.util.concurrent.CountDownLatch
import java.util.concurrent.Executors
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.atomic.AtomicInteger
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.cancelAndJoin
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.Json
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [34])
class AndroidLiveFeedClientTest {
    @Test fun bulletinDeliveryDoesNotWaitForAProductDownload() = runBlocking {
        val server = ServerSocket(0)
        val executor = Executors.newCachedThreadPool()
        val sockets = Collections.synchronizedList(mutableListOf<Socket>())
        val root = "http://127.0.0.1:${server.localPort}"
        val downloading = CompletableDeferred<Unit>()
        val bulletin = CompletableDeferred<LiveFeedSseEvent>()
        val releaseDownload = CountDownLatch(1)
        val releaseStream = CountDownLatch(1)
        val productMissing = AtomicBoolean(false)
        val pendingSignalsDrained = CompletableDeferred<Unit>()
        val emptyReads = AtomicInteger()
        val downloads = AtomicInteger()
        executor.submit {
            while (!server.isClosed) {
                val socket = server.accept()
                sockets.add(socket)
                executor.submit {
                    socket.use {
                        val reader = it.getInputStream().bufferedReader()
                        val path = reader.readLine().split(' ')[1]
                        while (!reader.readLine().isNullOrEmpty()) { }
                        val body = it.getOutputStream()
                        if (path == "/events") {
                            body.write(("HTTP/1.0 200 OK\r\nContent-Type: text/event-stream\r\n\r\n" +
                                "event: live-feed-catalog\ndata: {}\n\n").toByteArray())
                            body.flush()
                            runBlocking { withTimeout(3_000) { downloading.await() } }
                            repeat(50) { body.write("event: live-feed-catalog\ndata: {}\n\n".toByteArray()) }
                            body.write("event: service-bulletins\ndata: notice\n\n".toByteArray())
                            body.flush()
                            releaseStream.await()
                        } else {
                            check(path == "/product")
                            downloads.incrementAndGet()
                            downloading.complete(Unit)
                            releaseDownload.await()
                            body.write("HTTP/1.0 200 OK\r\nContent-Length: 1\r\n\r\nx".toByteArray())
                            body.flush()
                        }
                    }
                }
            }
        }
        val json = Json { encodeDefaults = true }
        val policy = SseTransportPolicy(15_000, 3_000, 65_000, 1_000, 65_000)
        val bridge = Proxy.newProxyInstance(NativeBridge::class.java.classLoader, arrayOf(NativeBridge::class.java)) { _, method, args ->
            when (method.name) {
                "createLiveFeedCache" -> 1L
                "liveFeedEventsUrl" -> "$root/events"
                "liveFeedStatusUrl" -> "$root/status"
                "liveFeedCacheRuntimeDecisionJson" -> json.encodeToString(LiveFeedRuntimeDecision(transportPolicy = policy, commands = emptyList()))
                "liveFeedCacheIngestSseEventJson" -> {
                    val event = json.decodeFromString<LiveFeedSseEvent>(args!![1] as String)
                    if (event.event == "live-feed-catalog") {
                        productMissing.set(true)
                        json.encodeToString(LiveFeedCacheSseOutcome(true, emptyList()))
                    } else json.encodeToString(LiveFeedCacheSseOutcome(false, listOf(event)))
                }
                "liveFeedCacheMissingRequestsAtEpochMsJson" -> if (productMissing.get()) {
                    """[{"id":"product","url":"$root/product","kind":{"kind":"full","product":"notams"}}]"""
                } else {
                    if (emptyReads.incrementAndGet() == 2) pendingSignalsDrained.complete(Unit)
                    "[]"
                }
                "liveFeedCacheInstallFetchedBytesJson" -> { productMissing.set(false); "null" }
                "destroyLiveFeedCache" -> Unit
                else -> error("Unexpected bridge operation: ${method.name}")
            }
        } as NativeBridge
        val cache = LiveFeedCache(root, bridge = bridge)
        val client = AndroidLiveFeedClient(ApplicationProvider.getApplicationContext(), cache, root)
        val job = launch(Dispatchers.Default) {
            client.bootstrapAndRun(promote = {}, onChanged = {}, onSessionEvents = { bulletin.complete(it.single()) })
        }
        try {
            withTimeout(3_000) { downloading.await() }
            // The actual HTTP product request remains blocked. The real client
            // must keep reading its socket and deliver the subsequent notice.
            assertEquals("notice", withTimeout(3_000) { bulletin.await() }.data)
            releaseDownload.countDown()
            withTimeout(3_000) { pendingSignalsDrained.await() }
            assertEquals("one acquisition owner, not a worker per event", 1, downloads.get())
            assertEquals("fifty wakeups collapse to one pending pump", 2, emptyReads.get())
        } finally {
            releaseDownload.countDown()
            releaseStream.countDown()
            job.cancel()
            server.close()
            synchronized(sockets) { sockets.forEach { it.close() } }
            job.cancelAndJoin()
            executor.shutdownNow()
            cache.close()
        }
    }
}
