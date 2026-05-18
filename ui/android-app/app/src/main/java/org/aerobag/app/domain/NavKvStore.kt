package org.aerobag.app.domain

import android.content.Context
import android.os.SystemClock
import android.util.Log
import kotlinx.serialization.SerialName
import kotlinx.serialization.KSerializer
import kotlinx.serialization.Serializable
import kotlinx.serialization.decodeFromString
import kotlinx.serialization.json.decodeFromJsonElement
import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive

data class CoreResourceRequest(
    val id: String,
    val address: String,
    val optional: Boolean,
)

data class NavDbOpenReport(
    val statuses: List<NavDbArtifactStatus>,
)

@Serializable
private data class WireNavDbArtifactCandidate(
    @SerialName("package_id")
    val packageId: String,
    val filename: String,
)

@Serializable
private data class WireNavDbOpenFinish(
    @SerialName("nav_kv_handle")
    val navKvHandle: Long,
    @SerialName("open_result")
    val openResult: WireNavDbOpenResult,
)

@Serializable
private data class WireNavDbOpenResult(
    @SerialName("selected_package_id")
    val selectedPackageId: String,
    @SerialName("selected_filename")
    val selectedFilename: String,
    val statuses: List<WireNavDbArtifactStatus>,
)

@Serializable
private data class WireNavDbArtifactStatus(
    @SerialName("package_id")
    val packageId: String,
    val filename: String,
    val readable: Boolean,
    val message: String? = null,
)

