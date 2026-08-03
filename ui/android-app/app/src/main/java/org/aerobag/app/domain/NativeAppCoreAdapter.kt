// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app.domain

import android.util.Log
import kotlinx.serialization.builtins.ListSerializer
import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.decodeFromJsonElement
import kotlinx.serialization.json.encodeToJsonElement
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.put
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.aerobag.app.diagnosticLogInfo
import org.aerobag.app.generated.NexradOverlayQueryResult
import org.aerobag.app.generated.CloudAuthorizationRequest
import org.aerobag.app.generated.CloudAuthorizationResponse
import org.aerobag.app.generated.CloudHttpRequest
import org.aerobag.app.generated.CloudHttpResponse
import org.aerobag.app.generated.CloudEventStreamEvent
import org.aerobag.app.generated.CloudEventStreamPlan
import org.aerobag.app.generated.CloudUiActionId
import org.aerobag.app.generated.CloudUiFieldValue
import org.aerobag.app.generated.UiCloudPageState
import org.aerobag.app.generated.UiHomePageState
import java.time.ZoneId
import java.util.concurrent.Executors

data class VectorTileRequest(
    val layer: String,
    val z: Int,
    val x: Int,
    val y: Int,
)

data class AirspaceFeatureRequest(
    val id: String,
    val path: String,
)

@kotlinx.serialization.Serializable
data class PointVectorRecord(
    val id: String,
    val kind: String,
    val lat: Double,
    val lon: Double,
    val label: String,
    @kotlinx.serialization.SerialName("style_class")
    val styleClass: String,
    val towered: Boolean? = null,
    @kotlinx.serialization.SerialName("fuel_available")
    val fuelAvailable: Boolean? = null,
    @kotlinx.serialization.SerialName("has_paved_runway")
    val hasPavedRunway: Boolean? = null,
    val heliport: Boolean? = null,
    @kotlinx.serialization.SerialName("has_water_runway")
    val hasWaterRunway: Boolean? = null,
    @kotlinx.serialization.SerialName("longest_runway_length_ft")
    val longestRunwayLengthFt: Double? = null,
    @kotlinx.serialization.SerialName("longest_runway_heading_true_deg")
    val longestRunwayHeadingTrueDeg: Double? = null,
    @kotlinx.serialization.SerialName("elevation_msl_ft")
    val elevationMslFt: Double? = null,
)

@kotlinx.serialization.Serializable
data class PointTilePayload(
    @kotlinx.serialization.SerialName("schema_version")
    val schemaVersion: Int,
    val layer: String,
    val z: Int,
    val x: Int,
    val y: Int,
    val records: List<PointVectorRecord>,
)

data class VisibleMapFeature(
    val id: String,
    val kind: String,
    val label: String,
    val symbolKind: String,
    val styleClass: String,
    val obstacleVariant: String?,
    val obstacleTone: String?,
    val screenX: Double,
    val screenY: Double,
    val towered: Boolean,
    val fuelAvailable: Boolean,
    val hasPavedRunway: Boolean?,
    val heliport: Boolean?,
    val hasWaterRunway: Boolean?,
    val runwayLengthRatio: Double,
    val longestRunwayHeadingTrueDeg: Double?,
    val labelStyle: String = "default",
)

data class VisibleMetarFeature(
    val stationId: String,
    val screenX: Double,
    val screenY: Double,
    val flightCategory: String,
    val ceilingAmount: String,
)

data class VisiblePirepFeature(
    val id: String,
    val screenX: Double,
    val screenY: Double,
    val symbol: String,
    val icing: String,
    val turbulence: String,
)

data class AirspaceDisplayStroke(
    val colorKey: String,
    val widthPx: Double,
    val dashPx: List<Double>,
    val lineCap: String,
)

data class AirspaceDisplayStyle(
    val fillColorKey: String,
    val fillOpacity: Double,
    val strokes: List<AirspaceDisplayStroke>,
)

data class AirspaceScreenPoint(
    val x: Double,
    val y: Double,
)

data class AirspaceDisplaySubpath(
    val closed: Boolean,
    val points: List<AirspaceScreenPoint>,
)

data class AirspaceDisplayDecorationSegment(
    val x1: Double,
    val y1: Double,
    val x2: Double,
    val y2: Double,
)

data class AirspaceDisplayDecoration(
    val colorKey: String,
    val widthPx: Double,
    val lineCap: String,
    val paths: List<AirspaceDisplaySubpath>,
    val segments: List<AirspaceDisplayDecorationSegment>,
)

data class AirspaceDisplayPath(
    val id: String,
    val name: String,
    val styleKey: String,
    val style: AirspaceDisplayStyle,
    val paths: List<AirspaceDisplaySubpath>,
    val decorations: List<AirspaceDisplayDecoration>,
)

data class AirspaceDisplayLabel(
    val featureId: String,
    val glyph: AirspaceLimitGlyph,
    val screenX: Double,
    val screenY: Double,
)

data class OfflineRegionDisplay(
    val id: String,
    val kind: String,
    val regionId: String,
    val label: String,
    val colorKey: String,
    val points: List<AirspaceScreenPoint>,
    val labelX: Double,
    val labelY: Double,
)

data class AirspaceLimitGlyph(
    val upper: String,
    val lower: String,
    val styleKey: String,
    val colorKey: String,
)

data class NavSymbolFeature(
    val kind: String,
    val label: String,
    val symbolKind: String,
    val styleClass: String,
    val obstacleVariant: String?,
    val obstacleTone: String?,
    val towered: Boolean,
    val fuelAvailable: Boolean,
    val hasPavedRunway: Boolean?,
    val heliport: Boolean?,
    val hasWaterRunway: Boolean?,
    val runwayLengthRatio: Double,
    val longestRunwayHeadingTrueDeg: Double?,
)

enum class MapLayerId {
    WorldBasemap,
    Vectors,
    Metars,
    Nexrad,
    TerrainWarning,
    OfflineRegions,
}

data class UiMapLayerToggleState(
    val visible: Boolean,
    val enabled: Boolean,
    val disabledReason: String? = null,
)

data class UiMapLayerState(
    val worldBasemap: UiMapLayerToggleState,
    val vectors: UiMapLayerToggleState,
    val metars: UiMapLayerToggleState,
    val nexrad: UiMapLayerToggleState,
    val terrainWarning: UiMapLayerToggleState,
    val offlineRegions: UiMapLayerToggleState,
)

data class RasterMapUiState(
    val selectedMapId: String,
    val selectedMapLabel: String,
    val selectedFamilyId: String,
    val selectedFamilyLabel: String,
    val selectedFamilyLauncherLabel: String,
    val minZoom: Double,
    val maxZoom: Double,
    val initialViewport: MapViewportSeed,
    val familyOptions: List<MapFamilyOption>,
)

data class MapFamilyOption(
    val id: String,
    val label: String,
    val launcherLabel: String,
    val enabled: Boolean,
    val disabledReason: String? = null,
    val active: Boolean,
    val hasReferences: Boolean,
)

data class MapOverlayQueryResult(
    val visibleFeatures: List<VisibleMapFeature>,
    val flightPlanFeatures: List<VisibleMapFeature> = emptyList(),
    val visibleMetars: List<VisibleMetarFeature>,
    val visiblePireps: List<VisiblePirepFeature>,
    val airspacePaths: List<AirspaceDisplayPath>,
    val tfrPaths: List<AirspaceDisplayPath>,
    val airspaceLabels: List<AirspaceDisplayLabel>,
    val offlineRegions: List<OfflineRegionDisplay>,
)

data class MapSelectionQueryResult(
    val clickLat: Double,
    val clickLon: Double,
    val initialSelectedItemId: String?,
    val categories: List<MapSelectionCategory>,
)

data class MapSelectionForNavRefResult(
    val position: LatLonPoint,
    val targetZoom: Double,
    val selection: MapSelectionQueryResult,
    val selectedItemId: String?,
)

data class MapSelectionCategory(
    val id: String,
    val label: String,
    val items: List<MapSelectionItem>,
)

data class MapSelectionItem(
    val id: String,
    val label: String,
    val sublabel: String,
    val description: String?,
    val secondaryDescription: String?,
    val detailText: String?,
    val highlight: MapSelectionHighlight,
    val navRef: NavRef?,
    val symbolFeature: NavSymbolFeature?,
    val metarFeature: VisibleMetarFeature?,
    val pirepFeature: VisiblePirepFeature?,
    val airspaceIcon: AirspaceDisplayPath?,
    val actions: List<MapSelectionAction>,
)

sealed interface MapSelectionHighlight {
    data class FeatureRef(val id: String) : MapSelectionHighlight
    data class Metar(val stationId: String) : MapSelectionHighlight
    data class Pirep(val id: String) : MapSelectionHighlight
    data class OfflineRegion(val id: String) : MapSelectionHighlight
    data class Spot(val lat: Double, val lon: Double) : MapSelectionHighlight
}

data class MapSelectionAction(
    val id: String,
    val label: String,
    val enabled: Boolean,
    val displayOnly: Boolean,
    val detailText: String?,
    val detailTitle: String?,
    val detailStatus: MapSelectionDetailStatus?,
    val disabledReason: String?,
    val weatherDetail: WeatherDetailUiView?,
    val airportInfoAirportId: String?,
    val airspaceLimit: AirspaceLimitGlyph?,
    val sessionAction: String?,
    val flightPlanRowAction: MapSelectionFlightPlanRowAction?,
    val navigation: MapSelectionNavigationAction?,
)

data class MapSelectionDetailStatus(
    val text: String,
    val colorKey: String,
)

data class MapSelectionFlightPlanRowAction(
    val rowUid: String,
    val actionUid: String,
)

sealed interface MapSelectionNavigationAction {
    data class OpenPlateTarget(
        val airportId: String,
        val target: String,
        val chartId: String,
    ) : MapSelectionNavigationAction
}

sealed interface TerrainOverlayStatus {
    data object Hidden : TerrainOverlayStatus
    data object NoPosition : TerrainOverlayStatus
    data object NoAltitude : TerrainOverlayStatus
    data class TooManyTiles(val count: Int) : TerrainOverlayStatus
    data class Unavailable(val reason: String) : TerrainOverlayStatus
    data class Ready(val count: Int) : TerrainOverlayStatus
}

data class TerrainOverlayTileRequest(
    val key: String,
    val cacheKey: String,
    val productId: String,
    val path: String,
    val sourceTiles: List<TerrainOverlaySourceTile>,
    val z: Int,
    val x: Int,
    val yTms: Int,
    val left: Double,
    val top: Double,
    val size: Double,
)

data class TerrainOverlaySourceTile(
    val productId: String,
    val path: String,
    val resource: CoreResourceRequest?,
)

data class TerrainOverlayQueryResult(
    val status: TerrainOverlayStatus,
    val tileRequests: List<TerrainOverlayTileRequest>,
    val altitudeBucketFt: Double?,
    val frameKey: String?,
    val schedule: TerrainOverlayScheduleDecision,
)

data class TerrainOverlayScheduleDecision(
    val cachedCount: Int,
    val inFlightCount: Int,
    val missingCount: Int,
    val frameComplete: Boolean,
    val workBatch: List<TerrainOverlayTileRequest>,
)

data class ClientBuildInfo(
    val platform: String,
    val version: String,
    val builtAtUtc: String?,
    val commit: String?,
    val dirty: Boolean,
)

private val NativeAppCoreJson = Json {
    encodeDefaults = true
    ignoreUnknownKeys = true
}

private object NoopCoreSettingsStore : CoreSettingsStore {
    override fun readSettings(): ByteArray? = null
    override fun writeSettings(bytes: ByteArray) = Unit
}

class NativeAppCoreAdapter(
    private val navKvStore: NavKvStore? = null,
    private val bridge: NativeBridge = NativeBindings,
    private val json: Json = NativeAppCoreJson,
    private val sessionResourceFetcher: ((CoreResourceRequest) -> ByteArray)? = null,
) {
    fun situationRingCandidates(): List<SituationRingCandidate> =
        json.decodeFromString<List<WireSituationRingCandidate>>(bridge.situationRingCandidatesJson())
            .map { it.toUi() }

    fun createUiSession(
        recentAirportIds: List<String>,
        selectedAirportId: String?,
        selectedChartId: String?,
        installedPackageIds: List<String> = emptyList(),
        settingsStore: CoreSettingsStore? = null,
        displayPolicySettingsAvailable: Boolean = false,
        aerobagCloudBaseUrl: String? = null,
        clientBuildInfo: ClientBuildInfo? = null,
    ): NativeUiSession {
        val resultJson = bridge.createUiSessionJson(
            json.encodeToString(recentAirportIds),
            json.encodeToString(selectedAirportId),
            json.encodeToString(selectedChartId),
        )
        val result = json.decodeFromString<WireUiSessionInitResult>(resultJson)
        val session = NativeUiSession(
            handle = result.handle,
            bridge = bridge,
            json = json,
            navKvStore = navKvStore,
            sessionResourceFetcher = sessionResourceFetcher,
            initialSnapshot = result.snapshot.toUi(),
        )
        navKvStore?.attachToSession(result.handle)
        session.configurePlatformCapabilities(
            capabilitiesJson = buildJsonObject {
                put(
                    "display_policy",
                    if (displayPolicySettingsAvailable) {
                        buildJsonObject {}
                    } else {
                        JsonNull
                    },
                )
                put("offline_packages", buildJsonObject {})
                put(
                    "cloud",
                    buildJsonObject {
                        put("qr_scan", true)
                        aerobagCloudBaseUrl?.let { put("aerobag_cloud_base_url", it) }
                    },
                )
                put(
                    "live_feeds",
                    buildJsonObject {
                        put("acquisition_policy", "durable_complete_states")
                    },
                )
                put(
                    "client_build",
                    clientBuildInfo?.let { buildInfo ->
                        buildJsonObject {
                            put("platform", buildInfo.platform)
                            put("version", buildInfo.version)
                            buildInfo.builtAtUtc?.let { put("built_at_utc", it) }
                            buildInfo.commit?.let { put("commit", it) }
                            put("dirty", buildInfo.dirty)
                        }
                    } ?: JsonNull,
                )
                put("local_time_zone", ZoneId.systemDefault().id)
            }.toString(),
            settingsStore = settingsStore ?: NoopCoreSettingsStore,
        )
        session.setInstalledPackageIds(installedPackageIds)
        session.loadRasterMapCatalog()
        return session.apply {
            syncGuidanceGeometry()
        }
    }

    fun suggestAirwaysNear(anchor: NavRef, limit: Int = 5): List<AirwaySuggestion> {
        val result = runHadOperationElement(
            buildJsonObject {
                put("kind", "suggest_airways_near_anchor")
                put("anchor", json.encodeToJsonElement(anchor.toWire()))
                put("limit", limit)
            },
        )
        return json.decodeFromJsonElement<List<WireAirwaySuggestion>>(result).map { it.toUi() }
    }

    fun resolveNavRefPosition(navRef: NavRef): LatLonPoint {
        val result = runHadOperationElement(
            buildJsonObject {
                put("kind", "resolve_nav_ref_position")
                put("nav_ref", json.encodeToJsonElement(navRef.toWire()))
            },
        )
        return json.decodeFromJsonElement<WireLatLon>(result).toUi()
    }

    fun resolveNavRefIdentifier(identifier: String): NavRef {
        val result = runHadOperationElement(
            buildJsonObject {
                put("kind", "resolve_waypoint_identifier")
                put("identifier", identifier)
            },
        )
        return json.decodeFromJsonElement<WireNavRef>(result).toUi()
    }

    fun resolveNavSymbolFeature(navRef: NavRef): NavSymbolFeature? {
        val result = runHadOperationElement(
            buildJsonObject {
                put("kind", "resolve_nav_symbol_feature")
                put("nav_ref", json.encodeToJsonElement(navRef.toWire()))
            },
        )
        return json.decodeFromJsonElement<WireNavSymbolFeature?>(result)?.toUi()
    }

    fun suggestWaypointIdentifiersNear(
        anchor: LatLonPoint,
        query: String,
        limit: Int = 8,
    ): List<WaypointIdentifierSuggestion> {
        val result = runHadOperationElement(
            buildJsonObject {
                put("kind", "suggest_waypoint_identifiers_near")
                put("anchor", json.encodeToJsonElement(WireLatLon.serializer(), anchor.toWire()))
                put("query", query)
                put("limit", limit)
            },
        )
        return json.decodeFromJsonElement<List<WireWaypointIdentifierSuggestion>>(result).map { it.toUi() }
    }

    fun listProcedures(airportId: String, kind: ProcedureKind): List<ProcedureSummary> {
        val result = runHadOperationElement(
            buildJsonObject {
                put("kind", "list_procedures")
                put("airport_id", airportId)
                put("procedure_kind", json.encodeToJsonElement(kind.toWire()))
            },
        )
        return json.decodeFromJsonElement<List<WireProcedureSummary>>(result).map { it.toUi() }
    }

    fun describeProcedureOptions(airportId: String, procedureId: String, kind: ProcedureKind): ProcedureOptions {
        val result = runHadOperationElement(
            buildJsonObject {
                put("kind", "describe_procedure_options")
                put("airport_id", airportId)
                put("procedure_id", procedureId)
                put("procedure_kind", json.encodeToJsonElement(kind.toWire()))
            },
        )
        return runCatching {
            json.decodeFromJsonElement<WireProcedureOptions>(result).toUi()
        }.getOrElse { error ->
            Log.e("AerobagProcedure", "describeProcedureOptions decode failed airport=$airportId procedure=$procedureId json=$result", error)
            throw error
        }
    }

    private fun runHadOperationElement(operation: kotlinx.serialization.json.JsonObject): JsonElement =
        navKvStore?.runCoreOperationElement(operation)
            ?: error("nav_kv store is required for core data operation ${operation["kind"]}")

}

