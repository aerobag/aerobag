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
import kotlinx.coroutines.CancellationException
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
import org.aerobag.app.diagnosticLogInfo

private const val LiveFeedCacheDirectoryName = "live-feeds"
private const val LiveFeedSseConnectTimeoutMs = 5_000
private const val LiveFeedSseIdleTimeoutMs = 65_000
private const val LiveFeedEventsPath = "/live-feeds/v2/events"
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
    @SerialName("payload_kind")
    val payloadKind: String,
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
    installedStatesJson: String = "[]",
    private val bridge: NativeBridge = NativeBindings,
    private val json: Json = Json {
        encodeDefaults = true
        ignoreUnknownKeys = true
    },
) : AutoCloseable {
    private val handle = bridge.createLiveFeedCache(installedStatesJson)
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

    fun runtimeDecision(input: LiveFeedRuntimeInput): LiveFeedRuntimeDecision =
        json.decodeFromString(bridge.liveFeedRuntimeDecisionJson(json.encodeToString(input)))

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

    fun ingestInstalledPayload(summary: LiveFeedInstalledSummary, payloadBytes: ByteArray) = withOpenHandle { handle ->
        bridge.liveFeedCacheIngestInstalledPayloadBytes(
            handle,
            json.encodeToString(summary),
            payloadBytes,
        )
    }

    fun installedPayloadBytes(product: String): ByteArray = withOpenHandle { handle ->
        bridge.liveFeedCacheInstalledPayloadBytes(handle, product)
    }

    fun installProductInSessionJson(sessionHandle: Long, product: String): String = withOpenHandle { handle ->
        bridge.liveFeedCacheInstallProductInSessionJson(handle, sessionHandle, product)
    }

    fun syncCatalogInSessionJson(sessionHandle: Long): String = withOpenHandle { handle ->
        bridge.liveFeedCacheSyncCatalogInSessionJson(handle, sessionHandle)
    }

    fun pumpOnce(
        fetch: (LiveFeedCacheRequest) -> ByteArray,
        persist: (LiveFeedInstalledSummary, ByteArray) -> Unit,
        promote: ((LiveFeedInstalledSummary) -> Unit)? = null,
    ): Int {
        var installs = 0
        for (request in missingRequests()) {
            val summary = installFetchedBytes(request, fetch(request)) ?: continue
            persist(summary, installedPayloadBytes(summary.product))
            promote?.invoke(summary)
            installs += 1
        }
        return installs
    }

    override fun close() {
        synchronized(this) {
            if (closed) return
            closed = true
            bridge.destroyLiveFeedCache(handle)
        }
    }

    private inline fun <T> withOpenHandle(block: (Long) -> T): T =
        synchronized(this) {
            if (closed) {
                throw LiveFeedCacheClosedException()
            }
            block(handle)
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

    suspend fun bootstrapAndRun(
        promote: suspend (LiveFeedInstalledSummary) -> Unit,
        onChanged: suspend () -> Unit,
        onConnectionEvent: suspend (LiveFeedConnectionEvent) -> Unit = {},
    ) = coroutineScope {
        val liveFeedEventsUrl = resolveLiveFeedUrl(LiveFeedEventsPath, sourceRootUrl)
        val networkChanges = Channel<LiveFeedNetworkStatus>(Channel.CONFLATED)
        val networkCallback = registerLiveFeedNetworkCallback(context, liveFeedEventsUrl) { status ->
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
            networkChanges.trySend(detectLiveFeedNetworkStatus(context, liveFeedEventsUrl))
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
                            networkStatus = detectLiveFeedNetworkStatus(context, sourceRootUrl),
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
                statusUrl = liveFeedStatusUrl(sourceRootUrl),
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
                val summary = cache.installFetchedBytes(request, bytes)
                if (summary == null) {
                    onChanged()
                    madeProgress = true
                    continue
                }
                val payloadBytes = cache.installedPayloadBytes(summary.product)
                withContext(Dispatchers.IO) {
                    LiveFeedCacheStore.persist(context, summary, payloadBytes)
                }
                promote(summary)
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
        val url = resolveLiveFeedUrl(LiveFeedEventsPath, sourceRootUrl)
        handleRuntimeEvent(
            kind = "connecting",
            networkStatus = detectLiveFeedNetworkStatus(context, url),
            promote = promote,
            onChanged = onChanged,
            onConnectionEvent = onConnectionEvent,
        )
        val connection = (URL(url).openConnection() as HttpURLConnection).apply {
            connectTimeout = LiveFeedSseConnectTimeoutMs
            readTimeout = LiveFeedSseIdleTimeoutMs
            setRequestProperty("Accept", "text/event-stream")
        }
        var connected = false
        try {
            connection.inputStream.bufferedReader().use { reader ->
                connected = true
                handleRuntimeEvent(
                    kind = "connected",
                    networkStatus = detectLiveFeedNetworkStatus(context, url),
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
                networkStatus = detectLiveFeedNetworkStatus(context, url),
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
            connection.disconnect()
        }
    }

    private fun fetchBytes(url: String): ByteArray {
        val resolved = resolveLiveFeedUrl(url, sourceRootUrl)
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

fun resolveLiveFeedUrl(url: String, sourceRootUrl: String): String =
    when {
        url.startsWith("http://") || url.startsWith("https://") -> url
        url.startsWith("/") -> "${sourceRootUrl.trimEnd('/')}$url"
        else -> "${sourceRootUrl.trimEnd('/')}/live-feeds/${url.trimStart('/')}"
    }

fun liveFeedStatusUrl(sourceRootUrl: String): String =
    resolveLiveFeedUrl("/live-feeds/status.html", sourceRootUrl)

object LiveFeedCacheStore {
    private val json = Json {
        encodeDefaults = true
        ignoreUnknownKeys = true
    }

    fun open(
        context: Context,
        bridge: NativeBridge = NativeBindings,
    ): LiveFeedCache {
        val cache = LiveFeedCache(bridge = bridge, json = json)
        for (entry in listInstalled(context)) {
            runCatching {
                cache.ingestInstalledPayload(entry.summary, entry.payloadFile.readBytes())
            }
        }
        return cache
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
        productDir
            .listFiles()
            ?.filter { it.isDirectory && it.name != targetDir.name && !it.name.startsWith(".") }
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
