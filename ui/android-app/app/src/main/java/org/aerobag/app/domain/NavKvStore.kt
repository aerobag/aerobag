package org.aerobag.app.domain

import android.content.Context
import android.os.SystemClock
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
import org.aerobag.app.diagnosticLogInfo
import org.aerobag.app.perfLogInfo

data class CoreResourceRequest(
    val id: String,
    val source: CoreResourceSource,
    val optional: Boolean,
)

sealed class CoreResourceSource {
    data class PublicUrl(val url: String) : CoreResourceSource()
    data class PackageMember(
        val packageId: String,
        val filename: String,
        val memberPath: String,
    ) : CoreResourceSource()
    data class InstalledArtifactMember(
        val filename: String,
        val memberPath: String,
    ) : CoreResourceSource()
    data class NavKvMember(val memberPath: String) : CoreResourceSource()
    data class Unavailable(val message: String) : CoreResourceSource()
}

data class PagedSessionOperationResult(
    val result: JsonElement,
    val invalidations: List<String>,
)

fun parseCoreResourceRequests(outcome: JsonObject): List<CoreResourceRequest> =
    outcome.getValue("resources").jsonArray.map { element ->
        parseCoreResourceRequest(element.jsonObject)
    }

fun parseCoreResourceRequest(resource: JsonObject): CoreResourceRequest =
    CoreResourceRequest(
        id = resource.getValue("id").jsonPrimitive.content,
        source = parseCoreResourceSource(resource.getValue("source").jsonObject),
        optional = resource["optional"]?.jsonPrimitive?.content?.toBooleanStrictOrNull() ?: false,
    )

fun parseCoreResourceSource(source: JsonObject): CoreResourceSource =
    when (val kind = source.getValue("kind").jsonPrimitive.content) {
        "public_url" -> CoreResourceSource.PublicUrl(
            url = source.getValue("url").jsonPrimitive.content,
        )
        "package_member" -> CoreResourceSource.PackageMember(
            packageId = source.getValue("package_id").jsonPrimitive.content,
            filename = source.getValue("filename").jsonPrimitive.content,
            memberPath = source.getValue("member_path").jsonPrimitive.content,
        )
        "installed_artifact_member" -> CoreResourceSource.InstalledArtifactMember(
            filename = source.getValue("filename").jsonPrimitive.content,
            memberPath = source.getValue("member_path").jsonPrimitive.content,
        )
        "nav_kv_member" -> CoreResourceSource.NavKvMember(
            memberPath = source.getValue("member_path").jsonPrimitive.content,
        )
        "unavailable" -> CoreResourceSource.Unavailable(
            message = source.getValue("message").jsonPrimitive.content,
        )
        else -> error("unknown core resource source kind: $kind")
    }

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
                                    readInstalledArtifactResource(artifactsByFilename, resource)
                                } catch (error: Throwable) {
                                    if (resource.optional) {
                                        diagnosticLogInfo(TAG) {
                                            "optional resource ${resource.id} unavailable: ${error.message}"
                                        }
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
                                    readInstalledArtifactResource(artifactsByFilename, resource)
                                } catch (error: Throwable) {
                                    if (resource.optional) {
                                        diagnosticLogInfo(TAG) {
                                            "optional resource ${resource.id} unavailable: ${error.message}"
                                        }
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
            resource: CoreResourceRequest,
        ): ByteArray {
            val source = resource.source
            if (source is CoreResourceSource.Unavailable) {
                error("core resource ${resource.id} is unavailable: ${source.message}")
            }
            require(source is CoreResourceSource.InstalledArtifactMember) {
                "Android nav_db open expected installed_artifact_member for ${resource.id}, got ${source.kindForLog()}"
            }
            val artifact = artifactsByFilename[source.filename]
                ?: error("missing installed artifact ${source.filename}")
            return InstalledPackages.readZipEntryBytes(artifact.file, source.memberPath)
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
    ): JsonElement = runPagedSessionOperation(
        fetchSessionResource = fetchSessionResource,
        ingestSessionResource = ingestSessionResource,
        operation = operation,
    ).result

    fun runPagedSessionOperation(
        fetchSessionResource: ((CoreResourceRequest) -> ByteArray)? = null,
        ingestSessionResource: ((CoreResourceRequest, ByteArray) -> Unit)? = null,
        operation: () -> String,
    ): PagedSessionOperationResult {
        while (true) {
            val outcome = json.parseToJsonElement(operation()).jsonObject
            return when (val state = outcome.getValue("state").jsonPrimitive.content) {
                "complete" -> PagedSessionOperationResult(
                    result = outcome["result"] ?: JsonNull,
                    invalidations = outcome["invalidations"]
                        ?.jsonArray
                        ?.map { it.jsonPrimitive.content }
                        ?: emptyList(),
                )
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
                                    diagnosticLogInfo(TAG) {
                                        "optional resource ${resource.id} unavailable: ${error.message}"
                                    }
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
        val source = resource.source
        require(source is CoreResourceSource.NavKvMember && source.memberPath.isNotBlank()) {
            "Android nav_kv paging expected nav_kv_member for ${resource.id}, got ${source.kindForLog()}"
        }
        val pageBytes = InstalledPackages.readZipEntryBytes(navDbArtifact.file, source.memberPath)
        bridge.navKvInsertResource(handle, resource.id, pageBytes)
        val elapsedMs = SystemClock.elapsedRealtime() - startMs
        if (elapsedMs >= 10) {
            perfLogInfo(TAG) { "ensurePage($pageIndex) took ${elapsedMs}ms" }
        }
    }

    override fun close() {
        bridge.navKvDestroy(handle)
    }
}

fun CoreResourceSource.kindForLog(): String =
    when (this) {
        is CoreResourceSource.PublicUrl -> "public_url"
        is CoreResourceSource.PackageMember -> "package_member"
        is CoreResourceSource.InstalledArtifactMember -> "installed_artifact_member"
        is CoreResourceSource.NavKvMember -> "nav_kv_member"
        is CoreResourceSource.Unavailable -> "unavailable"
    }

fun CoreResourceSource.describeForLog(): String =
    when (this) {
        is CoreResourceSource.PublicUrl -> "public_url url=$url"
        is CoreResourceSource.PackageMember ->
            "package_member packageId=$packageId filename=$filename memberPath=$memberPath"
        is CoreResourceSource.InstalledArtifactMember ->
            "installed_artifact_member filename=$filename memberPath=$memberPath"
        is CoreResourceSource.NavKvMember -> "nav_kv_member memberPath=$memberPath"
        is CoreResourceSource.Unavailable -> "unavailable message=$message"
    }