class NativeSessionCommandRejectedException(
    val commandName: String,
    val refreshedSnapshot: UiSessionSnapshot,
    cause: RuntimeException,
) : RuntimeException(
    "Session command failed; refreshed current app state command=$commandName",
    cause,
)

class NativeUiSession internal constructor(
    private val handle: Long,
    private val bridge: NativeBridge,
    private val json: Json,
    private val navKvStore: NavKvStore?,
    private val sessionResourceFetcher: ((CoreResourceRequest) -> ByteArray)?,
    initialSnapshot: UiSessionSnapshot,
) {
    @Volatile
    var snapshot: UiSessionSnapshot = initialSnapshot
        private set

    @Volatile
    private var invalidationListener: ((List<String>) -> Unit)? = null
    @Volatile
    private var snapshotListener: ((UiSessionSnapshot) -> Unit)? = null
    private val listenerLock = Any()
    private val sessionResourceEffectPump = navKvStore?.let { store ->
        AsyncSessionResourceEffectPump(
            executor = Executors.newSingleThreadExecutor { runnable ->
                Thread(runnable, "aerobag-session-effects").apply { isDaemon = true }
            },
            pump = {
                store.pumpSessionResourceEffects(
                    drainSessionResourceEffects = {
                        bridge.drainSessionResourceEffectsJson(handle)
                    },
                    fetchSessionResource = sessionResourceFetcher,
                    ingestSessionResource = { resource, bytes ->
                        bridge.ingestResourceInSession(handle, resource.id, bytes)
                    },
                )
            },
            publishInvalidations = { invalidations ->
                publishInvalidations("sessionResourceEffect", invalidations)
            },
        )
    }

    fun subscribeInvalidations(listener: (List<String>) -> Unit): AutoCloseable {
        synchronized(listenerLock) {
            invalidationListener = listener
        }
        return AutoCloseable {
            synchronized(listenerLock) {
                if (invalidationListener === listener) invalidationListener = null
            }
        }
    }

    fun subscribeSnapshots(listener: (UiSessionSnapshot) -> Unit): AutoCloseable {
        val currentSnapshot = synchronized(listenerLock) {
            snapshotListener = listener
            snapshot
        }
        listener(currentSnapshot)
        return AutoCloseable {
            synchronized(listenerLock) {
                if (snapshotListener === listener) snapshotListener = null
            }
        }
    }

    fun advanceInstalledArtifacts(
        artifacts: List<InstalledPackageArtifact>,
        libraryCacheJson: String,
        plannedGcFilenames: Set<String>,
    ): NavDbAdvanceUiResult {
        val store = requireNotNull(navKvStore) { "NAVDB advance requires a nav kv store" }
        val outcome = store.replaceInstalledArtifacts(
            artifacts,
            libraryCacheJson,
            handle,
            plannedGcFilenames,
        )
        val result = json.decodeFromJsonElement<WireNavDbAdvanceResult>(outcome.result)
        updateSnapshot(result.snapshot.toUi())
        publishPagedInvalidations("navDbAdvance", outcome, snapshotAlreadyReturned = true)
        return NavDbAdvanceUiResult(
            adopted = result.disposition == "adopted",
            snapshot = snapshot,
            retainedArtifactFilenames = result.retained_artifact_filenames.toSet(),
            rejectionReason = result.rejection_reason,
        )
    }

    fun maintainNavDb(nowEpochMs: Long): NavDbMaintenanceUiResult {
        val outcome = navKvStore?.runPagedSessionOperation(
            operation = {
                bridge.maintainNavDbInSessionAtEpochMsJson(handle, nowEpochMs)
            },
            resumeSnapshot = { bridge.getSessionSnapshotPagedJson(handle) },
        ) ?: error("NAVDB maintenance requires a nav kv store")
        val result = json.decodeFromJsonElement<WireNavDbMaintenanceResult>(outcome.result)
        updateSnapshot(result.snapshot.toUi())
        publishPagedInvalidations("navDbMaintenance", outcome, snapshotAlreadyReturned = true)
        return NavDbMaintenanceUiResult(
            shouldAttemptAdvance = result.action == "attempt_advance",
            snapshot = snapshot,
        )
    }

    private fun runPagedSnapshot(commandName: String, operation: () -> String): UiSessionSnapshot {
        val outcome = runNativeSessionCommand(commandName) {
            executePagedSnapshot(commandName, operation)
        } ?: return snapshot
        return outcome
    }

    private fun performFlightPlanCommand(
        commandName: String,
        command: JsonObject,
    ): UiSessionSnapshot =
        runPagedSnapshot(commandName) {
            bridge.performFlightPlanCommandInSessionJson(
                handle,
                command.toString(),
                System.currentTimeMillis(),
            )
        }

    private fun queryFlightPlan(query: JsonObject): JsonElement {
        val store = navKvStore ?: error("nav_kv store is required for flight-plan queries")
        val result = store.runPagedSessionOperationElement {
            bridge.queryFlightPlanInSessionJson(handle, query.toString())
        }
        sessionResourceEffectPump?.request()
        return result
    }

    private fun executePagedSnapshot(commandName: String, operation: () -> String): UiSessionSnapshot {
        val outcome = navKvStore?.runPagedSessionOperation(
            operation = operation,
            resumeSnapshot = { bridge.getSessionSnapshotPagedJson(handle) },
        )
            ?: run {
                val outcome = json.parseToJsonElement(operation()).jsonObject
                when (val state = outcome.getValue("state").jsonPrimitive.content) {
                    "complete" -> PagedSessionOperationResult(
                        result = outcome["result"] ?: JsonNull,
                        invalidations = outcome["invalidations"]
                            ?.jsonArray
                            ?.map { it.jsonPrimitive.content }
                            ?: emptyList(),
                    )
                    "need_resources", "need_snapshot_resources" ->
                        error("nav_kv store is required for paged session resources")
                    else -> error("unknown HAD session operation state: $state")
                }
            }
        updateSnapshot(json.decodeFromJsonElement<WireUiSessionSnapshot>(outcome.result).toUi())
        publishPagedInvalidations(commandName, outcome, snapshotAlreadyReturned = true)
        return snapshot
    }

    private fun publishPagedInvalidations(
        commandName: String,
        outcome: PagedSessionOperationResult,
        snapshotAlreadyReturned: Boolean = false,
    ): List<String> {
        val invalidations = outcome.invalidations.distinct()
        val publishedInvalidations = if (snapshotAlreadyReturned) {
            invalidations - "session_snapshot"
        } else {
            invalidations
        }
        publishInvalidations(commandName, publishedInvalidations)
        sessionResourceEffectPump?.request()
        return invalidations
    }

    private fun publishInvalidations(commandName: String, invalidations: List<String>) {
        if (invalidations.isEmpty()) return
        diagnosticLogInfo("AerobagInvalidation") {
            "source=$commandName invalidations=${invalidations.joinToString(",")}"
        }
        invalidationListener?.invoke(invalidations)
    }

    private fun updateSnapshot(nextSnapshot: UiSessionSnapshot) {
        snapshot = nextSnapshot
        snapshotListener?.invoke(nextSnapshot)
    }

    private fun <T> runNativeSessionCommand(commandName: String, operation: () -> T): T? =
        try {
            operation()
        } catch (error: RuntimeException) {
            if (!error.isNativeSessionCommandFailure()) {
                throw error
            }
            val refreshedSnapshot = refreshSnapshotAfterRejectedCommand(commandName, error)
            throw NativeSessionCommandRejectedException(commandName, refreshedSnapshot, error)
        }

    private fun refreshSnapshotAfterRejectedCommand(commandName: String, error: RuntimeException): UiSessionSnapshot {
        Log.w("AerobagSessionCommand", "session command failed; refreshing snapshot command=$commandName", error)
        return try {
            snapshot = executePagedSnapshot("refreshSnapshotAfterRejectedCommand") {
                bridge.getSessionSnapshotAtEpochMsPagedJson(handle, System.currentTimeMillis())
            }
            snapshot
        } catch (refreshError: RuntimeException) {
            Log.e(
                "AerobagSessionCommand",
                "failed to refresh snapshot after session command failure command=$commandName",
                refreshError,
            )
            snapshot
        }
    }

    fun syncGuidanceGeometry(commandName: String = "syncGuidanceGeometry"): UiSessionSnapshot {
        return runPagedSnapshot(commandName) {
            bridge.syncGuidanceGeometryInSessionJson(handle)
        }
    }

    fun installLiveFeedCacheProduct(
        cache: LiveFeedCache,
        product: String,
        version: String,
    ): UiSessionSnapshot {
        return runPagedSnapshot("installLiveFeedCacheProduct") {
            cache.installProductInSessionJson(handle, product, version)
        }
    }

    fun installPreparedLiveFeedCacheProduct(
        cache: LiveFeedCache,
        product: String,
        version: String,
        preparedBytes: ByteArray,
    ): UiSessionSnapshot {
        return runPagedSnapshot("installPreparedLiveFeedCacheProduct") {
            cache.installPreparedProductInSessionJson(
                handle,
                product,
                version,
                preparedBytes,
            )
        }
    }

    fun syncLiveFeedCacheCatalog(cache: LiveFeedCache): UiSessionSnapshot {
        return runPagedSnapshot("syncLiveFeedCacheCatalog") {
            cache.syncCatalogInSessionJson(handle)
        }
    }

    fun loadOfflinePackageLibraryCache(libraryCacheJson: String?): UiSessionSnapshot {
        val payload = libraryCacheJson?.takeIf { it.isNotBlank() } ?: return snapshot
        return runPagedSnapshot("loadOfflinePackageLibraryCache") {
            bridge.loadOfflinePackageLibraryCacheInSessionJson(handle, payload)
        }
    }

    fun projectFlightPlanRoute(): FlightPlanRouteProjection {
        val store = navKvStore ?: return FlightPlanRouteProjection(
            flightPlanRouteRevision = snapshot.flightPlanRouteRevision,
            segments = emptyList(),
        )
        val outcome = store.runPagedSessionOperation {
            bridge.projectFlightPlanRouteInSessionJson(handle)
        }
        publishPagedInvalidations("projectFlightPlanRoute", outcome)
        return json.decodeFromJsonElement<WireFlightPlanRouteProjection>(outcome.result).toUi()
    }

    fun performMapSelectionAction(action: String): UiSessionSnapshot {
        return runPagedSnapshot("performMapSelectionAction") {
            bridge.performMapSelectionActionInSessionJson(
                handle,
                action,
                System.currentTimeMillis(),
            )
        }
    }

    fun describePlateProcedureLoads(plateId: String): List<ProcedureLoadOption> {
        val result = queryFlightPlan(
            buildJsonObject {
                put("kind", "describe_plate_procedure_loads")
                put("plate_id", plateId)
            },
        )
        return json.decodeFromJsonElement<List<WireProcedureLoadOption>>(result).map { it.toUi() }
    }

    fun deriveChartPageState(): DerivedChartPageState {
        val result = queryFlightPlan(
            buildJsonObject { put("kind", "chart_page_state") },
        )
        return json.decodeFromJsonElement<WireDerivedChartPageState>(result).toUi()
    }

    fun airportInfo(airportId: String, nowEpochMs: Long = System.currentTimeMillis()): AirportInfoUiView {
        val result = queryFlightPlan(
            buildJsonObject {
                put("kind", "airport_info")
                put("airport_id", airportId)
                put("now_epoch_ms", nowEpochMs)
            },
        )
        return json.decodeFromJsonElement<WireAirportInfoUiView>(result).toUi()
    }

    fun loadPlateProcedure(loadId: String): UiSessionSnapshot {
        return performFlightPlanCommand(
            "loadPlateProcedure",
            buildJsonObject {
                put("kind", "load_plate_procedure")
                put("load_id", loadId)
            },
        )
    }

    fun prepareAirwayPresentationAtFlightPlanRow(
        rowUid: String,
        airwayName: String,
    ): AirwayPresentationPlan {
        val result = queryFlightPlan(
            buildJsonObject {
                put("kind", "prepare_airway_presentation_at_row")
                put("row_uid", rowUid)
                put("airway_name", airwayName)
            },
        )
        return json.decodeFromJsonElement<WireAirwayPresentationPlan>(result).toUi()
    }

    fun restoreDirectTo(): UiSessionSnapshot {
        return performFlightPlanCommand(
            "restoreDirectTo",
            buildJsonObject { put("kind", "restore_direct_to") },
        )
    }

    fun insertWaypointAtFlightPlanRow(rowUid: String, before: Boolean, waypoint: NavRef): UiSessionSnapshot {
        return performFlightPlanCommand(
            "insertWaypointAtFlightPlanRow",
            buildJsonObject {
                put("kind", "insert_waypoint_at_row")
                put("row_uid", rowUid)
                put("before", before)
                put("waypoint", json.encodeToJsonElement(waypoint.toWire()))
            },
        )
    }

    fun suggestWaypointIdentifiersAtFlightPlanRow(
        rowUid: String,
        before: Boolean,
        query: String,
        limit: Int = 8,
    ): List<WaypointIdentifierSuggestion> {
        val result = queryFlightPlan(
            buildJsonObject {
                put("kind", "suggest_waypoint_identifiers_at_row")
                put("row_uid", rowUid)
                put("before", before)
                put("query", query)
                put("limit", limit)
            },
        )
        return json.decodeFromJsonElement<List<WireWaypointIdentifierSuggestion>>(result).map { it.toUi() }
    }

    fun previewFlightPlanEntry(input: String): FlightPlanEntryPreview {
        val result = queryFlightPlan(
            buildJsonObject {
                put("kind", "preview_entry")
                put("input", input)
            },
        )
        return json.decodeFromJsonElement<WireFlightPlanEntryPreview>(result).toUi()
    }

    fun appendFlightPlanEntry(input: String): UiSessionSnapshot {
        return performFlightPlanCommand(
            "appendFlightPlanEntry",
            buildJsonObject {
                put("kind", "append_entry")
                put("input", input)
            },
        )
    }

    fun insertAirwayAtFlightPlanRow(
        rowUid: String,
        presentation: AirwayPresentationPlan,
        entryPointUid: String,
        exitPointUid: String,
    ): UiSessionSnapshot {
        return performFlightPlanCommand(
            "insertAirwayAtFlightPlanRow",
            buildJsonObject {
                put("kind", "insert_airway_at_row")
                put("row_uid", rowUid)
                put(
                    "selection",
                    buildJsonObject {
                        put("airway_name", presentation.airwayName)
                        put("branch_key", presentation.branchKey)
                        put("entry_point_uid", entryPointUid)
                        put("exit_point_uid", exitPointUid)
                    },
                )
            },
        )
    }

    fun selectProcedureAtFlightPlanRow(
        rowUid: String,
        airportId: String,
        procedureId: String,
        kind: ProcedureKind,
        runwayTransition: String?,
        enrouteTransition: String?,
    ): UiSessionSnapshot {
        return performFlightPlanCommand(
            "selectProcedureAtFlightPlanRow",
            buildJsonObject {
                put("kind", "select_procedure_at_row")
                put("row_uid", rowUid)
                put("airport_id", airportId)
                put("procedure_id", procedureId)
                put("procedure_kind", json.encodeToJsonElement(kind.toWire()))
                put("runway_transition", json.encodeToJsonElement(runwayTransition))
                put("enroute_transition", json.encodeToJsonElement(enrouteTransition))
            },
        )
    }

    fun registerOwnshipSource(registration: OwnshipSourceRegistration): UiSessionSnapshot {
        return runPagedSnapshot("registerOwnshipSource") {
            bridge.registerOwnshipSourceInSessionPagedJson(handle, registration.toCoreJson(json))
        }
    }

    fun updateOwnshipSourceStatus(update: OwnshipSourceStatusUpdate): UiSessionSnapshot {
        return runPagedSnapshot("updateOwnshipSourceStatus") {
            bridge.updateOwnshipSourceStatusInSessionPagedJson(handle, update.toCoreJson(json))
        }
    }

    fun pushSituationSample(sample: SituationSample): UiSessionSnapshot {
        return runPagedSnapshot("pushSituationSample") {
            bridge.pushSituationSampleInSessionPagedJson(handle, sample.toCoreJson(json))
        }
    }

    fun selectOwnshipSource(selection: OwnshipSelection): UiSessionSnapshot {
        return runPagedSnapshot("selectOwnshipSource") {
            bridge.selectOwnshipSourceInSessionPagedJson(handle, selection.toCoreJson(json))
        }
    }

    fun applySituationControlInput(input: SituationControlInput, nowEpochMs: Double): UiSessionSnapshot {
        return runPagedSnapshot("applySituationControlInput") {
            bridge.applySituationControlInputInSessionJson(
                handle,
                input.toCoreJson(json),
                nowEpochMs,
            )
        }
    }

    fun engageMapFollow(viewport: MapViewportState): UiSessionSnapshot {
        return runPagedSnapshot("engageMapFollow") {
            bridge.engageMapFollowInSessionJson(handle, viewport.toCoreViewport().toCoreJson(json))
        }
    }

    fun disengageMapFollow(viewport: MapViewportState): UiSessionSnapshot {
        return runPagedSnapshot("disengageMapFollow") {
            bridge.disengageMapFollowInSessionJson(handle, viewport.toCoreViewport().toCoreJson(json))
        }
    }

    fun loadPlaybackTrace(sourcePath: String, traceJson: String): UiSessionSnapshot {
        return runPagedSnapshot("loadPlaybackTrace") {
            bridge.loadPlaybackTraceInSessionPagedJson(
                handle,
                json.encodeToString(sourcePath),
                traceJson,
            )
        }
    }

    fun playPlayback(nowEpochMs: Double): UiSessionSnapshot {
        return runPagedSnapshot("playPlayback") {
            bridge.playPlaybackInSessionPagedJson(handle, nowEpochMs)
        }
    }

    fun pausePlayback(nowEpochMs: Double): UiSessionSnapshot {
        return runPagedSnapshot("pausePlayback") {
            bridge.pausePlaybackInSessionPagedJson(handle, nowEpochMs)
        }
    }

    fun seekPlayback(cursorSeconds: Double, nowEpochMs: Double): UiSessionSnapshot {
        return runPagedSnapshot("seekPlayback") {
            bridge.seekPlaybackInSessionPagedJson(handle, cursorSeconds, nowEpochMs)
        }
    }

    fun setPlaybackRate(rate: Double, nowEpochMs: Double): UiSessionSnapshot {
        return runPagedSnapshot("setPlaybackRate") {
            bridge.setPlaybackRateInSessionPagedJson(handle, rate, nowEpochMs)
        }
    }

    fun tickPlayback(nowEpochMs: Double): UiSessionSnapshot {
        return runPagedSnapshot("tickPlayback") {
            bridge.tickPlaybackInSessionPagedJson(handle, nowEpochMs)
        }
    }

    fun tickBadAutopilot(nowEpochMs: Double): UiSessionSnapshot {
        return runPagedSnapshot("tickBadAutopilot") {
            bridge.tickBadAutopilotInSessionPagedJson(handle, nowEpochMs)
        }
    }

    fun selectAirport(airportId: String): UiSessionSnapshot {
        return runPagedSnapshot("selectAirport") {
            bridge.selectAirportInSessionJson(handle, json.encodeToString(airportId))
        }
    }

    fun selectChart(chartId: String): UiSessionSnapshot {
        return runPagedSnapshot("selectChart") {
            bridge.selectChartInSessionJson(handle, json.encodeToString(chartId))
        }
    }

    fun selectChartReference(
        familyId: String,
        suggestedChartIds: List<String>,
    ): UiSessionSnapshot {
        return runPagedSnapshot("selectChartReference") {
            bridge.selectChartReferenceInSessionJson(
                handle,
                json.encodeToString(familyId),
                json.encodeToString(suggestedChartIds),
            )
        }
    }

    fun setMapLayerVisibility(layerId: MapLayerId, visible: Boolean): UiSessionSnapshot {
        return runPagedSnapshot("setMapLayerVisibility") {
            bridge.setMapLayerVisibilityInSessionPagedJson(handle, json.encodeToString(layerId.toWire()), visible)
        }
    }

    fun setMapLayerEnabled(layerId: MapLayerId, enabled: Boolean): UiSessionSnapshot {
        return runPagedSnapshot("setMapLayerEnabled") {
            bridge.setMapLayerEnabledInSessionPagedJson(handle, json.encodeToString(layerId.toWire()), enabled)
        }
    }

    fun setDebugFlag(flagId: String, enabled: Boolean): UiSessionSnapshot {
        return runPagedSnapshot("setDebugFlag") {
            bridge.setDebugFlagInSessionJson(handle, json.encodeToString(flagId), enabled)
        }
    }

    fun loadRasterMapCatalog(): UiSessionSnapshot {
        return runPagedSnapshot("loadRasterMapCatalog") {
            bridge.loadRasterMapCatalogInSessionJson(handle)
        }
    }

    fun selectMapFamily(familyId: String): UiSessionSnapshot {
        return runPagedSnapshot("selectMapFamily") {
            bridge.selectMapFamilyInSessionJson(handle, json.encodeToString(familyId))
        }
    }

    fun selectRasterMap(selectedMapId: String): UiSessionSnapshot {
        return runPagedSnapshot("selectRasterMap") {
            bridge.selectRasterMapInSessionJson(handle, json.encodeToString(selectedMapId))
        }
    }

    fun refreshSnapshot(): UiSessionSnapshot {
        runPagedSnapshot("refreshSnapshot") {
            bridge.getSessionSnapshotAtEpochMsPagedJson(handle, System.currentTimeMillis())
        }
        return syncGuidanceGeometry()
    }

    fun setInstalledPackageIds(packageIds: List<String>): UiSessionSnapshot {
        return runPagedSnapshot("setInstalledPackageIds") {
            bridge.setInstalledPackageIdsInSessionJson(handle, json.encodeToString(packageIds))
        }
    }

    fun configurePlatformCapabilities(
        capabilitiesJson: String,
        settingsStore: CoreSettingsStore,
    ): UiSessionSnapshot {
        return runPagedSnapshot("configurePlatformCapabilities") {
            bridge.configurePlatformCapabilitiesInSessionJson(handle, capabilitiesJson, settingsStore)
        }
    }

    fun performFlightPlanRowAction(rowUid: String, actionUid: String): UiSessionSnapshot {
        return performFlightPlanCommand(
            "performFlightPlanRowAction",
            buildJsonObject {
                put("kind", "perform_row_action")
                put("row_uid", rowUid)
                put("action_uid", actionUid)
            },
        )
    }

    fun performStatusAction(actionId: String): UiSessionSnapshot {
        return runPagedSnapshot("performStatusAction") {
            bridge.performStatusActionInSessionJson(handle, actionId)
        }
    }

    fun performSettingsAction(actionId: String, valueId: String): UiSessionSnapshot {
        return runPagedSnapshot("performSettingsAction") {
            bridge.performSettingsActionInSessionJson(
                handle,
                buildJsonObject {
                    put("action_id", actionId)
                    put("value_id", valueId)
                }.toString(),
            )
        }
    }

    fun takeCloudAuthorizationRequest(nowEpochMs: Long): CloudAuthorizationRequest? =
        json.decodeFromString(
            bridge.takeCloudAuthorizationRequestInSessionJson(handle, nowEpochMs),
        )

    fun completeCloudAuthorization(
        requestId: Long,
        response: CloudAuthorizationResponse,
        nowEpochMs: Long,
    ): UiSessionSnapshot = runPagedSnapshot("completeCloudAuthorization") {
        bridge.completeCloudAuthorizationInSessionJson(
            handle,
            requestId,
            json.encodeToString(response),
            nowEpochMs,
        )
    }

    fun performCloudUiAction(
        actionId: CloudUiActionId,
        fields: List<CloudUiFieldValue>,
        nowEpochMs: Long,
    ): UiSessionSnapshot = runPagedSnapshot("performCloudUiAction") {
        bridge.performCloudUiActionInSessionJson(
            handle,
            json.encodeToString(actionId),
            json.encodeToString(fields),
            nowEpochMs,
        )
    }

    fun recordOfflinePackagePreferences(
        preferencesJson: String,
        nowEpochMs: Long,
    ): UiSessionSnapshot = runPagedSnapshot("recordOfflinePackagePreferences") {
        bridge.recordOfflinePackagePreferencesInSessionJson(
            handle,
            preferencesJson,
            nowEpochMs,
        )
    }

    fun takeCloudProviderRequest(nowEpochMs: Long): CloudHttpRequest? =
        json.decodeFromString(
            bridge.takeCloudProviderRequestInSessionJson(handle, nowEpochMs),
        )

    fun completeCloudProviderRequest(
        requestId: Long,
        response: CloudHttpResponse,
        nowEpochMs: Long,
    ): UiSessionSnapshot = runPagedSnapshot("completeCloudProviderRequest") {
        bridge.completeCloudProviderRequestInSessionJson(
            handle,
            requestId,
            json.encodeToString(response),
            nowEpochMs,
        )
    }

    fun acceptDisclaimer(agreementId: String): UiSessionSnapshot {
        return runPagedSnapshot("acceptDisclaimer") {
            bridge.acceptDisclaimerInSessionJson(handle, agreementId)
        }
    }

    fun cloudEventStreamPlan(): CloudEventStreamPlan? =
        json.decodeFromString(bridge.cloudEventStreamPlanInSessionJson(handle))

    fun reportCloudEventStreamEvent(
        event: CloudEventStreamEvent,
        nowEpochMs: Long,
    ): UiSessionSnapshot = runPagedSnapshot("reportCloudEventStreamEvent") {
        bridge.reportCloudEventStreamEventInSessionJson(
            handle,
            json.encodeToString(event),
            nowEpochMs,
        )
    }

    fun activateNextLeg(): UiSessionSnapshot {
        return performFlightPlanCommand(
            "activateNextLeg",
            buildJsonObject { put("kind", "activate_next_leg") },
        )
    }

    fun stopNavigation(): UiSessionSnapshot {
        return performFlightPlanCommand(
            "stopNavigation",
            buildJsonObject { put("kind", "stop_navigation") },
        )
    }

    fun suspendSequencing(): UiSessionSnapshot {
        return performFlightPlanCommand(
            "suspendSequencing",
            buildJsonObject { put("kind", "suspend_sequencing") },
        )
    }

    fun unsuspendSequencing(): UiSessionSnapshot {
        return performFlightPlanCommand(
            "unsuspendSequencing",
            buildJsonObject { put("kind", "unsuspend_sequencing") },
        )
    }

    fun sequenceActiveLeg(): UiSessionSnapshot {
        return performFlightPlanCommand(
            "sequenceActiveLeg",
            buildJsonObject { put("kind", "sequence_active_leg") },
        )
    }

    fun restoreChartPageState(
        recentAirportIds: List<String>,
        plateTargetAirportId: String?,
        selectedAirportId: String?,
        selectedReferenceFamilyId: String?,
        selectedChartId: String?,
        suggestedChartIds: List<String>,
    ): UiSessionSnapshot {
        return runPagedSnapshot("restoreChartPageState") {
            bridge.restoreChartPageStateInSessionJson(
                handle,
                json.encodeToString(recentAirportIds),
                json.encodeToString(plateTargetAirportId),
                json.encodeToString(selectedAirportId),
                json.encodeToString(selectedReferenceFamilyId),
                json.encodeToString(selectedChartId),
                json.encodeToString(suggestedChartIds),
            )
        }
    }

    fun ingestPointTiles(tiles: List<PointTilePayload>) {
        bridge.ingestPointTilesInSessionJson(handle, json.encodeToString(tiles.map { it.toWire() }))
    }

    fun ingestAirspaceRefTilesJson(tilesJson: String) {
        bridge.ingestAirspaceRefTilesInSessionJson(handle, tilesJson)
    }

    fun ingestAirspaceFeaturesJson(featuresJson: String) {
        bridge.ingestAirspaceFeaturesInSessionJson(handle, featuresJson)
    }

    fun ingestAirspaceLabelTilesJson(tilesJson: String) {
        bridge.ingestAirspaceLabelTilesInSessionJson(handle, tilesJson)
    }

    fun syncLiveFeeds(fetchResource: (CoreResourceRequest) -> ByteArray): List<String> {
        val store = navKvStore ?: error("session missing nav_db for live-feed sync")
        val outcome = store.runPagedSessionOperation(
            operation = { bridge.syncLiveFeedsInSessionJson(handle) },
            fetchSessionResource = fetchResource,
            ingestSessionResource = { resource, bytes ->
                bridge.ingestResourceInSession(handle, resource.id, bytes)
            },
        )
        return publishPagedInvalidations("syncLiveFeeds", outcome)
    }

    fun ingestLiveFeedSseEvents(
        events: List<LiveFeedSseEvent>,
        fetchResource: (CoreResourceRequest) -> ByteArray,
    ): List<String> {
        val store = navKvStore ?: error("session missing nav_db for live-feed SSE")
        val outcome = store.runPagedSessionOperation(
            operation = {
                bridge.ingestLiveFeedSseEventsInSessionJson(handle, json.encodeToString(events))
            },
            fetchSessionResource = fetchResource,
            ingestSessionResource = { resource, bytes ->
                bridge.ingestResourceInSession(handle, resource.id, bytes)
            },
        )
        return publishPagedInvalidations("ingestLiveFeedSseEvents", outcome)
    }

    fun reportLiveFeedConnectionEvent(event: LiveFeedConnectionEvent): UiSessionSnapshot {
        return runPagedSnapshot("reportLiveFeedConnectionEvent") {
            bridge.reportLiveFeedConnectionEventInSessionJson(handle, json.encodeToString(event))
        }
    }

    @RawUiSessionWorkApi
    fun queryMapOverlay(
        viewport: MapViewportState,
        widthPx: Double,
        heightPx: Double,
        pointDisplayScale: Double,
        fetchResource: (CoreResourceRequest) -> ByteArray,
    ): MapOverlayQueryOutcome {
        val viewportJson = json.encodeToString(viewport.toWire())
        val store = navKvStore ?: error("session missing nav_db for map overlay")
        val outcome = store.runPagedSessionOperation(
            operation = {
                bridge.getMapOverlayInSessionWithPointDisplayScaleJson(
                    handle,
                    viewportJson,
                    widthPx,
                    heightPx,
                    pointDisplayScale,
                )
            },
            fetchSessionResource = fetchResource,
            ingestSessionResource = { resource, bytes ->
                bridge.ingestResourceInSession(handle, resource.id, bytes)
            },
        )
        val invalidations = publishPagedInvalidations("queryMapOverlay", outcome)
        return MapOverlayQueryOutcome(
            overlay = json.decodeFromJsonElement<WireMapOverlayQueryResult>(outcome.result).toUi(),
            invalidations = invalidations,
        )
    }

    @RawUiSessionWorkApi
    fun queryMapSelection(
        viewport: MapViewportState,
        widthPx: Double,
        heightPx: Double,
        click: LatLonPoint,
        pointDisplayScale: Double,
        fetchResource: (CoreResourceRequest) -> ByteArray,
    ): MapSelectionQueryResult {
        val viewportJson = json.encodeToString(viewport.toWire())
        val clickJson = json.encodeToString(click.toWire())
        val store = navKvStore ?: error("session missing nav_db for map selection")
        val result = store.runPagedSessionOperation(
            fetchSessionResource = fetchResource,
            ingestSessionResource = { resource, bytes ->
                bridge.ingestResourceInSession(handle, resource.id, bytes)
            },
        ) {
                bridge.getMapSelectionInSessionWithPointDisplayScaleJson(
                    handle,
                    viewportJson,
                    widthPx,
                    heightPx,
                    clickJson,
                    pointDisplayScale,
                )
        }
        publishPagedInvalidations("queryMapSelection", result)
        return json.decodeFromJsonElement<WireMapSelectionQueryResult>(result.result).toUi()
    }

    @RawUiSessionWorkApi
    fun queryMapSelectionForNavRef(
        viewport: MapViewportState,
        widthPx: Double,
        heightPx: Double,
        navRef: NavRef,
        pointDisplayScale: Double,
        fetchResource: (CoreResourceRequest) -> ByteArray,
    ): MapSelectionForNavRefResult {
        val viewportJson = json.encodeToString(viewport.toWire())
        val navRefJson = json.encodeToString(navRef.toWire())
        val store = navKvStore ?: error("session missing nav_db for map selection")
        val result = store.runPagedSessionOperation(
            fetchSessionResource = fetchResource,
            ingestSessionResource = { resource, bytes ->
                bridge.ingestResourceInSession(handle, resource.id, bytes)
            },
        ) {
                bridge.getMapSelectionForNavRefInSessionWithPointDisplayScaleJson(
                    handle,
                    viewportJson,
                    widthPx,
                    heightPx,
                    navRefJson,
                    pointDisplayScale,
                )
        }
        publishPagedInvalidations("queryMapSelectionForNavRef", result)
        return json.decodeFromJsonElement<WireMapSelectionForNavRefResult>(result.result).toUi()
    }

    fun queryTerrainOverlay(
        viewport: MapViewportState,
        widthPx: Double,
        heightPx: Double,
        decodedCacheKeys: Collection<String>,
        inFlightCacheKeys: Collection<String>,
        fetchResource: (CoreResourceRequest) -> ByteArray,
    ): TerrainOverlayQueryResult {
        val viewportJson = json.encodeToString(viewport.toWire())
        val decodedCacheKeysJson = json.encodeToString(decodedCacheKeys.toList())
        val inFlightCacheKeysJson = json.encodeToString(inFlightCacheKeys.toList())
        val store = navKvStore ?: error("session missing nav_db for terrain overlay")
        val result = store.runPagedSessionOperation(
            operation = {
                bridge.getScheduledTerrainOverlayInSessionJson(
                    handle,
                    viewportJson,
                    widthPx,
                    heightPx,
                    decodedCacheKeysJson,
                    inFlightCacheKeysJson,
                )
            },
            fetchSessionResource = fetchResource,
            ingestSessionResource = { resource, bytes ->
                bridge.ingestResourceInSession(handle, resource.id, bytes)
            },
        )
        publishPagedInvalidations("queryTerrainOverlay", result)
        return json.decodeFromJsonElement<WireTerrainOverlayQueryResult>(result.result).toUi()
    }

    fun queryNexradOverlay(
        viewport: MapViewportState,
        widthPx: Double,
        heightPx: Double,
        fetchResource: (CoreResourceRequest) -> ByteArray,
    ): NexradOverlayQueryResult {
        val viewportJson = json.encodeToString(viewport.toWire())
        val result = navKvStore?.runPagedSessionOperation(
            operation = {
                bridge.getNexradOverlayInSessionJson(handle, viewportJson, widthPx, heightPx)
            },
            fetchSessionResource = fetchResource,
            ingestSessionResource = { resource, bytes ->
                bridge.ingestResourceInSession(handle, resource.id, bytes)
            },
        ) ?: error("session missing nav_db for NEXRAD overlay")
        publishPagedInvalidations("queryNexradOverlay", result)
        return json.decodeFromJsonElement(
            result.result,
        )
    }

    fun chartAssetBytes(
        chartId: String,
        assetKind: String,
        fetchResource: (CoreResourceRequest) -> ByteArray,
    ): ByteArray {
        require(assetKind == "asset" || assetKind == "thumbnail") {
            "unsupported chart asset kind: $assetKind"
        }
        val store = navKvStore ?: error("session missing nav_db for chart asset fetch")
        val result = store.runPagedSessionOperation(
            fetchSessionResource = fetchResource,
            ingestSessionResource = { resource, bytes ->
                bridge.ingestResourceInSession(handle, resource.id, bytes)
            },
        ) {
            bridge.resolveChartAssetResourceInSessionJson(handle, chartId, assetKind)
        }
        publishPagedInvalidations("chartAssetBytes", result)
        val resultJson = result.result.jsonObject
        val source = parseCoreResourceSource(resultJson.getValue("source").jsonObject)
        val resource = CoreResourceRequest("chart_asset/$assetKind/$chartId", source, false)
        return try {
            fetchResource(resource).also { bytes ->
                Log.i(
                    "AerobagCharts",
                    "plate asset loaded chart=$chartId kind=$assetKind " +
                        "source=${source.describeForLog()} bytes=${bytes.size}",
                )
            }
        } catch (error: Throwable) {
            throw IllegalStateException(
                "failed to fetch chart asset chart=$chartId kind=$assetKind source=${source.describeForLog()}",
                error,
            )
        }
    }

    fun nexradTileBytes(
        src: String,
        fetchResource: (CoreResourceRequest) -> ByteArray,
    ): ByteArray {
        val store = navKvStore ?: error("session missing nav_db for NEXRAD tile fetch")
        val result = store.runPagedSessionOperation(
            fetchSessionResource = fetchResource,
            ingestSessionResource = { resource, bytes ->
                bridge.ingestResourceInSession(handle, resource.id, bytes)
            },
        ) {
            bridge.prepareNexradTileInSessionJson(handle, src)
        }
        publishPagedInvalidations("nexradTileBytes", result)
        return bridge.nexradTileBytesInSession(handle, src)
    }

    fun queryRasterTilePlanJson(
        viewport: MapViewportState,
        widthPx: Double,
        heightPx: Double,
        devicePixelRatio: Double,
    ): String {
        val viewportJson = json.encodeToString(logicalViewportForDisplayScale(viewport, devicePixelRatio).toWire())
        return bridge.getRasterTilePlanInSessionWithDisplayScaleJson(
            handle,
            viewportJson,
            widthPx,
            heightPx,
            devicePixelRatio,
        )
    }

    fun renderTerrainOverlayTile(
        request: TerrainOverlayTileRequest,
        aircraftAltitudeFt: Double,
        fetchResource: (CoreResourceRequest) -> ByteArray,
    ): ByteArray {
        val resources = request.sourceTiles
            .mapNotNull { it.resource }
            .distinctBy { it.id }
        require(resources.isNotEmpty()) {
            "terrain request ${request.key} has no core source resources"
        }
        resources.forEach { resource ->
            bridge.ingestResourceInSession(handle, resource.id, fetchResource(resource))
        }
        return bridge.renderTerrainOverlayTileByKeyInSession(handle, request.key, aircraftAltitudeFt)
    }

    fun syncMapFollow(viewport: MapViewportState, widthPx: Double, heightPx: Double): UiSessionSnapshot {
        val viewportJson = json.encodeToString(viewport.toWire())
        return runPagedSnapshot("syncMapFollow") {
            bridge.syncMapFollowInSessionJson(handle, viewportJson, widthPx, heightPx)
        }
    }

    fun destroy() {
        sessionResourceEffectPump?.close()
        bridge.destroySession(handle)
    }

}

