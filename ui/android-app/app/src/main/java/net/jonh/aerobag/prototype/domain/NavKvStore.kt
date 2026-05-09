package net.jonh.aerobag.prototype.domain

import android.content.Context
import android.os.SystemClock
import android.util.Log
import kotlinx.serialization.KSerializer
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import java.util.Locale

data class CoreResourceRequest(
    val id: String,
    val address: String,
    val optional: Boolean,
)

class NavKvStore private constructor(
    private val bridge: NativeBridge,
    private val json: Json,
    private val handle: Long,
    private val navDbZip: java.io.File,
    private val valueEntryPrefix: String,
) : AutoCloseable {
    private val loadedPages = mutableSetOf<Int>()

    companion object {
        private const val TAG = "NavKvStore"
        private const val ROOT_ENTRY_NAME = "root"
        private const val VALUE_ENTRY_PREFIX = "values_"

        fun open(
            navDbZip: java.io.File,
            bridge: NativeBridge = NativeBindings,
            json: Json = Json {
                encodeDefaults = true
                ignoreUnknownKeys = true
            },
        ): NavKvStore {
            val rootBytes = InstalledPackages.readZipEntryBytes(navDbZip, ROOT_ENTRY_NAME)
            val handle = bridge.navKvOpen(rootBytes)
            return NavKvStore(bridge, json, handle, navDbZip, VALUE_ENTRY_PREFIX)
        }

        fun open(
            context: Context,
            navDbPackageId: String,
            bridge: NativeBridge = NativeBindings,
            json: Json = Json {
                encodeDefaults = true
                ignoreUnknownKeys = true
            },
        ): NavKvStore {
            val appContext = context.applicationContext
            val navDbZip = InstalledPackages.installedFile(appContext, InstalledPackageKind.Data, navDbPackageId)
            return open(navDbZip = navDbZip, bridge = bridge, json = json)
        }
    }

    fun runCoreOperationElement(operation: JsonObject): JsonElement {
        while (true) {
            val outcome = json.parseToJsonElement(bridge.coreHadOperation(handle, operation.toString())).jsonObject
            return when (val state = outcome.getValue("state").jsonPrimitive.content) {
                "complete" -> outcome["result"] ?: JsonNull
                "need_resources" -> {
                    for (resource in parseCoreResourceRequests(outcome)) {
                        ensureNavKvResource(resource)
                    }
                    continue
                }
                else -> error("unknown HAD operation state: $state")
            }
        }
    }

    fun runPagedSessionOperationElement(
        fetchSessionResource: ((CoreResourceRequest) -> ByteArray)? = null,
        ingestSessionResource: ((CoreResourceRequest, ByteArray) -> Unit)? = null,
        operation: () -> String,
    ): JsonElement {
        while (true) {
            val outcome = json.parseToJsonElement(operation()).jsonObject
            return when (val state = outcome.getValue("state").jsonPrimitive.content) {
                "complete" -> outcome["result"] ?: JsonNull
                "need_resources" -> {
                    for (resource in parseCoreResourceRequests(outcome)) {
                        if (resource.id.startsWith("nav_kv/page/")) {
                            ensureNavKvResource(resource)
                        } else {
                            val fetch = fetchSessionResource
                                ?: error("session resource requested without fetcher: ${resource.id}")
                            val ingest = ingestSessionResource
                                ?: error("session resource requested without ingester: ${resource.id}")
                            val bytes = try {
                                fetch(resource)
                            } catch (error: Throwable) {
                                if (resource.optional) {
                                    Log.i(TAG, "optional resource ${resource.id} unavailable: ${error.message}")
                                    ingest(resource, ByteArray(0))
                                    continue
                                }
                                throw error
                            }
                            ingest(resource, bytes)
                        }
                    }
                    continue
                }
                else -> error("unknown HAD session operation state: $state")
            }
        }
    }

    fun <T> runCoreOperation(operation: JsonObject, serializer: KSerializer<T>): T =
        json.decodeFromJsonElement(serializer, runCoreOperationElement(operation))

    fun attachToSession(sessionHandle: Long) {
        bridge.attachNavKvStoreToSession(handle, sessionHandle)
    }

    private fun parseCoreResourceRequests(outcome: JsonObject): List<CoreResourceRequest> =
        outcome.getValue("resources").jsonArray.map { element ->
            val resource = element.jsonObject
            CoreResourceRequest(
                id = resource.getValue("id").jsonPrimitive.content,
                address = resource.getValue("address").jsonPrimitive.content,
                optional = resource["optional"]?.jsonPrimitive?.content?.toBooleanStrictOrNull() ?: false,
            )
        }

    private fun ensureNavKvResource(resource: CoreResourceRequest) {
        val pageIndex = resource.id.removePrefix("nav_kv/page/").toIntOrNull()
            ?: error("unsupported nav_kv resource id: ${resource.id}")
        ensurePage(pageIndex, resource.id)
    }

    @Synchronized
    private fun ensurePage(pageIndex: Int, resourceId: String) {
        if (!loadedPages.add(pageIndex)) {
            return
        }
        val startMs = SystemClock.elapsedRealtime()
        val pageName = String.format(Locale.US, "%04d", pageIndex)
        val pageBytes = InstalledPackages.readZipEntryBytes(navDbZip, "$valueEntryPrefix$pageName")
        bridge.navKvInsertResource(handle, resourceId, pageBytes)
        val elapsedMs = SystemClock.elapsedRealtime() - startMs
        if (elapsedMs >= 10) {
            Log.i(TAG, "ensurePage($pageIndex) took ${elapsedMs}ms")
        }
    }

    override fun close() {
        bridge.navKvDestroy(handle)
    }
}
