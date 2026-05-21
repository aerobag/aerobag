package org.aerobag.app.domain

import android.content.Context
import java.io.File
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.decodeFromString
import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject

private const val LiveFeedCacheDirectoryName = "live-feeds"

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