private fun RuntimeException.isNativeSessionCommandFailure(): Boolean =
    stackTrace.any { it.className == "org.aerobag.app.domain.NativeBindings" }

private fun MapViewportState.toWire(): WireMapViewport {
    val (lat, lon) = viewportCenterLatLon(this)
    return WireMapViewport(
        center = WireLatLon(lat = lat, lon = lon),
        zoom = zoom,
        rotation_deg = rotationDeg,
        pitch_deg = 0.0,
    )
}

private fun PlanLeg.toWire() = WirePlanLeg(
    from = from.toWire(),
    to = to.toWire(),
    airway = airway,
)

private fun NavRef.toWire(): WireNavRef = when (this) {
    is NavRef.Airport -> WireNavRef.Airport(code)
    is NavRef.Navaid -> WireNavRef.Navaid(code)
    is NavRef.ArincNavaid -> WireNavRef.ArincNavaid(
        identifier = identifier,
        icao_code = icaoCode,
        section_code = sectionCode,
        subsection_code = subsectionCode,
    )
    is NavRef.TerminalNavaid -> WireNavRef.TerminalNavaid(
        airport_id = airportId,
        identifier = identifier,
        icao_code = icaoCode,
        section_code = sectionCode,
        subsection_code = subsectionCode,
    )
    is NavRef.Fix -> WireNavRef.Fix(code)
    is NavRef.LatLon -> WireNavRef.LatLon(WireLatLon(lat, lon))
    is NavRef.Spot -> WireNavRef.Spot(WireLatLon(lat, lon))
}

