// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app.domain

import android.content.Context
import android.net.ConnectivityManager
import android.net.Network
import android.net.NetworkCapabilities
import android.os.SystemClock
import android.util.Log
import java.io.ByteArrayOutputStream
import java.io.File
import java.io.IOException
import java.io.InputStream
import java.net.HttpURLConnection
import java.net.SocketTimeoutException
import java.net.URL
import java.nio.file.Files
import java.nio.file.StandardCopyOption.ATOMIC_MOVE
import java.nio.file.StandardCopyOption.REPLACE_EXISTING
import java.util.concurrent.TimeUnit
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Job
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.delay
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext
import kotlinx.coroutines.Dispatchers
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.decodeFromString
import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import okhttp3.OkHttpClient
import okhttp3.Request
import org.aerobag.app.diagnosticLogInfo

private const val LiveFeedCacheDirectoryName = "live-feeds"
private const val LiveFeedSseConnectTimeoutMs = 5_000
private const val LiveFeedSseIdleTimeoutMs = 65_000
private const val LiveFeedLogTag = "AndroidLiveFeeds"
internal const val LiveFeedMaxInMemoryFetchBytes = 64L * 1024L * 1024L

@Serializable
data class LiveFeedCacheRequest(
    val id: String,
    val url: String,
    val kind: JsonObject,
)

@Serializable
data class LiveFeedInstalledSummary(
    val product: String,
    val version: String,
    @SerialName("state_sha256")
    val stateSha256: String,
    @SerialName("collected_at_utc")
    val collectedAtUtc: String? = null,
    @SerialName("payload_kind")
    val payloadKind: String,
    @SerialName("blob_sha256")
    val blobSha256: String? = null,
)

@Serializable
data class LiveFeedResourceManifest(
    val summary: LiveFeedInstalledSummary,
    val resources: List<LiveFeedResourceRef>,
)

@Serializable
data class LiveFeedResourceRef(
    val kind: String,
    @SerialName("blob_sha256")
    val blobSha256: String,
    val bytes: Long,
)

@Serializable
data class LiveFeedSseEvent(
    val id: String? = null,
    val event: String? = null,
    val data: String,
)

@Serializable
data class LiveFeedConnectionEvent(
    val kind: String,
    val message: String? = null,
    @SerialName("source_url")
    val sourceUrl: String? = null,
    @SerialName("status_url")
    val statusUrl: String? = null,
    @SerialName("network_status")
    val networkStatus: LiveFeedNetworkStatus? = null,
)

@Serializable
enum class LiveFeedNetworkStatus {
    @SerialName("unmetered")
    Unmetered,

    @SerialName("metered")
    Metered,

    @SerialName("no_active_network")
    NoActiveNetwork,

    @SerialName("unknown")
    Unknown,
}

@Serializable
data class LiveFeedRuntimeInput(
    val kind: String,
    val message: String? = null,
    @SerialName("source_url")
    val sourceUrl: String? = null,
    @SerialName("status_url")
    val statusUrl: String? = null,
    @SerialName("network_status")
    val networkStatus: LiveFeedNetworkStatus? = null,
)

@Serializable
data class LiveFeedRuntimeDecision(
    @SerialName("connection_event")
    val connectionEvent: LiveFeedConnectionEvent? = null,
    @SerialName("refresh_current")
    val refreshCurrent: Boolean = false,
    @SerialName("reconnect_delay_ms")
    val reconnectDelayMs: Long? = null,
)

