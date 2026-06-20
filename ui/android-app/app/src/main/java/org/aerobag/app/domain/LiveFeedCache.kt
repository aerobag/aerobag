package org.aerobag.app.domain

import android.content.Context
import android.net.ConnectivityManager
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
import kotlinx.coroutines.delay
import kotlinx.coroutines.isActive
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
private const val LiveFeedFailedRequestRetryDelayMs = 5 * 60_000L
private const val LiveFeedEventsPath = "/live-feeds/v2/events"
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

    fun missingRequests(): List<LiveFeedCacheRequest> =
        json.decodeFromString(bridge.liveFeedCacheMissingRequestsJson(handle))

    fun installFetchedBytes(
        request: LiveFeedCacheRequest,
        bytes: ByteArray,
    ): LiveFeedInstalledSummary? =
        json.decodeFromString(
            bridge.liveFeedCacheInstallFetchedBytesJson(
                handle,
                json.encodeToString(request),
                bytes,
            ),
        )

    fun ingestSseEvent(event: LiveFeedSseEvent): Boolean =
        json.decodeFromString(
            bridge.liveFeedCacheIngestSseEventJson(handle, json.encodeToString(event)),
        )

    fun installedSummary(product: String): LiveFeedInstalledSummary? =
        json.decodeFromString(bridge.liveFeedCacheInstalledSummaryJson(handle, product))

    fun ingestInstalledPayload(summary: LiveFeedInstalledSummary, payloadBytes: ByteArray) {
        bridge.liveFeedCacheIngestInstalledPayloadBytes(
            handle,
            json.encodeToString(summary),
            payloadBytes,
        )
    }

    fun installedPayloadBytes(product: String): ByteArray =
        bridge.liveFeedCacheInstalledPayloadBytes(handle, product)

    fun installProductInSessionJson(sessionHandle: Long, product: String): String =
        bridge.liveFeedCacheInstallProductInSessionJson(handle, sessionHandle, product)

    fun syncCatalogInSessionJson(sessionHandle: Long): String =
        bridge.liveFeedCacheSyncCatalogInSessionJson(handle, sessionHandle)

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
        bridge.destroyLiveFeedCache(handle)
    }
}