class NavKvStore private constructor(
    private val bridge: NativeBridge,
    private val json: Json,
    private val handle: Long,
    private val navDbArtifact: InstalledPackageArtifact,
) : AutoCloseable {
    private val loadedPages = mutableSetOf<Int>()

    companion object {
        private const val TAG = "NavKvStore"

        fun open(
            artifact: InstalledPackageArtifact,
            bridge: NativeBridge = NativeBindings,
            json: Json = Json {
                encodeDefaults = true
                ignoreUnknownKeys = true
            },
        ): NavKvStore =
            openCandidates(listOf(artifact), bridge, json).first

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
            val artifact = InstalledPackages.existingInstalledArtifacts(appContext, navDbPackageId).firstOrNull()
                ?: error("missing installed package $navDbPackageId")
            return open(artifact = artifact, bridge = bridge, json = json)
        }

        fun openCandidates(
            artifacts: List<InstalledPackageArtifact>,
            bridge: NativeBridge = NativeBindings,
            json: Json = Json {
                encodeDefaults = true
                ignoreUnknownKeys = true
            },
        ): Pair<NavKvStore, NavDbOpenReport> {
            val artifactsByFilename = artifacts.associateBy { it.filename }
            val controllerHandle = bridge.navDbOpenControllerCreate(
                json.encodeToString(artifacts.map { it.toWireCandidate() }),
            )
            try {
                while (true) {
                    val outcome = json.parseToJsonElement(
                        bridge.navDbOpenControllerStep(controllerHandle),
                    ).jsonObject
                    when (val state = outcome.getValue("state").jsonPrimitive.content) {
                        "complete" -> {
                            val finish = json.decodeFromString<WireNavDbOpenFinish>(
                                bridge.navDbOpenControllerFinish(controllerHandle),
                            )
                            val selectedArtifact = artifactsByFilename[finish.openResult.selectedFilename]
                                ?: error("core selected unknown nav_db artifact ${finish.openResult.selectedFilename}")
                            return NavKvStore(
                                bridge = bridge,
                                json = json,
                                handle = finish.navKvHandle,
                                navDbArtifact = selectedArtifact,
                            ) to NavDbOpenReport(
                                statuses = finish.openResult.statuses.map { it.toStatus() },
                            )
                        }
                        "need_resources" -> {
                            for (resource in parseCoreResourceRequests(outcome)) {
                                val bytes = try {
                                    readInstalledArtifactResource(artifactsByFilename, resource.address)
                                } catch (error: Throwable) {
                                    if (resource.optional) {
                                        Log.i(TAG, "optional resource ${resource.id} unavailable: ${error.message}")
                                        ByteArray(0)
                                    } else {
                                        throw error
                                    }
                                }
                                bridge.navDbOpenControllerIngestResource(controllerHandle, resource.id, bytes)
                            }
                        }
                        else -> error("unknown nav_db open state: $state")
                    }
                }
            } finally {
                bridge.navDbOpenControllerDestroy(controllerHandle)
            }
        }

        fun inspectCandidates(
            artifacts: List<InstalledPackageArtifact>,
            bridge: NativeBridge = NativeBindings,
            json: Json = Json {
                encodeDefaults = true
                ignoreUnknownKeys = true
            },
        ): NavDbOpenReport {
            val artifactsByFilename = artifacts.associateBy { it.filename }
            val controllerHandle = bridge.navDbOpenControllerCreate(
                json.encodeToString(artifacts.map { it.toWireCandidate() }),
            )
            try {
                while (true) {
                    val outcome = try {
                        json.parseToJsonElement(
                            bridge.navDbOpenControllerStep(controllerHandle),
                        ).jsonObject
                    } catch (_: Throwable) {
                        val statuses = json.decodeFromString<List<WireNavDbArtifactStatus>>(
                            bridge.navDbOpenControllerStatuses(controllerHandle),
                        )
                        return NavDbOpenReport(statuses = statuses.map { it.toStatus() })
                    }
                    when (val state = outcome.getValue("state").jsonPrimitive.content) {
                        "complete" -> {
                            val result = json.decodeFromJsonElement<WireNavDbOpenResult>(
                                outcome.getValue("result"),
                            )
                            return NavDbOpenReport(statuses = result.statuses.map { it.toStatus() })
                        }
                        "need_resources" -> {
                            for (resource in parseCoreResourceRequests(outcome)) {
                                val bytes = try {
                                    readInstalledArtifactResource(artifactsByFilename, resource.address)
                                } catch (error: Throwable) {
                                    if (resource.optional) {
                                        Log.i(TAG, "optional resource ${resource.id} unavailable: ${error.message}")
                                        ByteArray(0)
                                    } else {
                                        throw error
                                    }
                                }
                                bridge.navDbOpenControllerIngestResource(controllerHandle, resource.id, bytes)
                            }
                        }
                        else -> error("unknown nav_db open state: $state")
                    }
                }
            } finally {
                bridge.navDbOpenControllerDestroy(controllerHandle)
            }
        }

        private fun readInstalledArtifactResource(
            artifactsByFilename: Map<String, InstalledPackageArtifact>,
            address: String,
        ): ByteArray {
            val withoutScheme = address.removePrefix("installed-artifact://")
            require(withoutScheme != address) { "unsupported installed artifact address: $address" }
            val filename = withoutScheme.substringBefore('/')
            val memberPath = withoutScheme.substringAfter('/', missingDelimiterValue = "")
            require(filename.isNotBlank() && memberPath.isNotBlank()) {
                "invalid installed artifact address: $address"
            }
            val artifact = artifactsByFilename[filename]
                ?: error("missing installed artifact $filename")
            return InstalledPackages.readZipEntryBytes(artifact.file, memberPath)
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

        private fun WireNavDbArtifactStatus.toStatus(): NavDbArtifactStatus =
            NavDbArtifactStatus(
                packageId = packageId,
                filename = filename,
                readable = readable,
                message = message,
            )

        private fun InstalledPackageArtifact.toWireCandidate(): WireNavDbArtifactCandidate =
            WireNavDbArtifactCandidate(
                packageId = artifactId,
                filename = filename,
            )
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

    private fun ensureNavKvResource(resource: CoreResourceRequest) {
        val pageIndex = resource.id.removePrefix("nav_kv/page/").toIntOrNull()
            ?: error("unsupported nav_kv resource id: ${resource.id}")
        ensurePage(pageIndex, resource)
    }

    @Synchronized
    private fun ensurePage(pageIndex: Int, resource: CoreResourceRequest) {
        if (!loadedPages.add(pageIndex)) {
            return
        }
        val startMs = SystemClock.elapsedRealtime()
        val memberPath = resource.address.removePrefix("nav-kv://").also {
            require(it != resource.address && it.isNotBlank()) {
                "unsupported nav_kv resource address: ${resource.address}"
            }
        }
        val pageBytes = InstalledPackages.readZipEntryBytes(navDbArtifact.file, memberPath)
        bridge.navKvInsertResource(handle, resource.id, pageBytes)
        val elapsedMs = SystemClock.elapsedRealtime() - startMs
        if (elapsedMs >= 10) {
            Log.i(TAG, "ensurePage($pageIndex) took ${elapsedMs}ms")
        }
    }

    override fun close() {
        bridge.navKvDestroy(handle)
    }
}
