// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app.domain

import android.content.Context
import java.util.concurrent.locks.ReentrantReadWriteLock
import kotlin.concurrent.read
import kotlin.concurrent.write
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
    val maxResponseBytes: Long? = null,
)

sealed class CoreResourceSource {
    data class PublicUrl(val url: String) : CoreResourceSource()
    data class PackageMember(
        val packageId: String,
        val filename: String,
        val memberPath: String,
    ) : CoreResourceSource()
    data class LiveFeedPackageMember(
        val product: String,
        val version: String,
        val blobSha256: String,
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

private data class CoreSessionResourceEffect(
    val resource: CoreResourceRequest,
    val completionInvalidations: List<String>,
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
        maxResponseBytes = resource["max_response_bytes"]?.jsonPrimitive?.content?.toLongOrNull(),
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
        "live_feed_package_member" -> CoreResourceSource.LiveFeedPackageMember(
            product = source.getValue("product").jsonPrimitive.content,
            version = source.getValue("version").jsonPrimitive.content,
            blobSha256 = source.getValue("blob_sha256").jsonPrimitive.content,
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

@Serializable
private data class WireInstalledArtifact(
    @SerialName("artifact_id")
    val artifactId: String,
    val filename: String,
    @SerialName("size_bytes")
    val sizeBytes: Long? = null,
    @SerialName("checksum_sha256")
    val checksumSha256: String? = null,
    @SerialName("family_id")
    val familyId: String? = null,
    @SerialName("region_id")
    val regionId: String? = null,
    @SerialName("chart_package_tier")
    val chartPackageTier: String? = null,
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
)

private data class NavKvBackend(
    val handle: Long,
    val navDbArtifact: InstalledPackageArtifact,
    val loadedPages: MutableSet<Int> = mutableSetOf(),
    val pageLock: Any = Any(),
)

class NavKvStore private constructor(
    private val bridge: NativeBridge,
    private val json: Json,
    initialBackend: NavKvBackend,
) : AutoCloseable {
    private val backendLock = ReentrantReadWriteLock(true)
    private var backend = initialBackend
    private var closed = false
    private val attachedSessionHandles = linkedSetOf<Long>()

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
            openInstalledArtifacts(listOf(artifact), libraryCacheJson = "", bridge, json)

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

        fun openInstalledArtifacts(
            artifacts: List<InstalledPackageArtifact>,
            libraryCacheJson: String,
            bridge: NativeBridge = NativeBindings,
            json: Json = Json {
                encodeDefaults = true
                ignoreUnknownKeys = true
            },
        ): NavKvStore = NavKvStore(
            bridge = bridge,
            json = json,
            initialBackend = openBackend(artifacts, libraryCacheJson, bridge, json),
        )

        private fun openBackend(
            artifacts: List<InstalledPackageArtifact>,
            libraryCacheJson: String,
            bridge: NativeBridge,
            json: Json,
        ): NavKvBackend {
            val artifactsByFilename = artifacts.associateBy { it.filename }
            val controllerHandle = bridge.navDbOpenControllerCreateFromInstalledArtifacts(
                json.encodeToString(artifacts.map { it.toWireInstalledArtifact() }),
                libraryCacheJson,
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
                            return NavKvBackend(
                                handle = finish.navKvHandle,
                                navDbArtifact = selectedArtifact,
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

        private fun InstalledPackageArtifact.toWireInstalledArtifact(): WireInstalledArtifact =
            WireInstalledArtifact(
                artifactId = artifactId,
                filename = filename,
                sizeBytes = sizeBytes,
                checksumSha256 = checksumSha256,
                familyId = familyId,
                regionId = regionId,
                chartPackageTier = chartPackageTier,
            )
    }

    fun replaceInstalledArtifacts(
        artifacts: List<InstalledPackageArtifact>,
        libraryCacheJson: String,
        sessionHandle: Long,
        plannedGcFilenames: Set<String> = emptySet(),
    ): PagedSessionOperationResult {
        val nextBackend = openBackend(artifacts, libraryCacheJson, bridge, json)
        var previousBackend: NavKvBackend? = null
        try {
            val installedPackageIdsJson = json.encodeToString(
                artifacts
                    .filterNot { it.filename in plannedGcFilenames }
                    .map { it.artifactId }
                    .distinct()
                    .sorted(),
            )
            while (true) {
                val outcome = backendLock.write {
                    check(!closed) { "nav_kv store is closed" }
                    check(attachedSessionHandles == setOf(sessionHandle)) {
                        "NAVDB advance requires exactly the attached target session"
                    }
                    val parsed = json.parseToJsonElement(
                        bridge.advanceNavKvStoreInSessionJson(
                            nextBackend.handle,
                            sessionHandle,
                            installedPackageIdsJson,
                        ),
                    ).jsonObject
                    if (parsed.getValue("state").jsonPrimitive.content == "complete") {
                        when (parsed.getValue("result").jsonObject
                            .getValue("disposition").jsonPrimitive.content) {
                            "adopted" -> {
                                previousBackend = backend
                                backend = nextBackend
                            }
                            "rejected" -> Unit
                            else -> error("unknown NAVDB advance disposition")
                        }
                    }
                    parsed
                }
                when (val state = outcome.getValue("state").jsonPrimitive.content) {
                    "complete" -> {
                        val result = PagedSessionOperationResult(
                            result = outcome["result"] ?: JsonNull,
                            invalidations = outcome["invalidations"]
                                ?.jsonArray
                                ?.map { it.jsonPrimitive.content }
                                ?: emptyList(),
                        )
                        if (previousBackend == null) {
                            bridge.navKvDestroy(nextBackend.handle)
                        } else {
                            previousBackend?.let { bridge.navKvDestroy(it.handle) }
                        }
                        return result
                    }
                    "need_resources" -> {
                        var loadedAnyResource = false
                        for (resource in parseCoreResourceRequests(outcome)) {
                            require(resource.id.startsWith("nav_kv/page/")) {
                                "NAVDB advance requested non-NAVKV resource ${resource.id}"
                            }
                            loadedAnyResource = ensureNavKvResource(nextBackend, resource) || loadedAnyResource
                        }
                        check(loadedAnyResource) {
                            "NAVDB advance requested only already-loaded resources"
                        }
                    }
                    else -> error("unknown NAVDB advance state: $state")
                }
            }
        } catch (error: Throwable) {
            if (previousBackend == null) {
                bridge.navKvDestroy(nextBackend.handle)
            }
            throw error
        }
    }

    fun runCoreOperationElement(operation: JsonObject): JsonElement = backendLock.read {
        check(!closed) { "nav_kv store is closed" }
        runCoreOperationElement(backend, operation)
    }

    private fun runCoreOperationElement(activeBackend: NavKvBackend, operation: JsonObject): JsonElement {
        while (true) {
            val outcome = json.parseToJsonElement(
                bridge.coreHadOperation(activeBackend.handle, operation.toString()),
            ).jsonObject
            return when (val state = outcome.getValue("state").jsonPrimitive.content) {
                "complete" -> outcome["result"] ?: JsonNull
                "need_resources" -> {
                    var loadedAnyResource = false
                    for (resource in parseCoreResourceRequests(outcome)) {
                        loadedAnyResource = ensureNavKvResource(activeBackend, resource) || loadedAnyResource
                    }
                    if (!loadedAnyResource) {
                        error("HAD operation requested only already-loaded resources")
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
        resumeSnapshot: (() -> String)? = null,
        operation: () -> String,
    ): PagedSessionOperationResult = backendLock.read {
        check(!closed) { "nav_kv store is closed" }
        runPagedSessionOperation(
            activeBackend = backend,
            fetchSessionResource = fetchSessionResource,
            ingestSessionResource = ingestSessionResource,
            resumeSnapshot = resumeSnapshot,
            operation = operation,
        )
    }

    private fun runPagedSessionOperation(
        activeBackend: NavKvBackend,
        fetchSessionResource: ((CoreResourceRequest) -> ByteArray)?,
        ingestSessionResource: ((CoreResourceRequest, ByteArray) -> Unit)?,
        resumeSnapshot: (() -> String)?,
        operation: () -> String,
    ): PagedSessionOperationResult {
        var activeOperation = operation
        val pendingInvalidations = linkedSetOf<String>()
        // A peer may install a requested NAVKV page after core reports the request but
        // before this operation reaches ensurePage. An already-present page therefore
        // means the operation should resume, not that the request is invalid. Track the
        // full request cycle instead: a request may be observed once regardless of who
        // satisfied it, but seeing the same request again proves core made no progress.
        val seenResourceRequests = mutableSetOf<Pair<String, List<String>>>()
        while (true) {
            val outcome = json.parseToJsonElement(activeOperation()).jsonObject
            return when (val state = outcome.getValue("state").jsonPrimitive.content) {
                "complete" -> {
                    PagedSessionOperationResult(
                        result = outcome["result"] ?: JsonNull,
                        invalidations = (pendingInvalidations + (
                            outcome["invalidations"]
                                ?.jsonArray
                                ?.map { it.jsonPrimitive.content }
                                ?: emptyList()
                            )).toList(),
                    )
                }
                "need_resources", "need_snapshot_resources" -> {
                    if (state == "need_snapshot_resources") {
                        pendingInvalidations += outcome["invalidations"]
                            ?.jsonArray
                            ?.map { it.jsonPrimitive.content }
                            ?: emptyList()
                        activeOperation = resumeSnapshot
                            ?: error("committed session mutation requires a snapshot-resume operation")
                    }
                    val resources = parseCoreResourceRequests(outcome)
                    check(resources.isNotEmpty()) {
                        "paged session operation requested no resources"
                    }
                    val requestFingerprint = state to resources.map { it.id }.sorted()
                    check(seenResourceRequests.add(requestFingerprint)) {
                        "paged session operation repeated an already-satisfied resource request: " +
                            requestFingerprint.second.joinToString()
                    }
                    for (resource in resources) {
                        if (resource.id.startsWith("nav_kv/page/")) {
                            ensureNavKvResource(activeBackend, resource)
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

    fun pumpSessionResourceEffects(
        drainSessionResourceEffects: () -> String,
        fetchSessionResource: ((CoreResourceRequest) -> ByteArray)? = null,
        ingestSessionResource: ((CoreResourceRequest, ByteArray) -> Unit)? = null,
        reportSessionResourceFailure: ((CoreResourceRequest, Throwable) -> Unit)? = null,
    ): List<String> = backendLock.read {
        check(!closed) { "nav_kv store is closed" }
        pumpSessionResourceEffects(
            activeBackend = backend,
            drainSessionResourceEffects = drainSessionResourceEffects,
            fetchSessionResource = fetchSessionResource,
            ingestSessionResource = ingestSessionResource,
            reportSessionResourceFailure = reportSessionResourceFailure,
        )
    }

    private fun pumpSessionResourceEffects(
        activeBackend: NavKvBackend,
        drainSessionResourceEffects: () -> String,
        fetchSessionResource: ((CoreResourceRequest) -> ByteArray)?,
        ingestSessionResource: ((CoreResourceRequest, ByteArray) -> Unit)?,
        reportSessionResourceFailure: ((CoreResourceRequest, Throwable) -> Unit)?,
    ): List<String> {
        val invalidations = linkedSetOf<String>()
        while (true) {
            val effects = parseCoreSessionResourceEffects(drainSessionResourceEffects())
            if (effects.isEmpty()) {
                return invalidations.toList()
            }
            for (effect in effects) {
                try {
                    ensureSessionEffectResource(
                        activeBackend = activeBackend,
                        resource = effect.resource,
                        fetchSessionResource = fetchSessionResource,
                        ingestSessionResource = ingestSessionResource,
                    )
                    invalidations.addAll(effect.completionInvalidations)
                } catch (error: Throwable) {
                    reportSessionResourceFailure?.invoke(effect.resource, error)
                    invalidations.addAll(effect.completionInvalidations)
                    diagnosticLogInfo(TAG) {
                        "session resource effect failed resource=${effect.resource.id}: ${error.message}"
                    }
                }
            }
        }
    }

    private fun parseCoreSessionResourceEffects(effectsJson: String): List<CoreSessionResourceEffect> =
        json.parseToJsonElement(effectsJson).jsonArray.map { element ->
            val effect = element.jsonObject
            CoreSessionResourceEffect(
                resource = parseCoreResourceRequest(effect.getValue("resource").jsonObject),
                completionInvalidations = effect["completion_invalidations"]
                    ?.jsonArray
                    ?.map { it.jsonPrimitive.content }
                    ?: emptyList(),
            )
        }

    private fun ensureSessionEffectResource(
        activeBackend: NavKvBackend,
        resource: CoreResourceRequest,
        fetchSessionResource: ((CoreResourceRequest) -> ByteArray)?,
        ingestSessionResource: ((CoreResourceRequest, ByteArray) -> Unit)?,
    ) {
        if (resource.id.startsWith("nav_kv/page/")) {
            ensureNavKvResource(activeBackend, resource)
            return
        }
        val fetch = fetchSessionResource
            ?: error("session resource effect requested without fetcher: ${resource.id}")
        val ingest = ingestSessionResource
            ?: error("session resource effect requested without ingester: ${resource.id}")
        val bytes = try {
            fetch(resource)
        } catch (error: Throwable) {
            if (resource.optional) {
                diagnosticLogInfo(TAG) {
                    "optional session resource effect ${resource.id} unavailable: ${error.message}"
                }
                ByteArray(0)
            } else {
                throw error
            }
        }
        ingest(resource, bytes)
    }

    fun <T> runCoreOperation(operation: JsonObject, serializer: KSerializer<T>): T =
        json.decodeFromJsonElement(serializer, runCoreOperationElement(operation))

    fun attachToSession(sessionHandle: Long) {
        backendLock.write {
            check(!closed) { "nav_kv store is closed" }
            bridge.attachNavKvStoreToSession(backend.handle, sessionHandle)
            attachedSessionHandles.add(sessionHandle)
        }
    }

    private fun ensureNavKvResource(activeBackend: NavKvBackend, resource: CoreResourceRequest): Boolean {
        val pageIndex = resource.id.removePrefix("nav_kv/page/").toIntOrNull()
            ?: error("unsupported nav_kv resource id: ${resource.id}")
        return ensurePage(activeBackend, pageIndex, resource)
    }

    private fun ensurePage(
        activeBackend: NavKvBackend,
        pageIndex: Int,
        resource: CoreResourceRequest,
    ): Boolean = synchronized(activeBackend.pageLock) {
        if (activeBackend.loadedPages.contains(pageIndex)) {
            return@synchronized false
        }
        val startMs = System.nanoTime() / 1_000_000L
        val source = resource.source
        require(source is CoreResourceSource.NavKvMember && source.memberPath.isNotBlank()) {
            "Android nav_kv paging expected nav_kv_member for ${resource.id}, got ${source.kindForLog()}"
        }
        try {
            val pageBytes = InstalledPackages.readZipEntryBytes(activeBackend.navDbArtifact.file, source.memberPath)
            bridge.navKvInsertResource(activeBackend.handle, resource.id, pageBytes)
            activeBackend.loadedPages.add(pageIndex)
        } catch (error: Throwable) {
            diagnosticLogInfo(TAG) {
                "ensurePage($pageIndex) failed resource=${resource.id} source=${source.describeForLog()}: ${error.message}"
            }
            throw error
        }
        val elapsedMs = (System.nanoTime() / 1_000_000L) - startMs
        if (elapsedMs >= 10) {
            perfLogInfo(TAG) { "ensurePage($pageIndex) took ${elapsedMs}ms" }
        }
        true
    }

    fun debugDropAttachedSessionPages() {
        backendLock.write {
            check(!closed) { "nav_kv store is closed" }
            bridge.debugDropNavKvPagesForAttachedSessions(backend.handle)
            backend.loadedPages.clear()
        }
    }

    override fun close() {
        val closedBackend = backendLock.write {
            if (closed) return
            closed = true
            attachedSessionHandles.clear()
            backend
        }
        bridge.navKvDestroy(closedBackend.handle)
    }
}

fun CoreResourceSource.kindForLog(): String =
    when (this) {
        is CoreResourceSource.PublicUrl -> "public_url"
        is CoreResourceSource.PackageMember -> "package_member"
        is CoreResourceSource.LiveFeedPackageMember -> "live_feed_package_member"
        is CoreResourceSource.InstalledArtifactMember -> "installed_artifact_member"
        is CoreResourceSource.NavKvMember -> "nav_kv_member"
        is CoreResourceSource.Unavailable -> "unavailable"
    }

fun CoreResourceSource.describeForLog(): String =
    when (this) {
        is CoreResourceSource.PublicUrl -> "public_url url=$url"
        is CoreResourceSource.PackageMember ->
            "package_member packageId=$packageId filename=$filename memberPath=$memberPath"
        is CoreResourceSource.LiveFeedPackageMember ->
            "live_feed_package_member product=$product version=$version memberPath=$memberPath"
        is CoreResourceSource.InstalledArtifactMember ->
            "installed_artifact_member filename=$filename memberPath=$memberPath"
        is CoreResourceSource.NavKvMember -> "nav_kv_member memberPath=$memberPath"
        is CoreResourceSource.Unavailable -> "unavailable message=$message"
    }