class LiveFeedCache(
    private val sourceRootUrl: String,
    installedStatesJson: String = "[]",
    private val bridge: NativeBridge = NativeBindings,
    private val json: Json = Json {
        encodeDefaults = true
        ignoreUnknownKeys = true
    },
) : AutoCloseable {
    private val handle = bridge.createLiveFeedCache(sourceRootUrl, installedStatesJson)
    private val lifecycleLock = Any()
    private val operationLock = Any()
    private val persistedRestoreLock = Any()
    private var activeCalls = 0
    private var destroyed = false
    private var persistedRestoreComplete = false
    @Volatile
    private var closed = false

    val isClosed: Boolean
        get() = closed

    fun missingRequests(): List<LiveFeedCacheRequest> = withOpenHandle { handle ->
        json.decodeFromString(bridge.liveFeedCacheMissingRequestsJson(handle))
    }

    fun missingRequestsAtEpochMs(epochMs: Long): List<LiveFeedCacheRequest> = withOpenHandle { handle ->
        json.decodeFromString(bridge.liveFeedCacheMissingRequestsAtEpochMsJson(handle, epochMs))
    }

    fun currentRefreshRequestsAtEpochMs(epochMs: Long): List<LiveFeedCacheRequest> = withOpenHandle { handle ->
        json.decodeFromString(bridge.liveFeedCacheCurrentRefreshRequestsAtEpochMsJson(handle, epochMs))
    }

    fun recordRequestFailure(requestId: String, epochMs: Long) = withOpenHandle { handle ->
        bridge.liveFeedCacheRecordRequestFailure(handle, requestId, epochMs)
    }

    fun runtimeDecision(input: LiveFeedRuntimeInput): LiveFeedRuntimeDecision = withOpenHandle { handle ->
        json.decodeFromString(bridge.liveFeedCacheRuntimeDecisionJson(handle, json.encodeToString(input)))
    }

    fun installFetchedBytes(
        request: LiveFeedCacheRequest,
        bytes: ByteArray,
    ): LiveFeedInstalledSummary? = withOpenHandle { handle ->
        json.decodeFromString(
            bridge.liveFeedCacheInstallFetchedBytesJson(
                handle,
                json.encodeToString(request),
                bytes,
            ),
        )
    }

    fun ingestSseEvent(event: LiveFeedSseEvent): Boolean = withOpenHandle { handle ->
        json.decodeFromString(
            bridge.liveFeedCacheIngestSseEventJson(handle, json.encodeToString(event)),
        )
    }

    fun installedSummary(product: String): LiveFeedInstalledSummary? = withOpenHandle { handle ->
        json.decodeFromString(bridge.liveFeedCacheInstalledSummaryJson(handle, product))
    }

    fun retainedSummaries(product: String): List<LiveFeedInstalledSummary> = withOpenHandle { handle ->
        json.decodeFromString(bridge.liveFeedCacheRetainedSummariesJson(handle, product))
    }

    fun releasePersistedPayloadBytes(product: String, version: String) = withOpenHandle { handle ->
        bridge.liveFeedCacheReleasePersistedPayloadBytes(handle, product, version)
    }

    fun ingestInstalledPayload(summary: LiveFeedInstalledSummary, payloadBytes: ByteArray) = withOpenHandle { handle ->
        bridge.liveFeedCacheIngestInstalledPayloadBytes(
            handle,
            json.encodeToString(summary),
            payloadBytes,
        )
    }

    fun installedPayloadBytes(product: String, version: String): ByteArray = withOpenHandle { handle ->
        bridge.liveFeedCacheInstalledPayloadBytes(handle, product, version)
    }

    fun resourceManifest(product: String): LiveFeedResourceManifest? = withOpenHandle { handle ->
        json.decodeFromString(bridge.liveFeedCacheResourceManifestJson(handle, product))
    }

    fun resourceBytes(product: String, blobSha256: String): ByteArray = withOpenHandle { handle ->
        bridge.liveFeedCacheResourceBytes(handle, product, blobSha256)
    }

    fun restoreInstalledResources(
        manifest: LiveFeedResourceManifest,
        readResource: (LiveFeedResourceRef) -> ByteArray,
    ) = withOpenHandle { handle ->
        bridge.liveFeedCacheBeginRestoringResources(handle, json.encodeToString(manifest))
        for (resource in manifest.resources) {
            bridge.liveFeedCacheRestoreResourceBytes(
                handle,
                manifest.summary.product,
                resource.blobSha256,
                readResource(resource),
            )
        }
        bridge.liveFeedCacheFinishRestoringResources(handle, manifest.summary.product)
    }

    fun installProductInSessionJson(
        sessionHandle: Long,
        product: String,
        version: String,
    ): String = withOpenHandle { handle ->
        bridge.liveFeedCacheInstallProductInSessionJson(handle, sessionHandle, product, version)
    }

    fun preparedInstallCandidate(product: String, version: String): ByteArray? =
        withOpenHandle { handle ->
            bridge.liveFeedCachePreparedInstallCandidate(handle, product, version)
                .takeIf(ByteArray::isNotEmpty)
        }

    fun installPreparedProductInSessionJson(
        sessionHandle: Long,
        product: String,
        version: String,
        preparedBytes: ByteArray,
    ): String = withOpenHandle { handle ->
        bridge.liveFeedCacheInstallPreparedProductInSessionJson(
            handle,
            sessionHandle,
            product,
            version,
            preparedBytes,
        )
    }

    fun syncCatalogInSessionJson(sessionHandle: Long): String = withOpenHandle { handle ->
        bridge.liveFeedCacheSyncCatalogInSessionJson(handle, sessionHandle)
    }

    fun liveFeedEventsUrl(): String = bridge.liveFeedEventsUrl(sourceRootUrl)

    fun liveFeedStatusUrl(): String = bridge.liveFeedStatusUrl(sourceRootUrl)

    fun pumpOnce(
        fetch: (LiveFeedCacheRequest) -> ByteArray,
        persist: (LiveFeedInstalledSummary, ByteArray) -> Unit,
        promote: ((LiveFeedInstalledSummary) -> Unit)? = null,
    ): Int {
        var installs = 0
        for (request in missingRequests()) {
            val summary = installFetchedBytes(request, fetch(request)) ?: continue
            persist(summary, installedPayloadBytes(summary.product, summary.version))
            promote?.invoke(summary)
            installs += 1
        }
        return installs
    }

    internal fun restorePersistedOnce(restore: () -> Unit) {
        synchronized(persistedRestoreLock) {
            if (persistedRestoreComplete) return
            restore()
            persistedRestoreComplete = true
        }
    }

    override fun close() {
        val destroyNow = synchronized(lifecycleLock) {
            if (closed) return
            closed = true
            if (activeCalls == 0 && !destroyed) {
                destroyed = true
                true
            } else {
                false
            }
        }
        if (destroyNow) {
            bridge.destroyLiveFeedCache(handle)
        }
    }

    private inline fun <T> withOpenHandle(block: (Long) -> T): T {
        synchronized(lifecycleLock) {
            if (closed) {
                throw LiveFeedCacheClosedException()
            }
            activeCalls += 1
        }
        try {
            return synchronized(operationLock) {
                block(handle)
            }
        } finally {
            val destroyNow = synchronized(lifecycleLock) {
                activeCalls -= 1
                if (closed && activeCalls == 0 && !destroyed) {
                    destroyed = true
                    true
                } else {
                    false
                }
            }
            if (destroyNow) {
                bridge.destroyLiveFeedCache(handle)
            }
        }
    }

    class LiveFeedCacheClosedException :
        CancellationException("live-feed cache is closed")
}