private fun OwnshipSelection.toWire(): WireOwnshipSelection = when (this) {
    OwnshipSelection.Auto -> WireOwnshipSelection.Auto
    is OwnshipSelection.Source -> WireOwnshipSelection.Source(sourceId)
}

private fun OwnshipRenderState.toWire() = WireOwnshipRenderState(
    mode = mode.toWire(),
    banner_text = bannerText,
    banner_severity = bannerSeverity.toWire(),
    draw_aircraft = drawAircraft,
    draw_predictor = drawPredictor,
    draw_cdi = drawCdi,
    position = position?.toWire(),
    track_deg_true = trackDegTrue,
    orientation_deg = orientationDeg,
    magnetic_variation_deg = magneticVariationDeg,
    speed_kt = speedKt,
    terrain_altitude_bucket_ft = terrainAltitudeBucketFt,
)

private fun OwnshipControlModel.toWire() = WireOwnshipControlModel(
    mode = mode.toWire(),
    selection = selection.toWire(),
    launcher_label = launcherLabel,
    launcher_tone = launcherTone.toWire(),
    launcher_text_tone = launcherTextTone.toWire(),
    sources = sources.map { it.toWire() },
    situation_controls = situationControls.map { it.toWire() },
)

private fun OwnshipSourceMenuItem.toWire() = WireOwnshipSourceMenuItem(
    source_id = sourceId,
    source_kind = sourceKind.toWire(),
    label = label,
    launcher_label = launcherLabel,
    tone = tone.toWire(),
    enabled = enabled,
    disabled_reason = disabledReason,
    active = active,
    status_label = statusLabel,
)

private fun SituationControlMenuItem.toWire() = WireSituationControlMenuItem(
    input = input.toWire(),
    label = label,
    enabled = enabled,
    disabled_reason = disabledReason,
)

private fun SituationControlInput.toWire(): WireSituationControlInput = when (this) {
    SituationControlInput.SkipBackward -> WireSituationControlInput.SkipBackward
    SituationControlInput.FastRewind -> WireSituationControlInput.FastRewind
    SituationControlInput.FastForward -> WireSituationControlInput.FastForward
    SituationControlInput.SkipForward -> WireSituationControlInput.SkipForward
}

private fun WireAppUiState.toUi() = AppUiState(
    activePlan = active_plan?.toUi(),
    ownship = ownship.toUi(),
    flightDataBanner = flight_data_banner.toUi(),
)

private fun WireOwnshipSelection.toUi(): OwnshipSelection = when (this) {
    WireOwnshipSelection.Auto -> OwnshipSelection.Auto
    is WireOwnshipSelection.Source -> OwnshipSelection.Source(source_id)
}

private fun WireOwnshipRenderState.toUi() = OwnshipRenderState(
    mode = mode.toUi(),
    bannerText = banner_text,
    bannerSeverity = banner_severity.toUi(),
    drawAircraft = draw_aircraft,
    drawPredictor = draw_predictor,
    drawCdi = draw_cdi,
    position = position?.toUi(),
    trackDegTrue = track_deg_true,
    orientationDeg = orientation_deg,
    magneticVariationDeg = magnetic_variation_deg,
    speedKt = speed_kt,
    terrainAltitudeBucketFt = terrain_altitude_bucket_ft,
)

private fun WireOwnshipControlModel.toUi() = OwnshipControlModel(
    mode = mode.toUi(),
    selection = selection.toUi(),
    launcherLabel = launcher_label,
    launcherTone = launcher_tone.toUi(),
    launcherTextTone = launcher_text_tone.toUi(),
    sources = sources.map { it.toUi() },
    situationControls = situation_controls.map { it.toUi() },
)

private fun WireOwnshipUiState.toUi() = OwnshipUiState(
    render = render.toUi(),
    controls = controls.toUi(),
)

private fun WireSituationRingCandidate.toUi() = SituationRingCandidate(
    radiusNm = radius_nm,
    label = label,
)

private fun WireMapFollowUiState.toUi() = MapFollowUiState(
    canCenterHere = can_center_here,
    following = following,
    disabledReason = disabled_reason,
)

private fun WirePlaybackUiState.toUi() = PlaybackUiState(
    status = status.toUi(),
    sourcePath = source_path,
    titleLabel = title_label,
    registration = registration,
    icao = icao,
    aircraftType = aircraft_type,
    pointCount = point_count,
    durationSeconds = duration_seconds,
    cursorSeconds = cursor_seconds,
    cursorLabel = cursor_label,
    durationLabel = duration_label,
    rate = rate,
    tickIntervalMs = tick_interval_ms,
    speedProfileNorm = speed_profile_norm,
    altitudeProfileNorm = altitude_profile_norm,
    gapSpans = gap_spans.map { PlaybackGapSpan(it.start_seconds, it.end_seconds) },
)

private fun WireOwnshipSourceMenuItem.toUi() = OwnshipSourceMenuItem(
    sourceId = source_id,
    sourceKind = source_kind.toUi(),
    label = label,
    launcherLabel = launcher_label,
    tone = tone.toUi(),
    enabled = enabled,
    disabledReason = disabled_reason,
    active = active,
    statusLabel = status_label,
)

private fun WireSituationControlMenuItem.toUi() = SituationControlMenuItem(
    input = input.toUi(),
    label = label,
    enabled = enabled,
    disabledReason = disabled_reason,
)

private fun WireSituationControlInput.toUi(): SituationControlInput = when (this) {
    WireSituationControlInput.SkipBackward -> SituationControlInput.SkipBackward
    WireSituationControlInput.FastRewind -> SituationControlInput.FastRewind
    WireSituationControlInput.FastForward -> SituationControlInput.FastForward
    WireSituationControlInput.SkipForward -> SituationControlInput.SkipForward
}

private fun WireOwnshipMode.toUi(): OwnshipMode = when (this) {
    WireOwnshipMode.None -> OwnshipMode.None
    WireOwnshipMode.Live -> OwnshipMode.Live
    WireOwnshipMode.Replay -> OwnshipMode.Replay
    WireOwnshipMode.Simulated -> OwnshipMode.Simulated
}

private fun OwnshipMode.toWire(): WireOwnshipMode = when (this) {
    OwnshipMode.None -> WireOwnshipMode.None
    OwnshipMode.Live -> WireOwnshipMode.Live
    OwnshipMode.Replay -> WireOwnshipMode.Replay
    OwnshipMode.Simulated -> WireOwnshipMode.Simulated
}

private fun WirePlaybackStatus.toUi(): PlaybackStatus = when (this) {
    WirePlaybackStatus.Empty -> PlaybackStatus.Empty
    WirePlaybackStatus.Paused -> PlaybackStatus.Paused
    WirePlaybackStatus.Playing -> PlaybackStatus.Playing
}

private fun WireOwnshipBannerSeverity.toUi(): OwnshipBannerSeverity = when (this) {
    WireOwnshipBannerSeverity.Info -> OwnshipBannerSeverity.Info
    WireOwnshipBannerSeverity.Caution -> OwnshipBannerSeverity.Caution
    WireOwnshipBannerSeverity.Warning -> OwnshipBannerSeverity.Warning
}

private fun OwnshipBannerSeverity.toWire(): WireOwnshipBannerSeverity = when (this) {
    OwnshipBannerSeverity.Info -> WireOwnshipBannerSeverity.Info
    OwnshipBannerSeverity.Caution -> WireOwnshipBannerSeverity.Caution
    OwnshipBannerSeverity.Warning -> WireOwnshipBannerSeverity.Warning
}

private fun WireOwnshipControlTone.toUi(): OwnshipControlTone = when (this) {
    WireOwnshipControlTone.Ready -> OwnshipControlTone.Ready
    WireOwnshipControlTone.Unavailable -> OwnshipControlTone.Unavailable
    WireOwnshipControlTone.Neutral -> OwnshipControlTone.Neutral
}

private fun OwnshipControlTone.toWire(): WireOwnshipControlTone = when (this) {
    OwnshipControlTone.Ready -> WireOwnshipControlTone.Ready
    OwnshipControlTone.Unavailable -> WireOwnshipControlTone.Unavailable
    OwnshipControlTone.Neutral -> WireOwnshipControlTone.Neutral
}

private fun WireOwnshipLauncherTextTone.toUi(): OwnshipLauncherTextTone = when (this) {
    WireOwnshipLauncherTextTone.Normal -> OwnshipLauncherTextTone.Normal
    WireOwnshipLauncherTextTone.Unavailable -> OwnshipLauncherTextTone.Unavailable
}

private fun OwnshipLauncherTextTone.toWire(): WireOwnshipLauncherTextTone = when (this) {
    OwnshipLauncherTextTone.Normal -> WireOwnshipLauncherTextTone.Normal
    OwnshipLauncherTextTone.Unavailable -> WireOwnshipLauncherTextTone.Unavailable
}

private fun WireOwnshipSourceKind.toUi(): OwnshipSourceKind = when (this) {
    WireOwnshipSourceKind.DeviceGps -> OwnshipSourceKind.DeviceGps
    WireOwnshipSourceKind.ExternalGps -> OwnshipSourceKind.ExternalGps
    WireOwnshipSourceKind.ExternalAhrs -> OwnshipSourceKind.ExternalAhrs
    WireOwnshipSourceKind.GpxPlayback -> OwnshipSourceKind.GpxPlayback
    WireOwnshipSourceKind.AdsbTrackPlayback -> OwnshipSourceKind.AdsbTrackPlayback
    WireOwnshipSourceKind.LiveNetworkTrack -> OwnshipSourceKind.LiveNetworkTrack
    WireOwnshipSourceKind.FlightPlanSimulator -> OwnshipSourceKind.FlightPlanSimulator
    WireOwnshipSourceKind.BadAutopilot -> OwnshipSourceKind.BadAutopilot
}