class AndroidLiveFeedClient(
    private val context: Context,
    private val cache: LiveFeedCache,
    private val sourceRootUrl: String,
    private val policy: LiveFeedFetchPolicy = LiveFeedFetchPolicy.AllowAll,
    private val json: Json = Json {
        encodeDefaults = true
        ignoreUnknownKeys = true
    },
) {
    private val retryGate = LiveFeedRequestRetryGate(LiveFeedFailedRequestRetryDelayMs)

    suspend fun bootstrapAndRun(
        promote: suspend (LiveFeedInstalledSummary) -> Unit,
        onChanged: suspend () -> Unit,
        onConnectionEvent: suspend (LiveFeedConnectionEvent) -> Unit = {},
    ) {
        pumpUntilSettled(promote, onChanged)
        while (kotlin.coroutines.coroutineContext.isActive) {
            runCatching {
                readSseLoop(promote, onChanged, onConnectionEvent)
            }.onFailure { error ->
                if (error is CancellationException) throw error
                if (error is SocketTimeoutException) {
                    diagnosticLogInfo(TAG) {
                        "live-feed SSE idle for ${LiveFeedSseIdleTimeoutMs}ms; reconnecting"
                    }
                } else {
                    Log.w(TAG, "live-feed SSE loop failed: ${error.message}", error)
                }
                onConnectionEvent(
                    LiveFeedConnectionEvent(
                        kind = "error",
                        message = error.message ?: error::class.java.simpleName,
                        sourceUrl = sourceRootUrl,
                        statusUrl = liveFeedStatusUrl(sourceRootUrl),
                    ),
                )
                if (error !is SocketTimeoutException) {
                    delay(5_000)
                }
                pumpUntilSettled(promote, onChanged)
            }
        }
    }

    suspend fun pumpUntilSettled(
        promote: suspend (LiveFeedInstalledSummary) -> Unit,
        onChanged: suspend () -> Unit,
    ): Int {
        var installs = 0
        while (kotlin.coroutines.coroutineContext.isActive) {
            val nowMs = SystemClock.elapsedRealtime()
            val requests = cache.missingRequests()
            if (requests.isEmpty()) {
                return installs
            }
            val readyRequests = requests.filter { retryGate.shouldAttempt(it.id, nowMs) }
            if (readyRequests.isEmpty()) {
                return installs
            }
            var madeProgress = false
            for (request in readyRequests) {
                try {
                    val bytes = withContext(Dispatchers.IO) { fetchBytes(request.url) }
                    val summary = cache.installFetchedBytes(request, bytes)
                    retryGate.recordSuccess(request.id)
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
                    retryGate.recordFailure(request.id, SystemClock.elapsedRealtime())
                    Log.w(
                        TAG,
                        "failed to install live-feed request id=${request.id} url=${request.url}; " +
                            "retrying in ${LiveFeedFailedRequestRetryDelayMs / 1000}s: ${error.message}",
                        error,
                    )
                }
            }
            if (!madeProgress) {
                return installs
            }
        }
        return installs
    }

    private suspend fun readSseLoop(
        promote: suspend (LiveFeedInstalledSummary) -> Unit,
        onChanged: suspend () -> Unit,
        onConnectionEvent: suspend (LiveFeedConnectionEvent) -> Unit,
    ) = withContext(Dispatchers.IO) {
        val url = resolveLiveFeedUrl(LiveFeedEventsPath, sourceRootUrl)
        val statusUrl = liveFeedStatusUrl(sourceRootUrl)
        onConnectionEvent(
            LiveFeedConnectionEvent(
                kind = "connecting",
                sourceUrl = sourceRootUrl,
                statusUrl = statusUrl,
            ),
        )
        val connection = (URL(url).openConnection() as HttpURLConnection).apply {
            connectTimeout = LiveFeedSseConnectTimeoutMs
            readTimeout = LiveFeedSseIdleTimeoutMs
            setRequestProperty("Accept", "text/event-stream")
        }
        try {
            connection.inputStream.bufferedReader().use { reader ->
                onConnectionEvent(
                    LiveFeedConnectionEvent(
                        kind = "connected",
                        sourceUrl = sourceRootUrl,
                        statusUrl = statusUrl,
                    ),
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
                    onConnectionEvent(LiveFeedConnectionEvent(kind = "message"))
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
            onConnectionEvent(
                LiveFeedConnectionEvent(
                    kind = "closed",
                    sourceUrl = sourceRootUrl,
                    statusUrl = statusUrl,
                ),
            )
        } finally {
            connection.disconnect()
        }
    }

    private fun fetchBytes(url: String): ByteArray {
        val resolved = resolveLiveFeedUrl(url, sourceRootUrl)
        policy.checkMayFetch(context, resolved)
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
        private const val TAG = "AndroidLiveFeeds"
    }
}

class LiveFeedResponseTooLargeException(
    url: String,
    maxBytes: Long,
    observedBytes: Long,
) : IOException("live-feed response too large: observedBytes=$observedBytes maxBytes=$maxBytes url=$url")

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

internal class LiveFeedRequestRetryGate(
    private val retryDelayMs: Long,
) {
    private val retryNotBeforeMsByRequestId = mutableMapOf<String, Long>()

    fun shouldAttempt(requestId: String, nowMs: Long): Boolean =
        nowMs >= (retryNotBeforeMsByRequestId[requestId] ?: Long.MIN_VALUE)

    fun recordSuccess(requestId: String) {
        retryNotBeforeMsByRequestId.remove(requestId)
    }

    fun recordFailure(requestId: String, nowMs: Long) {
        retryNotBeforeMsByRequestId[requestId] = nowMs + retryDelayMs
    }
}

sealed interface LiveFeedFetchPolicy {
    fun checkMayFetch(context: Context, url: String)

    data object AllowAll : LiveFeedFetchPolicy {
        override fun checkMayFetch(context: Context, url: String) = Unit
    }

    data object UnmeteredOrLocal : LiveFeedFetchPolicy {
        override fun checkMayFetch(context: Context, url: String) {
            val host = runCatching { URL(url).host }.getOrNull().orEmpty()
            if (host == "10.0.2.2" || host == "localhost" || host == "127.0.0.1") {
                return
            }
            val connectivity = context.getSystemService(Context.CONNECTIVITY_SERVICE) as? ConnectivityManager
                ?: return
            val network = connectivity.activeNetwork ?: error("live-feed fetch blocked: no active network")
            val capabilities = connectivity.getNetworkCapabilities(network)
                ?: error("live-feed fetch blocked: network capabilities unavailable")
            if (!capabilities.hasCapability(NetworkCapabilities.NET_CAPABILITY_NOT_METERED)) {
                error("live-feed fetch blocked on metered network")
            }
        }
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