class AndroidLiveFeedClient(
    private val context: Context,
    private val cache: LiveFeedCache,
    private val sourceRootUrl: String,
    private val json: Json = Json {
        encodeDefaults = true
        ignoreUnknownKeys = true
    },
) {
    private val pumpMutex = Mutex()
    private val eventsUrl = cache.liveFeedEventsUrl()
    private val statusUrl = cache.liveFeedStatusUrl()
    private val sseHttpClient = OkHttpClient.Builder()
        .connectTimeout(LiveFeedSseConnectTimeoutMs.toLong(), TimeUnit.MILLISECONDS)
        .readTimeout(LiveFeedSseIdleTimeoutMs.toLong(), TimeUnit.MILLISECONDS)
        .callTimeout(0, TimeUnit.MILLISECONDS)
        .retryOnConnectionFailure(true)
        .build()

    suspend fun bootstrapAndRun(
        promote: suspend (LiveFeedInstalledSummary) -> Unit,
        onChanged: suspend () -> Unit,
        onConnectionEvent: suspend (LiveFeedConnectionEvent) -> Unit = {},
    ) = coroutineScope {
        val networkChanges = Channel<LiveFeedNetworkStatus>(Channel.CONFLATED)
        val networkCallback = registerLiveFeedNetworkCallback(context, eventsUrl) { status ->
            networkChanges.trySend(status)
        }
        val networkPump = launch {
            for (status in networkChanges) {
                handleRuntimeEvent(
                    kind = "network_status",
                    networkStatus = status,
                    promote = promote,
                    onChanged = onChanged,
                    onConnectionEvent = onConnectionEvent,
                )
            }
        }
        try {
            handleRuntimeEvent(
                kind = "start",
                promote = promote,
                onChanged = onChanged,
                onConnectionEvent = onConnectionEvent,
            )
            networkChanges.trySend(detectLiveFeedNetworkStatus(context, eventsUrl))
            while (kotlin.coroutines.coroutineContext.isActive) {
                runCatching {
                    readSseLoop(promote, onChanged, onConnectionEvent)
                }.onFailure { error ->
                    if (error is CancellationException) throw error
                    val decision = if (error is LiveFeedSseIdleTimeoutException) {
                        diagnosticLogInfo(TAG) {
                            "live-feed SSE idle for ${LiveFeedSseIdleTimeoutMs}ms; reconnecting"
                        }
                        handleRuntimeEvent(
                            kind = "idle_timeout",
                            message = error.message,
                            promote = promote,
                            onChanged = onChanged,
                            onConnectionEvent = onConnectionEvent,
                        )
                    } else {
                        Log.w(TAG, "live-feed SSE loop failed: ${error.message}", error)
                        handleRuntimeEvent(
                            kind = "error",
                            message = error.message ?: error::class.java.simpleName,
                            networkStatus = detectLiveFeedNetworkStatus(context, eventsUrl),
                            promote = promote,
                            onChanged = onChanged,
                            onConnectionEvent = onConnectionEvent,
                        )
                    }
                    decision.reconnectDelayMs?.takeIf { it > 0 }?.let { delay(it) }
                }
            }
        } finally {
            networkCallback.close()
            networkChanges.close()
            networkPump.cancel()
        }
    }

    private suspend fun handleRuntimeEvent(
        kind: String,
        message: String? = null,
        networkStatus: LiveFeedNetworkStatus? = null,
        promote: suspend (LiveFeedInstalledSummary) -> Unit,
        onChanged: suspend () -> Unit,
        onConnectionEvent: suspend (LiveFeedConnectionEvent) -> Unit,
    ): LiveFeedRuntimeDecision {
        val decision = cache.runtimeDecision(
            LiveFeedRuntimeInput(
                kind = kind,
                message = message,
                sourceUrl = sourceRootUrl,
                statusUrl = statusUrl,
                networkStatus = networkStatus,
            ),
        )
        decision.connectionEvent?.let { onConnectionEvent(it) }
        if (decision.refreshCurrent) {
            refreshCurrentAndPump(promote, onChanged)
        }
        return decision
    }

    suspend fun pumpUntilSettled(
        promote: suspend (LiveFeedInstalledSummary) -> Unit,
        onChanged: suspend () -> Unit,
    ): Int = pumpMutex.withLock {
        var installs = 0
        while (kotlin.coroutines.coroutineContext.isActive) {
            val nowMs = SystemClock.elapsedRealtime()
            val requests = cache.missingRequestsAtEpochMs(nowMs)
            if (requests.isEmpty()) {
                return installs
            }
            val result = pumpRequestsOnce(requests, promote, onChanged)
            installs += result.installs
            if (!result.madeProgress) {
                return installs
            }
        }
        return installs
    }

    private suspend fun refreshCurrentAndPump(
        promote: suspend (LiveFeedInstalledSummary) -> Unit,
        onChanged: suspend () -> Unit,
    ): Int = pumpMutex.withLock {
        val nowMs = SystemClock.elapsedRealtime()
        val result = pumpRequestsOnce(cache.currentRefreshRequestsAtEpochMs(nowMs), promote, onChanged)
        result.installs
    } + pumpUntilSettled(promote, onChanged)

    private suspend fun pumpRequestsOnce(
        requests: List<LiveFeedCacheRequest>,
        promote: suspend (LiveFeedInstalledSummary) -> Unit,
        onChanged: suspend () -> Unit,
    ): LiveFeedPumpResult {
        var installs = 0
        var madeProgress = false
        for (request in requests) {
            try {
                val bytes = withContext(Dispatchers.IO) { fetchBytes(request.url) }
                val summary = withContext(Dispatchers.IO) {
                    cache.installFetchedBytes(request, bytes)
                }
                if (summary == null) {
                    onChanged()
                    madeProgress = true
                    continue
                }
                val resourceManifest = withContext(Dispatchers.IO) {
                    cache.resourceManifest(summary.product)
                }
                if (resourceManifest != null) {
                    withContext(Dispatchers.IO) {
                        LiveFeedCacheStore.stageResources(context, resourceManifest) { resource ->
                            cache.resourceBytes(summary.product, resource.blobSha256)
                        }
                    }
                }
                withContext(Dispatchers.IO) {
                    if (resourceManifest != null) {
                        LiveFeedCacheStore.commitResourceManifest(context, resourceManifest)
                    } else {
                        LiveFeedCacheStore.persist(
                            context,
                            summary,
                            cache.installedPayloadBytes(summary.product, summary.version),
                        )
                    }
                }
                promote(summary)
                withContext(Dispatchers.IO) {
                    LiveFeedCacheStore.retainVersions(
                        context,
                        summary.product,
                        cache.retainedSummaries(summary.product).mapTo(mutableSetOf()) { it.version },
                    )
                    cache.releasePersistedPayloadBytes(summary.product, summary.version)
                }
                onChanged()
                installs += 1
                madeProgress = true
            } catch (error: Exception) {
                if (error is CancellationException) throw error
                cache.recordRequestFailure(request.id, SystemClock.elapsedRealtime())
                Log.w(
                    TAG,
                    "failed to install live-feed request id=${request.id} url=${request.url}; " +
                        "retrying after core cooldown: ${error.message}",
                    error,
                )
            }
        }
        return LiveFeedPumpResult(installs = installs, madeProgress = madeProgress)
    }

    private suspend fun readSseLoop(
        promote: suspend (LiveFeedInstalledSummary) -> Unit,
        onChanged: suspend () -> Unit,
        onConnectionEvent: suspend (LiveFeedConnectionEvent) -> Unit,
    ) = withContext(Dispatchers.IO) {
        handleRuntimeEvent(
            kind = "connecting",
            networkStatus = detectLiveFeedNetworkStatus(context, eventsUrl),
            promote = promote,
            onChanged = onChanged,
            onConnectionEvent = onConnectionEvent,
        )
        val request = Request.Builder()
            .url(eventsUrl)
            .header("Accept", "text/event-stream")
            .header("Cache-Control", "no-cache")
            .build()
        val call = sseHttpClient.newCall(request)
        val cancellationHandle = kotlin.coroutines.coroutineContext[Job]?.invokeOnCompletion { cause ->
            if (cause is CancellationException) {
                call.cancel()
            }
        }
        var connected = false
        try {
            Log.i(TAG, "live-feed SSE connect start url=$eventsUrl")
            val response = call.execute()
            response.use {
                if (!it.isSuccessful) {
                    throw IOException("live-feed SSE HTTP ${it.code}: ${it.message}")
                }
                val body = it.body ?: throw IOException("live-feed SSE response has no body")
                val reader = body.charStream().buffered()
                connected = true
                Log.i(TAG, "live-feed SSE connected url=$eventsUrl code=${it.code} protocol=${it.protocol}")
                handleRuntimeEvent(
                    kind = "connected",
                    networkStatus = detectLiveFeedNetworkStatus(context, eventsUrl),
                    promote = promote,
                    onChanged = onChanged,
                    onConnectionEvent = onConnectionEvent,
                )
                var eventName: String? = null
                var eventId: String? = null
                val dataLines = mutableListOf<String>()
                suspend fun flushEvent() {
                    if (dataLines.isEmpty()) return
                    val event = LiveFeedSseEvent(
                        id = eventId,
                        event = eventName ?: "message",
                        data = dataLines.joinToString("\n"),
                    )
                    dataLines.clear()
                    eventName = null
                    eventId = null
                    handleRuntimeEvent(
                        kind = "message",
                        promote = promote,
                        onChanged = onChanged,
                        onConnectionEvent = onConnectionEvent,
                    )
                    if (!cache.ingestSseEvent(event)) return
                    pumpUntilSettled(promote, onChanged)
                }
                while (kotlin.coroutines.coroutineContext.isActive) {
                    val line = reader.readLine() ?: break
                    when {
                        line.isEmpty() -> flushEvent()
                        line.startsWith(":") -> Unit
                        line.startsWith("event:") -> eventName = line.removePrefix("event:").trimStart()
                        line.startsWith("id:") -> eventId = line.removePrefix("id:").trimStart()
                        line.startsWith("data:") -> dataLines += line.removePrefix("data:").trimStart()
                    }
                }
                flushEvent()
            }
            handleRuntimeEvent(
                kind = "closed",
                networkStatus = detectLiveFeedNetworkStatus(context, eventsUrl),
                promote = promote,
                onChanged = onChanged,
                onConnectionEvent = onConnectionEvent,
            )
        } catch (error: SocketTimeoutException) {
            if (connected) {
                throw LiveFeedSseIdleTimeoutException()
            }
            throw error
        } finally {
            cancellationHandle?.dispose()
            call.cancel()
        }
    }

    private fun fetchBytes(url: String): ByteArray {
        require(url.startsWith("http://") || url.startsWith("https://")) {
            "core live-feed request URL must be absolute: $url"
        }
        val resolved = url
        val startMs = SystemClock.elapsedRealtime()
        val connection = (URL(resolved).openConnection() as HttpURLConnection).apply {
            connectTimeout = 5_000
            readTimeout = 20_000
        }
        return try {
            val contentLength = connection.contentLengthLong
            Log.i(
                TAG,
                "fetch live-feed start url=$resolved contentLength=$contentLength maxBytes=$LiveFeedMaxInMemoryFetchBytes",
            )
            if (contentLength > LiveFeedMaxInMemoryFetchBytes) {
                throw LiveFeedResponseTooLargeException(
                    url = resolved,
                    maxBytes = LiveFeedMaxInMemoryFetchBytes,
                    observedBytes = contentLength,
                )
            }
            connection.inputStream.buffered().use {
                readLiveFeedBytesBounded(
                    input = it,
                    maxBytes = LiveFeedMaxInMemoryFetchBytes,
                    url = resolved,
                )
            }.also {
                diagnosticLogInfo(TAG) {
                    "fetched live-feed bytes=${it.size} elapsedMs=${SystemClock.elapsedRealtime() - startMs} url=$resolved"
                }
            }
        } finally {
            connection.disconnect()
        }
    }

    companion object {
        private const val TAG = LiveFeedLogTag
    }
}