private fun OwnshipSourceKind.toWire(): WireOwnshipSourceKind = when (this) {
    OwnshipSourceKind.DeviceGps -> WireOwnshipSourceKind.DeviceGps
    OwnshipSourceKind.ExternalGps -> WireOwnshipSourceKind.ExternalGps
    OwnshipSourceKind.ExternalAhrs -> WireOwnshipSourceKind.ExternalAhrs
    OwnshipSourceKind.GpxPlayback -> WireOwnshipSourceKind.GpxPlayback
    OwnshipSourceKind.AdsbTrackPlayback -> WireOwnshipSourceKind.AdsbTrackPlayback
    OwnshipSourceKind.LiveNetworkTrack -> WireOwnshipSourceKind.LiveNetworkTrack
    OwnshipSourceKind.FlightPlanSimulator -> WireOwnshipSourceKind.FlightPlanSimulator
    OwnshipSourceKind.BadAutopilot -> WireOwnshipSourceKind.BadAutopilot
}

private fun WireSourceConnectionState.toUi(): SourceConnectionState = when (this) {
    WireSourceConnectionState.Unavailable -> SourceConnectionState.Unavailable
    WireSourceConnectionState.Searching -> SourceConnectionState.Searching
    WireSourceConnectionState.Connected -> SourceConnectionState.Connected
    WireSourceConnectionState.Stale -> SourceConnectionState.Stale
    WireSourceConnectionState.Failed -> SourceConnectionState.Failed
}

private fun SourceConnectionState.toWire(): WireSourceConnectionState = when (this) {
    SourceConnectionState.Unavailable -> WireSourceConnectionState.Unavailable
    SourceConnectionState.Searching -> WireSourceConnectionState.Searching
    SourceConnectionState.Connected -> WireSourceConnectionState.Connected
    SourceConnectionState.Stale -> WireSourceConnectionState.Stale
    SourceConnectionState.Failed -> WireSourceConnectionState.Failed
}

private fun OwnshipSourceRegistration.toCoreJson(json: Json): String =
    kotlinx.serialization.json.buildJsonObject {
        put("source_id", kotlinx.serialization.json.JsonPrimitive(sourceId))
        put("source_kind", kotlinx.serialization.json.JsonPrimitive(sourceKind.toWireName()))
        put("display_name", kotlinx.serialization.json.JsonPrimitive(displayName))
        put("selectable", kotlinx.serialization.json.JsonPrimitive(selectable))
        put("auto_eligible", kotlinx.serialization.json.JsonPrimitive(autoEligible))
    }.toString()

private fun OwnshipSourceStatusUpdate.toCoreJson(json: Json): String =
    kotlinx.serialization.json.buildJsonObject {
        put("source_id", kotlinx.serialization.json.JsonPrimitive(sourceId))
        put("connection_state", kotlinx.serialization.json.JsonPrimitive(connectionState.toWireName()))
        put("enabled", kotlinx.serialization.json.JsonPrimitive(enabled))
        put("status_label", kotlinx.serialization.json.JsonPrimitive(statusLabel))
    }.toString()

private fun SituationSample.toCoreJson(json: Json): String =
    kotlinx.serialization.json.buildJsonObject {
        put("source_id", kotlinx.serialization.json.JsonPrimitive(sourceId))
        put("source_kind", kotlinx.serialization.json.JsonPrimitive(sourceKind.toWireName()))
        put("event_time_epoch_ms", kotlinx.serialization.json.JsonPrimitive(eventTimeEpochMs))
        put("received_time_epoch_ms", kotlinx.serialization.json.JsonPrimitive(receivedTimeEpochMs))
        put(
            "position",
            position?.let { json.encodeToJsonElement(WireLatLon.serializer(), it.toWire()) }
                ?: kotlinx.serialization.json.JsonNull,
        )
        put("horizontal_accuracy_m", horizontalAccuracyM?.let { kotlinx.serialization.json.JsonPrimitive(it) } ?: kotlinx.serialization.json.JsonNull)
        put("vertical_accuracy_m", verticalAccuracyM?.let { kotlinx.serialization.json.JsonPrimitive(it) } ?: kotlinx.serialization.json.JsonNull)
        put("track_deg_true", trackDegTrue?.let { kotlinx.serialization.json.JsonPrimitive(it) } ?: kotlinx.serialization.json.JsonNull)
        put("heading_deg_true", headingDegTrue?.let { kotlinx.serialization.json.JsonPrimitive(it) } ?: kotlinx.serialization.json.JsonNull)
        put("ground_speed_kt", groundSpeedKt?.let { kotlinx.serialization.json.JsonPrimitive(it) } ?: kotlinx.serialization.json.JsonNull)
        put("altitude_msl_ft", altitudeMslFt?.let { kotlinx.serialization.json.JsonPrimitive(it) } ?: kotlinx.serialization.json.JsonNull)
        put("pressure_altitude_ft", pressureAltitudeFt?.let { kotlinx.serialization.json.JsonPrimitive(it) } ?: kotlinx.serialization.json.JsonNull)
        put("vertical_speed_fpm", verticalSpeedFpm?.let { kotlinx.serialization.json.JsonPrimitive(it) } ?: kotlinx.serialization.json.JsonNull)
    }.toString()

private fun OwnshipSelection.toCoreJson(json: Json): String =
    json.encodeToString(WireOwnshipSelectionSerializer, toWire())

private fun SituationControlInput.toCoreJson(json: Json): String =
    json.encodeToString(
        when (this) {
            SituationControlInput.SkipBackward -> "skip_backward"
            SituationControlInput.FastRewind -> "fast_rewind"
            SituationControlInput.FastForward -> "fast_forward"
            SituationControlInput.SkipForward -> "skip_forward"
        },
    )

private fun CoreMapViewport.toCoreJson(json: Json): String =
    json.encodeToString(WireMapViewport.serializer(), toWire())

private fun OwnshipSourceKind.toWireName(): String = when (this) {
    OwnshipSourceKind.DeviceGps -> "device_gps"
    OwnshipSourceKind.ExternalGps -> "external_gps"
    OwnshipSourceKind.ExternalAhrs -> "external_ahrs"
    OwnshipSourceKind.GpxPlayback -> "gpx_playback"
    OwnshipSourceKind.AdsbTrackPlayback -> "adsb_track_playback"
    OwnshipSourceKind.LiveNetworkTrack -> "live_network_track"
    OwnshipSourceKind.FlightPlanSimulator -> "flight_plan_simulator"
    OwnshipSourceKind.BadAutopilot -> "bad_autopilot"
}

private fun SourceConnectionState.toWireName(): String = when (this) {
    SourceConnectionState.Unavailable -> "unavailable"
    SourceConnectionState.Searching -> "searching"
    SourceConnectionState.Connected -> "connected"
    SourceConnectionState.Stale -> "stale"
    SourceConnectionState.Failed -> "failed"
}

@kotlinx.serialization.Serializable
internal data class WireDerivedChartPage(
    val airports: List<WireDerivedChartAirport>,
)

@kotlinx.serialization.Serializable
private data class WireDerivedChartPageState(
    val airports: List<WireDerivedChartAirport>,
    val reference_families: List<WireDerivedChartAirport> = emptyList(),
    val airport_menu_entries: List<WireDerivedChartAirportMenuEntry> = emptyList(),
    val recent_airport_ids: List<String>,
    val selected_airport_id: String,
    val selected_reference_family_id: String? = null,
    val selected_chart_id: String,
    val suggested_chart_ids: List<String> = emptyList(),
    val procedure_geometry_status: WireUiDataStatusState,
)

@kotlinx.serialization.Serializable
private data class WireUiChartPageState(
    val ordered_airport_ids: List<String>,
    val recent_airport_ids: List<String>,
    val plate_target_airport_id: String? = null,
    val selected_airport_id: String,
    val selected_reference_family_id: String? = null,
    val selected_chart_id: String,
    val suggested_chart_ids: List<String> = emptyList(),
)

@kotlinx.serialization.Serializable
private data class WireUiMapLayerToggleState(
    val visible: Boolean = false,
    val enabled: Boolean = false,
    val disabled_reason: String? = null,
)

@kotlinx.serialization.Serializable
private data class WireUiMapLayerState(
    val world_basemap: WireUiMapLayerToggleState = WireUiMapLayerToggleState(),
    val vectors: WireUiMapLayerToggleState = WireUiMapLayerToggleState(),
    val metars: WireUiMapLayerToggleState = WireUiMapLayerToggleState(),
    val nexrad: WireUiMapLayerToggleState = WireUiMapLayerToggleState(),
    val terrain_warning: WireUiMapLayerToggleState = WireUiMapLayerToggleState(),
    val offline_regions: WireUiMapLayerToggleState = WireUiMapLayerToggleState(),
)

@kotlinx.serialization.Serializable
private enum class WireUiStatusSeverity {
    @kotlinx.serialization.SerialName("ok")
    Ok,

    @kotlinx.serialization.SerialName("info")
    Info,

    @kotlinx.serialization.SerialName("caution")
    Caution,

    @kotlinx.serialization.SerialName("warning")
    Warning,

    @kotlinx.serialization.SerialName("unavailable")
    Unavailable,
}

@kotlinx.serialization.Serializable
private enum class WireUiStatusActionStyle {
    @kotlinx.serialization.SerialName("normal")
    Normal,

    @kotlinx.serialization.SerialName("hush")
    Hush,
}

@kotlinx.serialization.Serializable
private data class WireUiStatusAction(
    val id: String,
    val label: String,
    val enabled: Boolean,
    val style: WireUiStatusActionStyle,
)

@kotlinx.serialization.Serializable
private data class WireUiDataStatusBox(
    val id: String,
    val label: String,
    val value: String? = null,
    val severity: WireUiStatusSeverity,
    val drives_caution: Boolean,
    val detail: String,
    val actions: List<WireUiStatusAction>,
    val hushed: Boolean,
)

@kotlinx.serialization.Serializable
private data class WireUiDataStatusState(
    val boxes: List<WireUiDataStatusBox>,
    val launcher_count: String? = null,
    val launcher_severity: WireUiStatusSeverity = WireUiStatusSeverity.Info,
)

@kotlinx.serialization.Serializable
private enum class WireUiDataStatusPageTimeDisplay {
    @kotlinx.serialization.SerialName("ago")
    Ago,

    @kotlinx.serialization.SerialName("old")
    Old,

    @kotlinx.serialization.SerialName("until")
    Until,
}

@kotlinx.serialization.Serializable
private data class WireUiDataStatusPageFact(
    val label: String,
    val value: String,
    val link_url: String? = null,
    val time_utc: String? = null,
    val time_display: WireUiDataStatusPageTimeDisplay? = null,
)

@kotlinx.serialization.Serializable
private data class WireUiDataStatusPageRow(
    val id: String,
    val label: String,
    val value: String,
    val severity: WireUiStatusSeverity,
    val detail: String,
    val facts: List<WireUiDataStatusPageFact> = emptyList(),
)

@kotlinx.serialization.Serializable
private data class WireUiDataStatusPageState(
    val title: String,
    val summary: String,
    val rows: List<WireUiDataStatusPageRow> = emptyList(),
)

@kotlinx.serialization.Serializable
private data class WireUiSettingsSliderStop(
    val id: String,
    val label: String,
)

@kotlinx.serialization.Serializable
private data class WireUiSettingsGridItem(
    val cell: WireFlightDataCell,
    val enabled: Boolean,
)

@kotlinx.serialization.Serializable
private data class WireUiSettingsPageRow(
    val kind: String,
    val id: String,
    val title: String,
    val value_id: String,
    val stops: List<WireUiSettingsSliderStop> = emptyList(),
    val items: List<WireUiSettingsGridItem> = emptyList(),
    val action_id: String,
)

@kotlinx.serialization.Serializable
private data class WireUiSettingsPageState(
    val title: String = "Settings",
    val summary: String = "",
    val rows: List<WireUiSettingsPageRow> = emptyList(),
)

@kotlinx.serialization.Serializable
private data class WireUiDisplayPolicy(
    val keep_screen_on: Boolean,
    val dim_after_ms: Long? = null,
    val dim_brightness: Float,
)

@kotlinx.serialization.Serializable
private data class WireUiDisclaimerState(
    val agreement_id: String = "no-warranty-v1",
    val required: Boolean = true,
    val html: String = "",
    val text: String = "",
    val accept_label: String = "I understand and agree",
)

@kotlinx.serialization.Serializable
private data class WireUiDebugState(
    val tile_labels: Boolean = false,
    val nexrad_tile_labels: Boolean = false,
    val fast_tiles: Boolean = false,
    val offline_simulated_clock_buttons: Boolean = false,
    val plate_flight_plan: Boolean = false,
    val bad_autopilot: Boolean = false,
    val gps_capture: Boolean = false,
    val debug_log_to_developer_server: Boolean = false,
)

@kotlinx.serialization.Serializable
private data class WireUiPlaybackPanelState(
    val visible: Boolean = false,
)

@kotlinx.serialization.Serializable
private data class WireUiSessionSnapshot(
    val session_revision: Long = 0,
    val flight_plan_route_revision: Long = 0,
    val nav_data_epoch: Long = 0,
    val active_nav_db: WireUiNavDbIdentity? = null,
    val next_nav_db_maintenance_epoch_ms: Long? = null,
    val app_ui_state: WireAppUiState = WireAppUiState(),
    val playback_ui_state: WirePlaybackUiState = WirePlaybackUiState(),
    val playback_panel_state: WireUiPlaybackPanelState = WireUiPlaybackPanelState(),
    val map_follow_ui_state: WireMapFollowUiState = WireMapFollowUiState(),
    val map_follow_target_viewport: WireMapViewport? = null,
    val chart_page_state: WireUiChartPageState,
    val map_layer_state: WireUiMapLayerState = WireUiMapLayerState(),
    val data_status_state: WireUiDataStatusState,
    val data_status_page_state: WireUiDataStatusPageState,
    val settings_page_state: WireUiSettingsPageState = WireUiSettingsPageState(),
    val cloud_page_state: UiCloudPageState,
    val offline_package_preferences_json: String = "{\"regions\":{},\"products\":{}}",
    val home_page_state: UiHomePageState,
    val display_policy: WireUiDisplayPolicy? = null,
    val disclaimer_state: WireUiDisclaimerState = WireUiDisclaimerState(),
    val debug_state: WireUiDebugState = WireUiDebugState(),
    val raster_map: WireRasterMapUiState? = null,
    val next_cycle_product_freshness_check_epoch_ms: Long? = null,
)

@kotlinx.serialization.Serializable
private data class WireUiNavDbIdentity(
    val package_id: String,
    val filename: String,
    val contract_id: String? = null,
    val cycle: String? = null,
    val cycle_version: String? = null,
)

@kotlinx.serialization.Serializable
private data class WireNavDbAdvanceResult(
    val disposition: String,
    val snapshot: WireUiSessionSnapshot,
    val retained_artifact_filenames: List<String> = emptyList(),
    val rejection_reason: String? = null,
)

@kotlinx.serialization.Serializable
private data class WireNavDbMaintenanceResult(
    val action: String,
    val snapshot: WireUiSessionSnapshot,
)

@kotlinx.serialization.Serializable
private data class WireRasterMapUiState(
    val selected_map_id: String = "",
    val selected_map_label: String = "",
    val selected_family_id: String,
    val selected_family_label: String = "",
    val selected_family_launcher_label: String = "",
    val min_zoom: Double = 0.0,
    val max_zoom: Double = 0.0,
    val initial_viewport: WireMapViewportSeed,
    val family_options: List<WireMapFamilyOption> = emptyList(),
)

@kotlinx.serialization.Serializable
private data class WireMapFamilyOption(
    val id: String,
    val label: String,
    val launcher_label: String,
    val enabled: Boolean,
    val disabled_reason: String? = null,
    val active: Boolean,
    val has_references: Boolean = false,
)

@kotlinx.serialization.Serializable
private data class WireUiSessionInitResult(
    val handle: Long,
    val snapshot: WireUiSessionSnapshot,
)

@kotlinx.serialization.Serializable
private data class WireProcedureLoadOption(
    val load_id: String,
    val label: String,
)

@kotlinx.serialization.Serializable
internal data class WireDerivedChartAirport(
    val id: String,
    val label: String,
    val charts: List<WireDerivedChartAsset>,
)

@kotlinx.serialization.Serializable(with = WireDerivedChartAirportMenuEntrySerializer::class)
private sealed interface WireDerivedChartAirportMenuEntry

private object WireDerivedChartAirportMenuEntrySerializer :
    kotlinx.serialization.json.JsonContentPolymorphicSerializer<WireDerivedChartAirportMenuEntry>(WireDerivedChartAirportMenuEntry::class) {
    override fun selectDeserializer(
        element: JsonElement,
    ): kotlinx.serialization.DeserializationStrategy<WireDerivedChartAirportMenuEntry> =
        when (val kind = element.jsonObject["kind"]?.jsonPrimitive?.content) {
            "separator" -> WireDerivedChartAirportMenuSeparator.serializer()
            "airport" -> WireDerivedChartAirportMenuAirport.serializer()
            "reference" -> WireDerivedChartAirportMenuReference.serializer()
            "external_link" -> WireDerivedChartAirportMenuExternalLink.serializer()
            else -> error("Unsupported chart airport menu entry variant: $kind")
        }
}

