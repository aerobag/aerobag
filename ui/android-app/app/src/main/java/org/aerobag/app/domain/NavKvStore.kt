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
    val resumedSnapshot: Boolean = false,
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
    private val resourceFrontierLoader: ResourceFrontierLoader,
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
        ): NavKvStore {
            val resourceFrontierLoader = ResourceFrontierLoader()
            return try {
                NavKvStore(
                    bridge = bridge,
                    json = json,
                    initialBackend = openBackend(
                        artifacts,
                        libraryCacheJson,
                        bridge,
                        json,
                        resourceFrontierLoader,
                    ),
                    resourceFrontierLoader = resourceFrontierLoader,
                )
            } catch (error: Throwable) {
                resourceFrontierLoader.close()
                throw error
            }
        }

        private fun openBackend(
            artifacts: List<InstalledPackageArtifact>,
            libraryCacheJson: String,
            bridge: NativeBridge,
            json: Json,
            resourceFrontierLoader: ResourceFrontierLoader,
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
                            val resources = parseCoreResourceRequests(outcome)
                            val batch = resourceFrontierLoader.load(
                                resources.map { resource ->
                                    { readInstalledArtifactResource(artifactsByFilename, resource) }
                                },
                            )
                            for ((resource, loaded) in resources.zip(batch.outcomes)) {
                                val bytes = loaded.bytes ?: if (resource.optional) {
                                    diagnosticLogInfo(TAG) {
                                        "optional resource ${resource.id} unavailable: ${loaded.error?.message}"
                                    }
                                    ByteArray(0)
                                } else {
                                    throw loaded.error ?: error("resource ${resource.id} failed without an error")
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
        val nextBackend = openBackend(
            artifacts,
            libraryCacheJson,
            bridge,
            json,
            resourceFrontierLoader,
        )
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
                        val loadedAnyResource = loadNavKvFrontier(
                            nextBackend,
                            parseCoreResourceRequests(outcome),
                        )
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
                    val loadedAnyResource = loadNavKvFrontier(
                        activeBackend,
                        parseCoreResourceRequests(outcome),
                    )
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
        metrics: PagedSessionOperationMetrics? = null,
        operation: () -> String,
    ): JsonElement = runPagedSessionOperation(
        fetchSessionResource = fetchSessionResource,
        ingestSessionResource = ingestSessionResource,
        metrics = metrics,
        operation = operation,
    ).result

    fun runPagedSessionOperation(
        fetchSessionResource: ((CoreResourceRequest) -> ByteArray)? = null,
        ingestSessionResource: ((CoreResourceRequest, ByteArray) -> Unit)? = null,
        resumeSnapshot: (() -> String)? = null,
        metrics: PagedSessionOperationMetrics? = null,
        operation: () -> String,
    ): PagedSessionOperationResult = backendLock.read {
        check(!closed) { "nav_kv store is closed" }
        runPagedSessionOperation(
            activeBackend = backend,
            fetchSessionResource = fetchSessionResource,
            ingestSessionResource = ingestSessionResource,
            resumeSnapshot = resumeSnapshot,
            metrics = metrics,
            operation = operation,
        )
    }

    private fun runPagedSessionOperation(
        activeBackend: NavKvBackend,
        fetchSessionResource: ((CoreResourceRequest) -> ByteArray)?,
        ingestSessionResource: ((CoreResourceRequest, ByteArray) -> Unit)?,
        resumeSnapshot: (() -> String)?,
        metrics: PagedSessionOperationMetrics?,
        operation: () -> String,
    ): PagedSessionOperationResult {
        var activeOperation = operation
        var resumedSnapshot = false
        val pendingInvalidations = linkedSetOf<String>()
        // A peer may install a requested NAVKV page after core reports the request but
        // before this operation reaches ensurePage. An already-present page therefore
        // means the operation should resume, not that the request is invalid. Track the
        // full request cycle instead: a request may be observed once regardless of who
        // satisfied it, but seeing the same request again proves core made no progress.
        val seenResourceRequests = mutableSetOf<Pair<String, List<String>>>()
        while (true) {
            val outcomeJson = metrics?.measureCoreCall(activeOperation) ?: activeOperation()
            val outcome = json.parseToJsonElement(outcomeJson).jsonObject
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
                        resumedSnapshot = resumedSnapshot,
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
                        resumedSnapshot = true
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
                    val roundIndex = metrics?.beginResourceRound(resources)
                    try {
                        val loads = resources.mapNotNull { resource ->
                            prepareResourceLoad(
                                activeBackend = activeBackend,
                                resource = resource,
                                fetchSessionResource = fetchSessionResource,
                                metrics = metrics,
                            )
                        }
                        val batch = resourceFrontierLoader.load(loads.map { it.load })
                        if (roundIndex != null) {
                            metrics?.recordResourceBatch(roundIndex, batch)
                        }
                        for ((load, loaded) in loads.zip(batch.outcomes)) {
                            ingestResourceLoad(
                                activeBackend = activeBackend,
                                load = load,
                                loaded = loaded,
                                ingestSessionResource = ingestSessionResource,
                                metrics = metrics,
                            )
                        }
                    } finally {
                        if (roundIndex != null) {
                            metrics.finishResourceRound(roundIndex)
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
            val prepared = effects.map { effect ->
                try {
                    PreparedSessionResourceEffect(
                        effect = effect,
                        load = prepareResourceLoad(
                            activeBackend = activeBackend,
                            resource = effect.resource,
                            fetchSessionResource = fetchSessionResource,
                            metrics = null,
                        ),
                    )
                } catch (error: Throwable) {
                    PreparedSessionResourceEffect(effect = effect, error = error)
                }
            }
            val loads = prepared.mapNotNull(PreparedSessionResourceEffect::load)
            val batch = resourceFrontierLoader.load(loads.map(PendingResourceLoad::load))
            val loadedResults = batch.outcomes.iterator()
            for (preparedEffect in prepared) {
                val effect = preparedEffect.effect
                val error = preparedEffect.error ?: preparedEffect.load?.let { load ->
                    val loaded = loadedResults.next()
                    runCatching {
                        ingestResourceLoad(
                            activeBackend = activeBackend,
                            load = load,
                            loaded = loaded,
                            ingestSessionResource = ingestSessionResource,
                            metrics = null,
                        )
                    }.exceptionOrNull()
                }
                if (error != null) {
                    reportSessionResourceFailure?.invoke(effect.resource, error)
                    diagnosticLogInfo(TAG) {
                        "session resource effect failed resource=${effect.resource.id}: ${error.message}"
                    }
                }
                invalidations.addAll(effect.completionInvalidations)
            }
        }
    }

    private data class PreparedSessionResourceEffect(
        val effect: CoreSessionResourceEffect,
        val load: PendingResourceLoad? = null,
        val error: Throwable? = null,
    )

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

    fun <T> runCoreOperation(operation: JsonObject, serializer: KSerializer<T>): T =
        json.decodeFromJsonElement(serializer, runCoreOperationElement(operation))

    fun attachToSession(sessionHandle: Long) {
        backendLock.write {
            check(!closed) { "nav_kv store is closed" }
            bridge.attachNavKvStoreToSession(backend.handle, sessionHandle)
            attachedSessionHandles.add(sessionHandle)
        }
    }

    private data class PendingResourceLoad(
        val resource: CoreResourceRequest,
        val navPageIndex: Int?,
        val load: () -> ByteArray,
    )

    private fun prepareResourceLoad(
        activeBackend: NavKvBackend,
        resource: CoreResourceRequest,
        fetchSessionResource: ((CoreResourceRequest) -> ByteArray)?,
        metrics: PagedSessionOperationMetrics?,
    ): PendingResourceLoad? {
        if (!resource.id.startsWith("nav_kv/page/")) {
            val fetch = fetchSessionResource
                ?: error("session resource requested without fetcher: ${resource.id}")
            return PendingResourceLoad(resource, navPageIndex = null) { fetch(resource) }
        }
        val pageIndex = resource.id.removePrefix("nav_kv/page/").toIntOrNull()
            ?: error("unsupported nav_kv resource id: ${resource.id}")
        val source = resource.source
        require(source is CoreResourceSource.NavKvMember && source.memberPath.isNotBlank()) {
            "Android nav_kv paging expected nav_kv_member for ${resource.id}, got ${source.kindForLog()}"
        }
        synchronized(activeBackend.pageLock) {
            if (activeBackend.loadedPages.contains(pageIndex)) {
                metrics?.recordResourceCacheHit()
                return null
            }
        }
        return PendingResourceLoad(resource, navPageIndex = pageIndex) {
            InstalledPackages.readZipEntryBytes(activeBackend.navDbArtifact.file, source.memberPath)
        }
    }

    private fun ingestResourceLoad(
        activeBackend: NavKvBackend,
        load: PendingResourceLoad,
        loaded: ResourceFrontierLoadOutcome,
        ingestSessionResource: ((CoreResourceRequest, ByteArray) -> Unit)?,
        metrics: PagedSessionOperationMetrics?,
    ) {
        val resource = load.resource
        val bytes = loaded.bytes ?: if (resource.optional) {
            diagnosticLogInfo(TAG) {
                "optional resource ${resource.id} unavailable: ${loaded.error?.message}"
            }
            ByteArray(0)
        } else {
            throw loaded.error ?: error("resource ${resource.id} failed without an error")
        }
        val pageIndex = load.navPageIndex
        if (pageIndex == null) {
            val ingest = ingestSessionResource
                ?: error("session resource requested without ingester: ${resource.id}")
            metrics?.measureResourceIngest {
                ingest(resource, bytes)
            } ?: ingest(resource, bytes)
            return
        }
        synchronized(activeBackend.pageLock) {
            if (activeBackend.loadedPages.contains(pageIndex)) {
                return
            }
            try {
                metrics?.measureResourceIngest {
                    bridge.navKvInsertResource(activeBackend.handle, resource.id, bytes)
                } ?: bridge.navKvInsertResource(activeBackend.handle, resource.id, bytes)
                activeBackend.loadedPages.add(pageIndex)
            } catch (error: Throwable) {
                diagnosticLogInfo(TAG) {
                    "ingest page $pageIndex failed resource=${resource.id} source=${resource.source.describeForLog()}: ${error.message}"
                }
                throw error
            }
        }
    }

    private fun loadNavKvFrontier(
        activeBackend: NavKvBackend,
        resources: List<CoreResourceRequest>,
        metrics: PagedSessionOperationMetrics? = null,
    ): Boolean {
        resources.forEach { resource ->
            require(resource.id.startsWith("nav_kv/page/")) {
                "NAVKV operation requested non-NAVKV resource ${resource.id}"
            }
        }
        val loads = resources.mapNotNull { resource ->
            prepareResourceLoad(
                activeBackend = activeBackend,
                resource = resource,
                fetchSessionResource = null,
                metrics = metrics,
            )
        }
        val batch = resourceFrontierLoader.load(loads.map(PendingResourceLoad::load))
        for ((load, loaded) in loads.zip(batch.outcomes)) {
            ingestResourceLoad(
                activeBackend = activeBackend,
                load = load,
                loaded = loaded,
                ingestSessionResource = null,
                metrics = metrics,
            )
        }
        return loads.isNotEmpty()
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
        try {
            bridge.navKvDestroy(closedBackend.handle)
        } finally {
            resourceFrontierLoader.close()
        }
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