private class LiveFeedSseIdleTimeoutException :
    IOException("live-feed SSE idle timeout")

class LiveFeedResponseTooLargeException(
    url: String,
    maxBytes: Long,
    observedBytes: Long,
) : IOException("live-feed response too large: observedBytes=$observedBytes maxBytes=$maxBytes url=$url")

private data class LiveFeedPumpResult(
    val installs: Int,
    val madeProgress: Boolean,
)

internal fun readLiveFeedBytesBounded(
    input: InputStream,
    maxBytes: Long,
    url: String,
): ByteArray {
    require(maxBytes <= Int.MAX_VALUE) { "maxBytes must fit in a ByteArray" }
    val output = ByteArrayOutputStream()
    val buffer = ByteArray(DEFAULT_BUFFER_SIZE)
    var totalBytes = 0L
    while (true) {
        val bytesRead = input.read(buffer)
        if (bytesRead == -1) break
        totalBytes += bytesRead.toLong()
        if (totalBytes > maxBytes) {
            throw LiveFeedResponseTooLargeException(
                url = url,
                maxBytes = maxBytes,
                observedBytes = totalBytes,
            )
        }
        output.write(buffer, 0, bytesRead)
    }
    return output.toByteArray()
}

internal fun registerLiveFeedNetworkCallback(
    context: Context,
    url: String,
    onChanged: (LiveFeedNetworkStatus) -> Unit,
): AutoCloseable {
    val connectivity = context.getSystemService(Context.CONNECTIVITY_SERVICE) as? ConnectivityManager
        ?: return AutoCloseable {}
    val callback = object : ConnectivityManager.NetworkCallback() {
        override fun onAvailable(network: Network) {
            onChanged(detectLiveFeedNetworkStatus(context, url))
        }

        override fun onLost(network: Network) {
            onChanged(detectLiveFeedNetworkStatus(context, url))
        }

        override fun onCapabilitiesChanged(
            network: Network,
            networkCapabilities: NetworkCapabilities,
        ) {
            onChanged(detectLiveFeedNetworkStatus(context, url))
        }
    }
    return runCatching {
        connectivity.registerDefaultNetworkCallback(callback)
        AutoCloseable {
            runCatching {
                connectivity.unregisterNetworkCallback(callback)
            }.onFailure { error ->
                Log.w(LiveFeedLogTag, "failed to unregister live-feed network callback", error)
            }
        }
    }.getOrElse { error ->
        Log.w(LiveFeedLogTag, "failed to register live-feed network callback", error)
        AutoCloseable {}
    }
}