@kotlinx.serialization.Serializable
@kotlinx.serialization.SerialName("separator")
private data class WireDerivedChartAirportMenuSeparator(
    val kind: String = "separator",
    val label: String,
) : WireDerivedChartAirportMenuEntry

@kotlinx.serialization.Serializable
@kotlinx.serialization.SerialName("airport")
private data class WireDerivedChartAirportMenuAirport(
    val kind: String = "airport",
    val airport: WireDerivedChartAirport,
) : WireDerivedChartAirportMenuEntry

@kotlinx.serialization.Serializable
@kotlinx.serialization.SerialName("reference")
private data class WireDerivedChartAirportMenuReference(
    val kind: String = "reference",
    val reference: WireDerivedChartAirport,
) : WireDerivedChartAirportMenuEntry

@kotlinx.serialization.Serializable
@kotlinx.serialization.SerialName("external_link")
private data class WireDerivedChartAirportMenuExternalLink(
    val kind: String = "external_link",
    val label: String,
    val url: String,
) : WireDerivedChartAirportMenuEntry

@kotlinx.serialization.Serializable
internal data class WireDerivedChartAsset(
    val id: String,
    val airport_id: String? = null,
    val collection_id: String = "",
    val label: String,
    val kind: String,
    val folder_category: String,
    val has_thumbnail: Boolean,
    val procedure_geometry_warning_count: Int = 0,
    val georef: WirePlateGeoref? = null,
)

@kotlinx.serialization.Serializable(with = WirePlateGeorefSerializer::class)
internal sealed interface WirePlateGeoref

private object WirePlateGeorefSerializer :
    kotlinx.serialization.json.JsonContentPolymorphicSerializer<WirePlateGeoref>(WirePlateGeoref::class) {
    override fun selectDeserializer(
        element: JsonElement,
    ): kotlinx.serialization.DeserializationStrategy<WirePlateGeoref> =
        when (val kind = element.jsonObject["kind"]?.jsonPrimitive?.content) {
            "plate_transform_v1" -> WirePlateTransformV1.serializer()
            "airport_diagram_transform_v1" -> WireAirportDiagramTransformV1.serializer()
            else -> error("Unsupported PlateGeoref variant: $kind")
        }
}

@kotlinx.serialization.Serializable
@kotlinx.serialization.SerialName("plate_transform_v1")
internal data class WirePlateTransformV1(
    val kind: String = "plate_transform_v1",
    val pixels_per_longitude: Double,
    val pixels_per_latitude: Double,
    val top_left_lon: Double,
    val top_left_lat: Double,
) : WirePlateGeoref

@kotlinx.serialization.Serializable
@kotlinx.serialization.SerialName("airport_diagram_transform_v1")
internal data class WireAirportDiagramTransformV1(
    val kind: String = "airport_diagram_transform_v1",
    val pixel_x_from_lon: Double,
    val pixel_x_from_lat: Double,
    val pixel_x_offset: Double,
    val pixel_y_from_lon: Double,
    val pixel_y_from_lat: Double,
    val pixel_y_offset: Double,
) : WirePlateGeoref

internal fun WireDerivedChartPage.toUi() = ChartPageFixture(
    airports = airports.map { it.toUi() },
)

data class DerivedChartPageState(
    val airports: List<ChartAirport>,
    val referenceFamilies: List<ChartAirport>,
    val airportMenuEntries: List<ChartAirportMenuEntry>,
    val recentAirportIds: List<String>,
    val selectedAirportId: String,
    val selectedReferenceFamilyId: String?,
    val selectedChartId: String,
    val suggestedChartIds: List<String>,
    val procedureGeometryStatus: UiDataStatusState,
)

data class UiSessionSnapshot(
    val sessionRevision: Long,
    val flightPlanRouteRevision: Long,
    val navDataEpoch: Long,
    val activeNavDb: UiNavDbIdentity?,
    val nextNavDbMaintenanceEpochMs: Long?,
    val appUiState: AppUiState,
    val playbackUiState: PlaybackUiState,
    val playbackPanelState: UiPlaybackPanelState,
    val mapFollowUiState: MapFollowUiState,
    val mapFollowTargetViewport: CoreMapViewport?,
    val chartPageState: UiChartPageState,
    val mapLayerState: UiMapLayerState,
    val dataStatusState: UiDataStatusState,
    val dataStatusPageState: UiDataStatusPageState,
    val settingsPageState: UiSettingsPageState,
    val cloudPageState: UiCloudPageState,
    val offlinePackagePreferencesJson: String,
    val homePageState: UiHomePageState,
    val displayPolicy: UiDisplayPolicy?,
    val disclaimerState: UiDisclaimerState,
    val debugState: UiDebugState,
    val rasterMap: RasterMapUiState?,
    val nextCycleProductFreshnessCheckEpochMs: Long?,
)

data class UiNavDbIdentity(
    val packageId: String,
    val filename: String,
    val contractId: String?,
    val cycle: String?,
    val cycleVersion: String?,
)

data class NavDbAdvanceUiResult(
    val adopted: Boolean,
    val snapshot: UiSessionSnapshot,
    val retainedArtifactFilenames: Set<String>,
    val rejectionReason: String?,
)

data class NavDbMaintenanceUiResult(
    val shouldAttemptAdvance: Boolean,
    val snapshot: UiSessionSnapshot,
)

data class MapOverlayQueryOutcome(
    val overlay: MapOverlayQueryResult,
    val invalidations: List<String>,
)

enum class UiStatusSeverity {
    Ok,
    Info,
    Caution,
    Warning,
    Unavailable,
}

enum class UiStatusActionStyle {
    Normal,
    Hush,
}

data class UiStatusAction(
    val id: String,
    val label: String,
    val enabled: Boolean,
    val style: UiStatusActionStyle,
)

data class UiDataStatusBox(
    val id: String,
    val label: String,
    val value: String?,
    val severity: UiStatusSeverity,
    val drivesCaution: Boolean,
    val detail: String,
    val actions: List<UiStatusAction>,
    val hushed: Boolean,
)

data class UiDataStatusState(
    val boxes: List<UiDataStatusBox>,
    val launcherCount: String?,
    val launcherSeverity: UiStatusSeverity,
)

enum class UiDataStatusPageTimeDisplay {
    Ago,
    Old,
    Until,
}

data class UiDataStatusPageFact(
    val label: String,
    val value: String,
    val linkUrl: String?,
    val timeUtc: String?,
    val timeDisplay: UiDataStatusPageTimeDisplay?,
)

data class UiDataStatusPageRow(
    val id: String,
    val label: String,
    val value: String,
    val severity: UiStatusSeverity,
    val detail: String,
    val facts: List<UiDataStatusPageFact>,
)

data class UiDataStatusPageState(
    val title: String,
    val summary: String,
    val rows: List<UiDataStatusPageRow>,
)

data class UiSettingsSliderStop(
    val id: String,
    val label: String,
)

data class UiSettingsGridItem(
    val cell: FlightDataCell,
    val enabled: Boolean,
)

data class UiSettingsPageRow(
    val kind: String,
    val id: String,
    val title: String,
    val valueId: String,
    val stops: List<UiSettingsSliderStop>,
    val items: List<UiSettingsGridItem>,
    val actionId: String,
)

data class UiSettingsPageState(
    val title: String,
    val summary: String,
    val rows: List<UiSettingsPageRow>,
)

data class UiDisplayPolicy(
    val keepScreenOn: Boolean,
    val dimAfterMs: Long?,
    val dimBrightness: Float,
)

data class UiDisclaimerState(
    val agreementId: String,
    val required: Boolean,
    val html: String,
    val text: String,
    val acceptLabel: String,
)

data class UiDebugState(
    val tileLabels: Boolean,
    val nexradTileLabels: Boolean,
    val fastTiles: Boolean,
    val offlineSimulatedClockButtons: Boolean,
    val plateFlightPlan: Boolean,
    val badAutopilot: Boolean,
    val gpsCapture: Boolean,
    val debugLogToDeveloperServer: Boolean,
)

data class UiPlaybackPanelState(
    val visible: Boolean,
)

data class UiChartPageState(
    val orderedAirportIds: List<String>,
    val recentAirportIds: List<String>,
    val plateTargetAirportId: String?,
    val selectedAirportId: String,
    val selectedReferenceFamilyId: String?,
    val selectedChartId: String,
    val suggestedChartIds: List<String>,
)

private fun WireRasterMapUiState.toUi() = RasterMapUiState(
    selectedMapId = selected_map_id,
    selectedMapLabel = selected_map_label,
    selectedFamilyId = selected_family_id,
    selectedFamilyLabel = selected_family_label,
    selectedFamilyLauncherLabel = selected_family_launcher_label,
    minZoom = min_zoom,
    maxZoom = max_zoom,
    initialViewport = MapViewportSeed(
        lat = initial_viewport.lat,
        lon = initial_viewport.lon,
        zoom = initial_viewport.zoom,
    ),
    familyOptions = family_options.map { it.toUi() },
)

private fun WireMapFamilyOption.toUi() = MapFamilyOption(
    id = id,
    label = label,
    launcherLabel = launcher_label,
    enabled = enabled,
    disabledReason = disabled_reason,
    active = active,
    hasReferences = has_references,
)

internal fun decodeRasterMapUiStateForTesting(rasterMapJson: String): RasterMapUiState =
    NativeAppCoreJson.decodeFromString<WireRasterMapUiState>(rasterMapJson).toUi()

private fun WireDerivedChartPageState.toUi() = DerivedChartPageState(
    airports = airports.map { it.toUi() },
    referenceFamilies = reference_families.map { it.toUi() },
    airportMenuEntries = airport_menu_entries.map { it.toUi() },
    recentAirportIds = recent_airport_ids,
    selectedAirportId = selected_airport_id,
    selectedReferenceFamilyId = selected_reference_family_id,
    selectedChartId = selected_chart_id,
    suggestedChartIds = suggested_chart_ids,
    procedureGeometryStatus = procedure_geometry_status.toUi(),
)

private fun WireUiChartPageState.toUi() = UiChartPageState(
    orderedAirportIds = ordered_airport_ids,
    recentAirportIds = recent_airport_ids,
    plateTargetAirportId = plate_target_airport_id,
    selectedAirportId = selected_airport_id,
    selectedReferenceFamilyId = selected_reference_family_id,
    selectedChartId = selected_chart_id,
    suggestedChartIds = suggested_chart_ids,
)

private fun WireUiMapLayerToggleState.toUi() = UiMapLayerToggleState(
    visible = visible,
    enabled = enabled,
    disabledReason = disabled_reason,
)

private fun WireUiMapLayerState.toUi() = UiMapLayerState(
    worldBasemap = world_basemap.toUi(),
    vectors = vectors.toUi(),
    metars = metars.toUi(),
    nexrad = nexrad.toUi(),
    terrainWarning = terrain_warning.toUi(),
    offlineRegions = offline_regions.toUi(),
)

private fun WireUiStatusSeverity.toUi() = when (this) {
    WireUiStatusSeverity.Ok -> UiStatusSeverity.Ok
    WireUiStatusSeverity.Info -> UiStatusSeverity.Info
    WireUiStatusSeverity.Caution -> UiStatusSeverity.Caution
    WireUiStatusSeverity.Warning -> UiStatusSeverity.Warning
    WireUiStatusSeverity.Unavailable -> UiStatusSeverity.Unavailable
}

private fun WireUiStatusActionStyle.toUi() = when (this) {
    WireUiStatusActionStyle.Normal -> UiStatusActionStyle.Normal
    WireUiStatusActionStyle.Hush -> UiStatusActionStyle.Hush
}

private fun WireUiStatusAction.toUi() = UiStatusAction(
    id = id,
    label = label,
    enabled = enabled,
    style = style.toUi(),
)

private fun WireUiDataStatusBox.toUi() = UiDataStatusBox(
    id = id,
    label = label,
    value = value,
    severity = severity.toUi(),
    drivesCaution = drives_caution,
    detail = detail,
    actions = actions.map { it.toUi() },
    hushed = hushed,
)

private fun WireUiDataStatusState.toUi() = UiDataStatusState(
    boxes = boxes.map { it.toUi() },
    launcherCount = launcher_count,
    launcherSeverity = launcher_severity.toUi(),
)

private fun WireUiDataStatusPageTimeDisplay.toUi() = when (this) {
    WireUiDataStatusPageTimeDisplay.Ago -> UiDataStatusPageTimeDisplay.Ago
    WireUiDataStatusPageTimeDisplay.Old -> UiDataStatusPageTimeDisplay.Old
    WireUiDataStatusPageTimeDisplay.Until -> UiDataStatusPageTimeDisplay.Until
}

private fun WireUiDataStatusPageFact.toUi() = UiDataStatusPageFact(
    label = label,
    value = value,
    linkUrl = link_url,
    timeUtc = time_utc,
    timeDisplay = time_display?.toUi(),
)

private fun WireUiDataStatusPageRow.toUi() = UiDataStatusPageRow(
    id = id,
    label = label,
    value = value,
    severity = severity.toUi(),
    detail = detail,
    facts = facts.map { it.toUi() },
)

private fun WireUiDataStatusPageState.toUi() = UiDataStatusPageState(
    title = title,
    summary = summary,
    rows = rows.map { it.toUi() },
)

private fun WireUiSettingsSliderStop.toUi() = UiSettingsSliderStop(
    id = id,
    label = label,
)

private fun WireUiSettingsGridItem.toUi() = UiSettingsGridItem(
    cell = cell.toUi(),
    enabled = enabled,
)

private fun WireUiSettingsPageRow.toUi() = UiSettingsPageRow(
    kind = kind,
    id = id,
    title = title,
    valueId = value_id,
    stops = stops.map { it.toUi() },
    items = items.map { it.toUi() },
    actionId = action_id,
)

private fun WireUiSettingsPageState.toUi() = UiSettingsPageState(
    title = title,
    summary = summary,
    rows = rows.map { it.toUi() },
)

private fun WireUiDisplayPolicy.toUi() = UiDisplayPolicy(
    keepScreenOn = keep_screen_on,
    dimAfterMs = dim_after_ms,
    dimBrightness = dim_brightness,
)

private fun WireUiDisclaimerState.toUi() = UiDisclaimerState(
    agreementId = agreement_id,
    required = required,
    html = html,
    text = text,
    acceptLabel = accept_label,
)

private fun WireUiDebugState.toUi() = UiDebugState(
    tileLabels = tile_labels,
    nexradTileLabels = nexrad_tile_labels,
    fastTiles = fast_tiles,
    offlineSimulatedClockButtons = offline_simulated_clock_buttons,
    plateFlightPlan = plate_flight_plan,
    badAutopilot = bad_autopilot,
    gpsCapture = gps_capture,
    debugLogToDeveloperServer = debug_log_to_developer_server,
)

private fun WireUiPlaybackPanelState.toUi() = UiPlaybackPanelState(
    visible = visible,
)

private fun WireUiSessionSnapshot.toUi() = UiSessionSnapshot(
    sessionRevision = session_revision,
    flightPlanRouteRevision = flight_plan_route_revision,
    navDataEpoch = nav_data_epoch,
    activeNavDb = active_nav_db?.let {
        UiNavDbIdentity(
            packageId = it.package_id,
            filename = it.filename,
            contractId = it.contract_id,
            cycle = it.cycle,
            cycleVersion = it.cycle_version,
        )
    },
    nextNavDbMaintenanceEpochMs = next_nav_db_maintenance_epoch_ms,
    appUiState = app_ui_state.toUi(),
    playbackUiState = playback_ui_state.toUi(),
    playbackPanelState = playback_panel_state.toUi(),
    mapFollowUiState = map_follow_ui_state.toUi(),
    mapFollowTargetViewport = map_follow_target_viewport?.toUi(),
    chartPageState = chart_page_state.toUi(),
    mapLayerState = map_layer_state.toUi(),
    dataStatusState = data_status_state.toUi(),
    dataStatusPageState = data_status_page_state.toUi(),
    settingsPageState = settings_page_state.toUi(),
    cloudPageState = cloud_page_state,
    offlinePackagePreferencesJson = offline_package_preferences_json,
    homePageState = home_page_state,
    displayPolicy = display_policy?.toUi(),
    disclaimerState = disclaimer_state.toUi(),
    debugState = debug_state.toUi(),
    rasterMap = raster_map?.toUi(),
    nextCycleProductFreshnessCheckEpochMs = next_cycle_product_freshness_check_epoch_ms,
)

internal fun WireDerivedChartAirport.toUi() = ChartAirport(
    id = id,
    label = label,
    charts = charts.map { it.toUi() },
)

private fun WireDerivedChartAirportMenuEntry.toUi(): ChartAirportMenuEntry =
    when (this) {
        is WireDerivedChartAirportMenuSeparator -> ChartAirportMenuEntry.Separator(label)
        is WireDerivedChartAirportMenuAirport -> ChartAirportMenuEntry.Airport(airport.toUi())
        is WireDerivedChartAirportMenuReference -> ChartAirportMenuEntry.Reference(reference.toUi())
        is WireDerivedChartAirportMenuExternalLink -> ChartAirportMenuEntry.ExternalLink(label, url)
    }