internal fun detectLiveFeedNetworkStatus(context: Context, url: String): LiveFeedNetworkStatus {
    val host = runCatching { URL(url).host }.getOrNull().orEmpty()
    if (host == "10.0.2.2" || host == "localhost" || host == "127.0.0.1") {
        return LiveFeedNetworkStatus.Unmetered
    }
    val connectivity = context.getSystemService(Context.CONNECTIVITY_SERVICE) as? ConnectivityManager
        ?: return LiveFeedNetworkStatus.Unknown
    val network = connectivity.activeNetwork ?: return LiveFeedNetworkStatus.NoActiveNetwork
    val capabilities = connectivity.getNetworkCapabilities(network)
        ?: return LiveFeedNetworkStatus.Unknown
    return if (capabilities.hasCapability(NetworkCapabilities.NET_CAPABILITY_NOT_METERED)) {
        LiveFeedNetworkStatus.Unmetered
    } else {
        LiveFeedNetworkStatus.Metered
    }
}

object LiveFeedCacheStore {
    private val json = Json {
        encodeDefaults = true
        ignoreUnknownKeys = true
    }

    fun create(
        sourceRootUrl: String,
        bridge: NativeBridge = NativeBindings,
    ): LiveFeedCache =
        LiveFeedCache(sourceRootUrl = sourceRootUrl, bridge = bridge, json = json)