internal fun WireDerivedChartAsset.toUi() = ChartAsset(
    id = id,
    airportId = airport_id,
    collectionId = collection_id,
    label = label,
    kind = kind,
    folderCategory = folder_category,
    hasThumbnail = has_thumbnail,
    procedureGeometryWarningCount = procedure_geometry_warning_count,
    georef = georef?.toUi(),
)

private fun WirePlateGeoref.toUi(): PlateGeoref =
    when (this) {
        is WirePlateTransformV1 -> PlateGeoref.PlateTransformV1(
            pixelsPerLongitude = pixels_per_longitude,
            pixelsPerLatitude = pixels_per_latitude,
            topLeftLon = top_left_lon,
            topLeftLat = top_left_lat,
        )
        is WireAirportDiagramTransformV1 -> PlateGeoref.AirportDiagramTransformV1(
            pixelXFromLon = pixel_x_from_lon,
            pixelXFromLat = pixel_x_from_lat,
            pixelXOffset = pixel_x_offset,
            pixelYFromLon = pixel_y_from_lon,
            pixelYFromLat = pixel_y_from_lat,
            pixelYOffset = pixel_y_offset,
        )
    }

@kotlinx.serialization.Serializable
private data class WireMapViewport(
    val center: WireLatLon,
    val zoom: Double,
    val rotation_deg: Double,
    val pitch_deg: Double,
)

private fun WireMapViewport.toUi() = CoreMapViewport(
    center = center.toUi(),
    zoom = zoom,
    rotationDeg = rotation_deg,
    pitchDeg = pitch_deg,
)

private fun CoreMapViewport.toWire() = WireMapViewport(
    center = center.toWire(),
    zoom = zoom,
    rotation_deg = rotationDeg,
    pitch_deg = pitchDeg,
)

private fun MapViewportState.toCoreViewport() = CoreMapViewport(
    center = viewportCenterLatLon(this).let { LatLonPoint(lat = it.first, lon = it.second) },
    zoom = zoom,
    rotationDeg = rotationDeg,
    pitchDeg = 0.0,
)

private fun PointTilePayload.toWire() = WirePointTilePayload(
    schema_version = schemaVersion,
    layer = layer,
    z = z,
    x = x,
    y = y,
    records = records.map { it.toWire() },
)

private fun PointVectorRecord.toWire() = WirePointVectorRecord(
    id = id,
    kind = kind,
    lat = lat,
    lon = lon,
    label = label,
    style_class = styleClass,
    towered = towered,
    fuel_available = fuelAvailable,
    has_paved_runway = hasPavedRunway,
    heliport = heliport,
    has_water_runway = hasWaterRunway,
    longest_runway_heading_true_deg = longestRunwayHeadingTrueDeg,
)

private fun WireMapOverlayQueryResult.toUi() = MapOverlayQueryResult(
    visibleFeatures = visible_features.map { it.toUi() },
    flightPlanFeatures = flight_plan_features.map { it.toUi() },
    visibleMetars = visible_metars.map { it.toUi() },
    visiblePireps = visible_pireps.map { it.toUi() },
    airspacePaths = airspace_paths.map { it.toUi() },
    tfrPaths = tfr_paths.map { it.toUi() },
    airspaceLabels = airspace_labels.map { it.toUi() },
    offlineRegions = offline_regions.map { it.toUi() },
)

private fun WireAirspaceFeatureRequest.toUi() = AirspaceFeatureRequest(
    id = id,
    path = path,
)

private fun WireTerrainOverlayQueryResult.toUi() = TerrainOverlayQueryResult(
    status = status.toUi(),
    tileRequests = tile_requests.map { it.toUi() },
    altitudeBucketFt = altitude_bucket_ft,
    frameKey = frame_key,
    schedule = schedule.toUi(),
)

private fun WireTerrainOverlayScheduleDecision.toUi() = TerrainOverlayScheduleDecision(
    cachedCount = cached_count,
    inFlightCount = in_flight_count,
    missingCount = missing_count,
    frameComplete = frame_complete,
    workBatch = work_batch.map { it.toUi() },
)

private fun WireTerrainOverlayStatus.toUi(): TerrainOverlayStatus = when (this) {
    is WireTerrainOverlayStatusHidden -> TerrainOverlayStatus.Hidden
    is WireTerrainOverlayStatusNoPosition -> TerrainOverlayStatus.NoPosition
    is WireTerrainOverlayStatusNoAltitude -> TerrainOverlayStatus.NoAltitude
    is WireTerrainOverlayStatusTooManyTiles -> TerrainOverlayStatus.TooManyTiles(count)
    is WireTerrainOverlayStatusUnavailable -> TerrainOverlayStatus.Unavailable(reason)
    is WireTerrainOverlayStatusReady -> TerrainOverlayStatus.Ready(count)
}

private fun WireTerrainOverlayTileRequest.toUi() = TerrainOverlayTileRequest(
    key = key,
    cacheKey = cache_key,
    productId = product_id,
    path = path,
    sourceTiles = source_tiles.map { it.toUi() },
    z = z,
    x = x,
    yTms = y_tms,
    left = left,
    top = top,
    size = size,
)

private fun WireTerrainOverlaySourceTile.toUi() = TerrainOverlaySourceTile(
    productId = product_id,
    path = path,
    resource = resource?.toUi(),
)

private fun WireCoreResourceRequest.toUi() = CoreResourceRequest(
    id = id,
    source = parseCoreResourceSource(source),
    optional = optional,
)

private fun WireVectorTileRequest.toUi() = VectorTileRequest(
    layer = layer,
    z = z,
    x = x,
    y = y,
)

private fun WireVisibleMapFeature.toUi() = VisibleMapFeature(
    id = id,
    kind = kind,
    label = label,
    symbolKind = symbol_kind,
    styleClass = style_class,
    obstacleVariant = obstacle_variant,
    obstacleTone = obstacle_tone,
    screenX = screen_x,
    screenY = screen_y,
    towered = towered,
    fuelAvailable = fuel_available,
    hasPavedRunway = has_paved_runway,
    heliport = heliport,
    hasWaterRunway = has_water_runway,
    runwayLengthRatio = runway_length_ratio,
    longestRunwayHeadingTrueDeg = longest_runway_heading_true_deg,
    labelStyle = label_style,
)

private fun WireVisibleMetarFeature.toUi() = VisibleMetarFeature(
    stationId = station_id,
    screenX = screen_x,
    screenY = screen_y,
    flightCategory = flight_category,
    ceilingAmount = ceiling_amount,
)

private fun WireVisiblePirepFeature.toUi() = VisiblePirepFeature(
    id = id,
    screenX = screen_x,
    screenY = screen_y,
    symbol = symbol,
    icing = icing,
    turbulence = turbulence,
)

private fun WireAirspaceDisplayStroke.toUi() = AirspaceDisplayStroke(
    colorKey = color_key,
    widthPx = width_px,
    dashPx = dash_px,
    lineCap = line_cap,
)

private fun WireAirspaceDisplayStyle.toUi() = AirspaceDisplayStyle(
    fillColorKey = fill_color_key,
    fillOpacity = fill_opacity,
    strokes = strokes.map { it.toUi() },
)

private fun WireAirspaceScreenPoint.toUi() = AirspaceScreenPoint(
    x = x,
    y = y,
)

private fun WireAirspaceDisplaySubpath.toUi() = AirspaceDisplaySubpath(
    closed = closed,
    points = points.map { it.toUi() },
)

private fun WireAirspaceDisplayDecoration.toUi() = AirspaceDisplayDecoration(
    colorKey = color_key,
    widthPx = width_px,
    lineCap = line_cap,
    paths = paths.map { it.toUi() },
    segments = segments.mapNotNull { it.toAirspaceDisplayDecorationSegmentOrNull() },
)

private fun List<Double>.toAirspaceDisplayDecorationSegmentOrNull(): AirspaceDisplayDecorationSegment? =
    if (size >= 4) {
        AirspaceDisplayDecorationSegment(
            x1 = this[0],
            y1 = this[1],
            x2 = this[2],
            y2 = this[3],
        )
    } else {
        null
    }

private fun WireAirspaceDisplayPath.toUi() = AirspaceDisplayPath(
    id = id,
    name = name,
    styleKey = style_key,
    style = style.toUi(),
    paths = paths.map { it.toUi() },
    decorations = decorations.map { it.toUi() },
)

private fun WireAirspaceDisplayLabel.toUi() = AirspaceDisplayLabel(
    featureId = feature_id,
    glyph = glyph.toUi(),
    screenX = screen_x,
    screenY = screen_y,
)

private fun WireOfflineRegionDisplay.toUi() = OfflineRegionDisplay(
    id = id,
    kind = kind,
    regionId = region_id,
    label = label,
    colorKey = color_key,
    points = points.map { it.toUi() },
    labelX = label_x,
    labelY = label_y,
)

private fun WireAirspaceLimitGlyph.toUi() = AirspaceLimitGlyph(
    upper = upper,
    lower = lower,
    styleKey = style_key,
    colorKey = color_key,
)

private fun WireMapSelectionQueryResult.toUi() = MapSelectionQueryResult(
    clickLat = click_lat,
    clickLon = click_lon,
    initialSelectedItemId = initial_selected_item_id,
    categories = categories.map { it.toUi() },
)

private fun WireMapSelectionForNavRefResult.toUi() = MapSelectionForNavRefResult(
    position = position.toUi(),
    targetZoom = target_zoom,
    selection = selection.toUi(),
    selectedItemId = selected_item_id,
)

private fun WireMapSelectionCategory.toUi() = MapSelectionCategory(
    id = id,
    label = label,
    items = items.map { it.toUi() },
)

private fun WireMapSelectionItem.toUi() = MapSelectionItem(
    id = id,
    label = label,
    sublabel = sublabel,
    description = description,
    secondaryDescription = secondary_description,
    detailText = detail_text,
    highlight = highlight.toUi(),
    navRef = nav_ref?.toUi(),
    symbolFeature = symbol_feature?.toUi(),
    metarFeature = metar_feature?.toUi(),
    pirepFeature = pirep_feature?.toUi(),
    airspaceIcon = airspace_icon?.toUi(),
    actions = actions.map { it.toUi() },
)

private fun WireMapSelectionHighlight.toUi(): MapSelectionHighlight = when (this) {
    is WireMapSelectionHighlightFeatureRef -> MapSelectionHighlight.FeatureRef(id)
    is WireMapSelectionHighlightMetar -> MapSelectionHighlight.Metar(station_id)
    is WireMapSelectionHighlightPirep -> MapSelectionHighlight.Pirep(id)
    is WireMapSelectionHighlightOfflineRegion -> MapSelectionHighlight.OfflineRegion(id)
    is WireMapSelectionHighlightSpot -> MapSelectionHighlight.Spot(lat, lon)
    is WireMapSelectionHighlight.FeatureRef -> MapSelectionHighlight.FeatureRef(id)
    is WireMapSelectionHighlight.Metar -> MapSelectionHighlight.Metar(station_id)
    is WireMapSelectionHighlight.Pirep -> MapSelectionHighlight.Pirep(id)
    is WireMapSelectionHighlight.OfflineRegion -> MapSelectionHighlight.OfflineRegion(id)
    is WireMapSelectionHighlight.Spot -> MapSelectionHighlight.Spot(lat, lon)
}

private fun WireMapSelectionAction.toUi() = MapSelectionAction(
    id = id,
    label = label,
    enabled = enabled,
    displayOnly = display_only,
    detailText = detail_text,
    detailTitle = detail_title,
    detailStatus = detail_status?.toUi(),
    disabledReason = disabled_reason,
    weatherDetail = weather_detail?.toUi(),
    airportInfoAirportId = airport_info_airport_id,
    airspaceLimit = airspace_limit?.toUi(),
    sessionAction = session_action,
    flightPlanRowAction = flight_plan_row_action?.toUi(),
    navigation = navigation?.toUi(),
)

private fun WireMapSelectionDetailStatus.toUi() = MapSelectionDetailStatus(
    text = text,
    colorKey = color_key,
)

private fun WireWeatherDetailUiView.toUi() = WeatherDetailUiView(
    stationId = station_id,
    advisoryText = advisory_text,
    metarText = metar_text,
    metarAgeLabel = metar_age_label,
    metarAgeWarning = metar_age_warning,
    tafText = taf_text,
    tafAgeLabel = taf_age_label,
    tafAgeWarning = taf_age_warning,
    notams = notams.map { it.toUi() },
)

private fun WireAirportNotamUiView.toUi() = AirportNotamUiView(
    id = id,
    label = label,
    text = text,
)

private fun WeatherDetailUiView.toWire() = WireWeatherDetailUiView(
    station_id = stationId,
    advisory_text = advisoryText,
    metar_text = metarText,
    metar_age_label = metarAgeLabel,
    metar_age_warning = metarAgeWarning,
    taf_text = tafText,
    taf_age_label = tafAgeLabel,
    taf_age_warning = tafAgeWarning,
    notams = notams.map { it.toWire() },
)

private fun AirportNotamUiView.toWire() = WireAirportNotamUiView(
    id = id,
    label = label,
    text = text,
)

private fun WireMapSelectionFlightPlanRowAction.toUi() = MapSelectionFlightPlanRowAction(
    rowUid = row_uid,
    actionUid = action_uid,
)

private fun WireMapSelectionNavigationAction.toUi(): MapSelectionNavigationAction? =
    when (kind) {
        "open_plate_target" -> {
            val airportId = airport_id ?: return null
            val target = target ?: return null
            val chartId = chart_id ?: return null
            MapSelectionNavigationAction.OpenPlateTarget(
                airportId = airportId,
                target = target,
                chartId = chartId,
            )
        }
        else -> null
    }

private fun WireNavSymbolFeature.toUi() = NavSymbolFeature(
    kind = kind,
    label = label,
    symbolKind = symbol_kind,
    styleClass = style_class,
    obstacleVariant = obstacle_variant,
    obstacleTone = obstacle_tone,
    towered = towered,
    fuelAvailable = fuel_available,
    hasPavedRunway = has_paved_runway,
    heliport = heliport,
    hasWaterRunway = has_water_runway,
    runwayLengthRatio = runway_length_ratio,
    longestRunwayHeadingTrueDeg = longest_runway_heading_true_deg,
)

private fun NavSymbolFeature.toWire() = WireNavSymbolFeature(
    kind = kind,
    label = label,
    symbol_kind = symbolKind,
    style_class = styleClass,
    obstacle_variant = obstacleVariant,
    obstacle_tone = obstacleTone,
    towered = towered,
    fuel_available = fuelAvailable,
    has_paved_runway = hasPavedRunway,
    heliport = heliport,
    has_water_runway = hasWaterRunway,
    runway_length_ratio = runwayLengthRatio,
    longest_runway_heading_true_deg = longestRunwayHeadingTrueDeg,
)

private fun WireAirwaySuggestion.toUi() = AirwaySuggestion(
    airwayName = airway_name,
    nearestBranchKey = nearest_branch_key,
    nearestNavRef = nearest_nav_ref.toUi(),
    nearestSequence = nearest_sequence,
    distanceFromAnchorNm = distance_from_anchor_nm,
)

private fun WireWaypointIdentifierSuggestion.toUi() = WaypointIdentifierSuggestion(
    identifier = identifier,
    navRef = nav_ref.toUi(),
    kind = kind,
    displayName = display_name,
    distanceFromAnchorNm = distance_from_anchor_nm,
)

private fun AirwaySuggestion.toWire() = WireAirwaySuggestion(
    airway_name = airwayName,
    nearest_branch_key = nearestBranchKey,
    nearest_nav_ref = nearestNavRef.toWire(),
    nearest_sequence = nearestSequence,
    distance_from_anchor_nm = distanceFromAnchorNm,
)

private fun WireAirwayPresentationPlan.toUi() = AirwayPresentationPlan(
    airwayName = airway_name,
    branchKey = branch_key,
    points = points.map { it.toUi() },
    suggestedEntryUid = suggested_entry_uid,
    suggestedExitUid = suggested_exit_uid,
)

private fun AirwayPresentationPlan.toWire() = WireAirwayPresentationPlan(
    airway_name = airwayName,
    branch_key = branchKey,
    points = points.map { it.toWire() },
    suggested_entry_uid = suggestedEntryUid,
    suggested_exit_uid = suggestedExitUid,
)

private fun WireAirwayPresentationPoint.toUi() = AirwayPresentationPoint(
    uid = uid,
    sequence = sequence,
    navRef = nav_ref.toUi(),
)

private fun AirwayPresentationPoint.toWire() = WireAirwayPresentationPoint(
    uid = uid,
    sequence = sequence,
    nav_ref = navRef.toWire(),
)

private fun LatLonPoint.toWire() = WireLatLon(lat = lat, lon = lon)

private fun WireLatLon.toUi() = LatLonPoint(lat = lat, lon = lon)

private fun ProcedureKind.toWire() = when (this) {
    ProcedureKind.Sid -> WireProcedureKind.Sid
    ProcedureKind.Star -> WireProcedureKind.Star
    ProcedureKind.Approach -> WireProcedureKind.Approach
}