    fun restore(
        context: Context,
        cache: LiveFeedCache,
    ) {
        cache.restorePersistedOnce {
            for (stored in listInstalledResourceManifests(context)) {
                runCatching {
                    cache.restoreInstalledResources(stored.manifest) { resource ->
                        stored.resourceFile(resource).readBytes()
                    }
                }.onFailure { error ->
                    if (error is CancellationException) throw error
                    stored.manifestFile.delete()
                }
            }
            for (entry in listInstalled(context)) {
                runCatching {
                    cache.ingestInstalledPayload(entry.summary, entry.payloadFile.readBytes())
                    cache.releasePersistedPayloadBytes(
                        entry.summary.product,
                        entry.summary.version,
                    )
                }.onFailure { error ->
                    if (error is CancellationException) throw error
                    entry.payloadFile.parentFile?.deleteRecursively()
                }
            }
        }
    }

    fun stageResources(
        context: Context,
        manifest: LiveFeedResourceManifest,
        readResource: (LiveFeedResourceRef) -> ByteArray,
    ) {
        val productDir = File(rootDir(context), safePathComponent(manifest.summary.product))
        val resourcesDir = File(productDir, "resources")
        resourcesDir.mkdirs()
        for (resource in manifest.resources) {
            require(resource.blobSha256.matches(Regex("[0-9a-f]{64}"))) {
                "unsafe live-feed resource hash: ${resource.blobSha256}"
            }
            val target = File(resourcesDir, "${resource.blobSha256}.blob")
            if (target.isFile && target.length() == resource.bytes) continue
            val bytes = readResource(resource)
            require(bytes.size.toLong() == resource.bytes) {
                "live-feed resource ${resource.blobSha256} has ${bytes.size} bytes, expected ${resource.bytes}"
            }
            val temp = File(resourcesDir, ".${resource.blobSha256}.tmp")
            temp.writeBytes(bytes)
            Files.move(temp.toPath(), target.toPath(), ATOMIC_MOVE, REPLACE_EXISTING)
        }
    }