private fun WireProcedureKind.toUi() = when (this) {
    WireProcedureKind.Sid -> ProcedureKind.Sid
    WireProcedureKind.Star -> ProcedureKind.Star
    WireProcedureKind.Approach -> ProcedureKind.Approach
}

private fun WireProcedureSummary.toUi() = ProcedureSummary(
    airportId = airport_id,
    procedureId = procedure_id,
    displayLabel = display_label,
    kind = kind.toUi(),
)

private fun WireProcedureLoadOption.toUi() = ProcedureLoadOption(
    loadId = load_id,
    label = label,
)

private fun WireProcedureSpecChoice.toUi() = ProcedureSpecChoice(
    runwayTransition = runway_transition,
    enrouteTransition = enroute_transition,
)

private fun WireProcedureOptions.toUi() = ProcedureOptions(
    airportId = airport_id,
    procedureId = procedure_id,
    kind = kind.toUi(),
    runwayTransitions = runway_transitions,
    enrouteTransitions = enroute_transitions,
    hasCommonSegment = has_common_segment,
    validChoices = valid_choices.map { it.toUi() },
)

private fun WirePlanLeg.toUiPlanLeg() = PlanLeg(
    from = from.toUi(),
    to = to.toUi(),
    airway = airway,
)

private fun WireRouteComponentViewKind.toUi() = when (this) {
    WireRouteComponentViewKind.Waypoint -> RouteComponentViewKind.Waypoint
    WireRouteComponentViewKind.Airway -> RouteComponentViewKind.Airway
    WireRouteComponentViewKind.Procedure -> RouteComponentViewKind.Procedure
}

private fun RouteComponentViewKind.toWire() = when (this) {
    RouteComponentViewKind.Waypoint -> WireRouteComponentViewKind.Waypoint
    RouteComponentViewKind.Airway -> WireRouteComponentViewKind.Airway
    RouteComponentViewKind.Procedure -> WireRouteComponentViewKind.Procedure
}

private fun WireDirectToUiView.toUi() = DirectToUiView(
    start = start.toUi(),
    target = target.toUi(),
    targetRowId = target_row_id,
    onPlanTarget = on_plan_target,
)

private fun DirectToUiView.toWire() = WireDirectToUiView(
    start = start.toWire(),
    target = target.toWire(),
    target_row_id = targetRowId,
    on_plan_target = onPlanTarget,
)

private fun WireFlightPlanControlId.toUi() = when (this) {
    WireFlightPlanControlId.ActivateNextLeg -> FlightPlanControlId.ActivateNextLeg
    WireFlightPlanControlId.RestoreDirectTo -> FlightPlanControlId.RestoreDirectTo
    WireFlightPlanControlId.SequenceActiveLeg -> FlightPlanControlId.SequenceActiveLeg
    WireFlightPlanControlId.StopNavigation -> FlightPlanControlId.StopNavigation
    WireFlightPlanControlId.SuspendSequencing -> FlightPlanControlId.SuspendSequencing
    WireFlightPlanControlId.UnsuspendSequencing -> FlightPlanControlId.UnsuspendSequencing
}

private fun FlightPlanControlId.toWire() = when (this) {
    FlightPlanControlId.ActivateNextLeg -> WireFlightPlanControlId.ActivateNextLeg
    FlightPlanControlId.RestoreDirectTo -> WireFlightPlanControlId.RestoreDirectTo
    FlightPlanControlId.SequenceActiveLeg -> WireFlightPlanControlId.SequenceActiveLeg
    FlightPlanControlId.StopNavigation -> WireFlightPlanControlId.StopNavigation
    FlightPlanControlId.SuspendSequencing -> WireFlightPlanControlId.SuspendSequencing
    FlightPlanControlId.UnsuspendSequencing -> WireFlightPlanControlId.UnsuspendSequencing
}

private fun WireFlightPlanControlUiView.toUi() = FlightPlanControlUiView(
    id = id.toUi(),
    label = label,
    enabled = enabled,
    disabledReason = disabled_reason,
)

private fun FlightPlanControlUiView.toWire() = WireFlightPlanControlUiView(
    id = id.toWire(),
    label = label,
    enabled = enabled,
    disabled_reason = disabledReason,
)

private fun WireGuidanceUiView.toUi() = GuidanceUiView(
    sequencingMode = sequencing_mode.toUi(),
    activeFromRowUid = active_from_row_uid,
    activeToRowUid = active_to_row_uid,
    activeLeg = active_leg?.toUiPlanLeg(),
    navElement = nav_element.toUi(),
    directTo = direct_to?.toUi(),
    suspendBoundaryAfterActiveLeg = suspend_boundary_after_active_leg,
)

private fun GuidanceUiView.toWire() = WireGuidanceUiView(
    sequencing_mode = sequencingMode.toWire(),
    active_from_row_uid = activeFromRowUid,
    active_to_row_uid = activeToRowUid,
    active_leg = activeLeg?.toWire(),
    nav_element = navElement.toWire(),
    direct_to = directTo?.toWire(),
    suspend_boundary_after_active_leg = suspendBoundaryAfterActiveLeg,
)

private fun WireNavElementUiView.toUi() = NavElementUiView(
    activeLegSummary = active_leg_summary,
    cdiIndicatorDots = cdi_indicator_dots,
    cdiOffscaleReadout = cdi_offscale_readout,
)

private fun NavElementUiView.toWire() = WireNavElementUiView(
    active_leg_summary = activeLegSummary,
    cdi_indicator_dots = cdiIndicatorDots,
    cdi_offscale_readout = cdiOffscaleReadout,
)

private fun WireSequencingMode.toUi() = when (this) {
    WireSequencingMode.FollowPlan -> SequencingMode.FollowPlan
    WireSequencingMode.Suspended -> SequencingMode.Suspended
    WireSequencingMode.DirectTo -> SequencingMode.DirectTo
}

private fun SequencingMode.toWire() = when (this) {
    SequencingMode.FollowPlan -> WireSequencingMode.FollowPlan
    SequencingMode.Suspended -> WireSequencingMode.Suspended
    SequencingMode.DirectTo -> WireSequencingMode.DirectTo
}

private fun MapLayerId.toWire() = when (this) {
    MapLayerId.WorldBasemap -> "world_basemap"
    MapLayerId.Vectors -> "vectors"
    MapLayerId.Metars -> "metars"
    MapLayerId.Nexrad -> "nexrad"
    MapLayerId.TerrainWarning -> "terrain_warning"
    MapLayerId.OfflineRegions -> "offline_regions"
}

private fun WireFlightPlanRouteSegment.toUi() = FlightPlanRouteSegment(
    id = id,
    legId = legId,
    from = from.toUi(),
    to = to.toUi(),
    path = path.map { it.toUi() },
    style = style,
    distanceNm = distance_nm,
    courseDeg = course_deg,
    status = status.toUi(),
)

private fun WireFlightPlanRouteProjection.toUi() = FlightPlanRouteProjection(
    flightPlanRouteRevision = flight_plan_route_revision,
    segments = segments.map { it.toUi() },
)

private fun WireRouteSegmentStatus.toUi() = when (this) {
    WireRouteSegmentStatus.Completed -> RouteSegmentStatus.Completed
    WireRouteSegmentStatus.Active -> RouteSegmentStatus.Active
    WireRouteSegmentStatus.ActiveLegRemaining -> RouteSegmentStatus.ActiveLegRemaining
    WireRouteSegmentStatus.Remaining -> RouteSegmentStatus.Remaining
}

private fun WireFlightPlanUiState.toUi() = FlightPlanUiState(
    planId = plan_id,
    planVersion = plan_version,
    displayRows = display_rows.map { it.toUi() },
    dataColumns = data_columns.map { it.toUi() },
    controls = controls.map { it.toUi() },
    guidance = guidance?.toUi(),
)

private fun FlightPlanUiState.toWire() = WireFlightPlanUiState(
    plan_id = planId,
    plan_version = planVersion,
    display_rows = displayRows.map { it.toWire() },
    data_columns = dataColumns.map { it.toWire() },
    controls = controls.map { it.toWire() },
    guidance = guidance?.toWire(),
)

private fun WireFlightDataCell.toUi() = FlightDataCell(
    id = id,
    label = label,
    value = value,
    tone = tone,
)

private fun FlightDataCell.toWire() = WireFlightDataCell(
    id = id,
    label = label,
    value = value,
    tone = tone,
)

private fun WireFlightDataColumn.toUi() = FlightDataColumn(
    id = id,
    label = label,
)

private fun WireFlightDataBannerModel.toUi() = FlightDataBannerModel(
    cells = cells.map { it.toUi() },
)

private fun FlightDataColumn.toWire() = WireFlightDataColumn(
    id = id,
    label = label,
)

private fun WireFlightPlanDisplayRowUiView.toUi() = FlightPlanDisplayRowUiView(
    uid = uid,
    label = label,
    rowKind = row_kind.toUi(),
    componentKind = component_kind?.toUi(),
    componentUid = component_uid,
    procedureId = procedure_id,
    procedureKind = procedure_kind?.toUi(),
    dataCells = data_cells.map { it.toUi() },
    showPlateTargetId = show_plate_target_id,
    chartAirportId = chart_airport_id,
    navRef = nav_ref?.toUi(),
    symbolFeature = symbol_feature?.toUi(),
    depth = depth,
    active = active,
    enabled = enabled,
    disabledReason = disabled_reason,
    syntheticDirectTo = synthetic_direct_to,
    canAddAirwayAfter = can_add_airway_after,
    canAddProcedureBefore = can_add_procedure_before,
    canRemoveComponent = can_remove_component,
    canReorderComponent = can_reorder_component,
    canReorderUp = can_reorder_up,
    canReorderDown = can_reorder_down,
    originAnchor = origin_anchor?.toUi(),
    destinationAnchor = destination_anchor?.toUi(),
    precedingWaypoint = preceding_waypoint?.toUi(),
    followingWaypoint = following_waypoint?.toUi(),
    actionMatrix = action_matrix.map { row -> row.map { it.toUi() } },
)

private fun FlightPlanDisplayRowUiView.toWire() = WireFlightPlanDisplayRowUiView(
    uid = uid,
    label = label,
    row_kind = rowKind.toWire(),
    component_kind = componentKind?.toWire(),
    component_uid = componentUid,
    procedure_id = procedureId,
    procedure_kind = procedureKind?.toWire(),
    data_cells = dataCells.map { it.toWire() },
    show_plate_target_id = showPlateTargetId,
    chart_airport_id = chartAirportId,
    nav_ref = navRef?.toWire(),
    symbol_feature = symbolFeature?.toWire(),
    depth = depth,
    active = active,
    enabled = enabled,
    disabled_reason = disabledReason,
    synthetic_direct_to = syntheticDirectTo,
    can_add_airway_after = canAddAirwayAfter,
    can_add_procedure_before = canAddProcedureBefore,
    can_remove_component = canRemoveComponent,
    can_reorder_component = canReorderComponent,
    can_reorder_up = canReorderUp,
    can_reorder_down = canReorderDown,
    origin_anchor = originAnchor?.toWire(),
    destination_anchor = destinationAnchor?.toWire(),
    preceding_waypoint = precedingWaypoint?.toWire(),
    following_waypoint = followingWaypoint?.toWire(),
    action_matrix = actionMatrix.map { row -> row.map { it.toWire() } },
)

private fun WireFlightPlanDisplayRowKind.toUi() = when (this) {
    WireFlightPlanDisplayRowKind.Waypoint -> FlightPlanDisplayRowKind.Waypoint
    WireFlightPlanDisplayRowKind.Group -> FlightPlanDisplayRowKind.Group
    WireFlightPlanDisplayRowKind.Discontinuity -> FlightPlanDisplayRowKind.Discontinuity
    WireFlightPlanDisplayRowKind.Summary -> FlightPlanDisplayRowKind.Summary
}

private fun FlightPlanDisplayRowKind.toWire() = when (this) {
    FlightPlanDisplayRowKind.Waypoint -> WireFlightPlanDisplayRowKind.Waypoint
    FlightPlanDisplayRowKind.Group -> WireFlightPlanDisplayRowKind.Group
    FlightPlanDisplayRowKind.Discontinuity -> WireFlightPlanDisplayRowKind.Discontinuity
    FlightPlanDisplayRowKind.Summary -> WireFlightPlanDisplayRowKind.Summary
}

private fun WireFlightPlanRowActionUiView.toUi() = FlightPlanRowActionUiView(
    id = id,
    uid = uid,
    label = label,
    enabled = enabled,
    disabledReason = disabled_reason,
    execution = execution,
    dismissTrayOnSuccess = dismiss_tray_on_success,
    navigation = navigation?.toUi(),
    weatherDetail = weather_detail?.toUi(),
    airportInfoAirportId = airport_info_airport_id,
)

private fun FlightPlanRowActionUiView.toWire() = WireFlightPlanRowActionUiView(
    id = id,
    uid = uid,
    label = label,
    enabled = enabled,
    disabled_reason = disabledReason,
    execution = execution,
    dismiss_tray_on_success = dismissTrayOnSuccess,
    navigation = navigation?.toWire(),
    weather_detail = weatherDetail?.toWire(),
    airport_info_airport_id = airportInfoAirportId,
)

private fun WireAirportInfoUiView.toUi() = AirportInfoUiView(
    airportId = airport_id,
    name = name,
    elevationLabel = elevation_label,
    trafficPatternAltitudeLabel = traffic_pattern_altitude_label,
    trafficPatternAltitudeSource = traffic_pattern_altitude_source,
    localTimeLabel = local_time_label,
    utcTimeLabel = utc_time_label,
    timeZoneLabel = time_zone_label,
    sunrise = sunrise?.toUi(),
    sunset = sunset?.toUi(),
    communications = communications.map { it.toUi() },
    runwayDiagramComplex = runway_diagram_complex,
    runways = runways.map { it.toUi() },
)

private fun WireAirportSolarEventUiView.toUi() = AirportSolarEventUiView(
    localTimeLabel = local_time_label,
    utcTimeLabel = utc_time_label,
    nextInLabel = next_in_label,
)

private fun WireAirportCommunicationUiView.toUi() = AirportCommunicationUiView(
    label = label,
    value = value,
    kind = kind,
)

private fun WireAirportRunwayUiView.toUi() = AirportRunwayUiView(
    endALabel = end_a_label,
    endBLabel = end_b_label,
    dimensionsLabel = dimensions_label,
    surfaceLabel = surface_label,
    surfaceColorKey = surface_color_key,
    diagramEndAX = diagram_end_a_x,
    diagramEndAY = diagram_end_a_y,
    diagramEndBX = diagram_end_b_x,
    diagramEndBY = diagram_end_b_y,
    diagramWidthRatio = diagram_width_ratio,
)

private fun WireFlightPlanRowNavigationAction.toUi(): FlightPlanRowNavigationAction? =
    when (kind) {
        "open_airport_charts" -> {
            val airportId = airport_id ?: return null
            FlightPlanRowNavigationAction.OpenAirportCharts(airportId)
        }
        "open_plate_target" -> {
            val airportId = airport_id ?: return null
            val target = target ?: return null
            FlightPlanRowNavigationAction.OpenPlateTarget(airportId, target)
        }
        else -> null
    }

private fun FlightPlanRowNavigationAction.toWire(): WireFlightPlanRowNavigationAction =
    when (this) {
        is FlightPlanRowNavigationAction.OpenAirportCharts -> WireFlightPlanRowNavigationAction(
            kind = "open_airport_charts",
            airport_id = airportId,
        )
        is FlightPlanRowNavigationAction.OpenPlateTarget -> WireFlightPlanRowNavigationAction(
            kind = "open_plate_target",
            airport_id = airportId,
            target = target,
        )
    }

private fun WireFlightPlanEntryPreview.toUi() = FlightPlanEntryPreview(
    canCommit = can_commit,
    tokens = tokens.map { it.toUi() },
    issues = issues.map { it.toUi() },
)

private fun WireFlightPlanEntryToken.toUi() = FlightPlanEntryToken(
    start = start,
    end = end,
    state = state,
)

private fun WireFlightPlanEntryIssue.toUi() = FlightPlanEntryIssue(
    start = start,
    end = end,
    message = message,
)

private fun WireNavRef.toUi(): NavRef = when (this) {
    is WireNavRef.Airport -> NavRef.Airport(code)
    is WireNavRef.Navaid -> NavRef.Navaid(code)
    is WireNavRef.ArincNavaid -> NavRef.ArincNavaid(
        identifier = identifier,
        icaoCode = icao_code,
        sectionCode = section_code,
        subsectionCode = subsection_code,
    )
    is WireNavRef.TerminalNavaid -> NavRef.TerminalNavaid(
        airportId = airport_id,
        identifier = identifier,
        icaoCode = icao_code,
        sectionCode = section_code,
        subsectionCode = subsection_code,
    )
    is WireNavRef.Fix -> NavRef.Fix(code)
    is WireNavRef.LatLon -> NavRef.LatLon(value.lat, value.lon)
    is WireNavRef.Spot -> NavRef.Spot(value.lat, value.lon)
}