    fun commitResourceManifest(context: Context, manifest: LiveFeedResourceManifest) {
        val productDir = File(rootDir(context), safePathComponent(manifest.summary.product))
        productDir.mkdirs()
        val target = File(productDir, "installed-resources.json")
        val temp = File(productDir, ".installed-resources.tmp")
        temp.writeText(json.encodeToString(manifest))
        Files.move(temp.toPath(), target.toPath(), ATOMIC_MOVE, REPLACE_EXISTING)
        val retained = manifest.resources.mapTo(mutableSetOf()) { "${it.blobSha256}.blob" }
        File(productDir, "resources")
            .listFiles()
            ?.filter { it.isFile && !it.name.startsWith(".") && it.name !in retained }
            ?.forEach { it.delete() }
        productDir
            .listFiles()
            ?.filter { it.isDirectory && it.name != "resources" }
            ?.forEach { it.deleteRecursively() }
    }

    private fun listInstalledResourceManifests(context: Context): List<LiveFeedStoredResources> =
        rootDir(context)
            .takeIf { it.isDirectory }
            ?.listFiles()
            ?.asList()
            .orEmpty()
            .mapNotNull { productDir ->
                val manifestFile = File(productDir, "installed-resources.json")
                if (!manifestFile.isFile) return@mapNotNull null
                runCatching {
                    LiveFeedStoredResources(
                        root = File(productDir, "resources"),
                        manifest = json.decodeFromString(manifestFile.readText()),
                        manifestFile = manifestFile,
                    )
                }.getOrNull()
            }

    fun listInstalled(context: Context): List<LiveFeedStoredPayload> =
        rootDir(context)
            .takeIf { it.isDirectory }
            ?.listFiles()
            ?.asList()
            .orEmpty()
            .flatMap { productDir ->
                productDir.listFiles()?.asList().orEmpty()
            }
            .mapNotNull { versionDir ->
                val metadataFile = File(versionDir, "metadata.json")
                val payloadFile = File(versionDir, "payload.bin")
                if (!metadataFile.isFile || !payloadFile.isFile) {
                    return@mapNotNull null
                }
                runCatching {
                    LiveFeedStoredPayload(
                        summary = json.decodeFromString(metadataFile.readText()),
                        payloadFile = payloadFile,
                    )
                }.getOrNull()
            }
            .sortedBy { "${it.summary.product}/${it.summary.version}" }

    fun listInstalledSummaries(context: Context): List<LiveFeedInstalledSummary> =
        (listInstalled(context).map { it.summary } +
            listInstalledResourceManifests(context).map { it.manifest.summary })
            .sortedBy { "${it.product}/${it.version}" }

    fun readPackageMember(
        context: Context,
        product: String,
        version: String,
        blobSha256: String,
        memberPath: String,
    ): ByteArray {
        require(!memberPath.startsWith('/') && memberPath.split('/').none { it == ".." }) {
            "unsafe live-feed package member path: $memberPath"
        }
        val versionDir = File(
            File(rootDir(context), safePathComponent(product)),
            safePathComponent(version),
        )
        val metadataFile = File(versionDir, "metadata.json")
        val payloadFile = File(versionDir, "payload.bin")
        val summary = runCatching {
            json.decodeFromString<LiveFeedInstalledSummary>(metadataFile.readText())
        }.getOrElse {
            error("live-feed package $product/$version metadata is unavailable")
        }
        require(
            summary.product == product &&
                summary.version == version &&
                summary.blobSha256 == blobSha256,
        ) {
            "live-feed package $product/$version does not match blob $blobSha256"
        }
        require(payloadFile.isFile) {
            "live-feed package $product/$version payload is unavailable"
        }
        return PackageZipStore.readEntryBytes(payloadFile, memberPath)
            ?: error("live-feed package $product/$version has no member $memberPath")
    }

    fun persist(
        context: Context,
        summary: LiveFeedInstalledSummary,
        payloadBytes: ByteArray,
    ) {
        val productDir = File(rootDir(context), safePathComponent(summary.product))
        val targetDir = File(productDir, safePathComponent(summary.version))
        val tempDir = File(productDir, ".${safePathComponent(summary.version)}.tmp")
        if (tempDir.exists()) {
            tempDir.deleteRecursively()
        }
        tempDir.mkdirs()
        File(tempDir, "metadata.json").writeText(json.encodeToString(summary))
        File(tempDir, "payload.bin").writeBytes(payloadBytes)
        if (targetDir.exists()) {
            targetDir.deleteRecursively()
        }
        if (!tempDir.renameTo(targetDir)) {
            tempDir.copyRecursively(targetDir, overwrite = true)
            tempDir.deleteRecursively()
        }
    }

    fun retainVersions(
        context: Context,
        product: String,
        versions: Set<String>,
    ) {
        val productDir = File(rootDir(context), safePathComponent(product))
        val retainedNames = versions.mapTo(mutableSetOf(), ::safePathComponent)
        productDir
            .listFiles()
            ?.filter {
                it.isDirectory &&
                    it.name != "resources" &&
                    !it.name.startsWith(".") &&
                    it.name !in retainedNames
            }
            ?.forEach { it.deleteRecursively() }
    }

    private fun rootDir(context: Context): File =
        File(context.filesDir, LiveFeedCacheDirectoryName)

    private fun safePathComponent(value: String): String {
        require(value.isNotBlank()) { "live-feed path component must not be blank" }
        require(!value.contains('/') && !value.contains('\\') && value != "." && value != "..") {
            "unsafe live-feed path component: $value"
        }
        return value
    }
}

data class LiveFeedStoredPayload(
    val summary: LiveFeedInstalledSummary,
    val payloadFile: File,
)

data class LiveFeedStoredResources(
    val root: File,
    val manifest: LiveFeedResourceManifest,
    val manifestFile: File,
) {
    fun resourceFile(resource: LiveFeedResourceRef): File =
        File(root, "${resource.blobSha256}.blob")
}
