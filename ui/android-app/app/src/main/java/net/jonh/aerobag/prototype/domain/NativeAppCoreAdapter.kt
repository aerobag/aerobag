package net.jonh.aerobag.prototype.domain

import android.util.Log
import kotlinx.serialization.builtins.ListSerializer
import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.decodeFromJsonElement
import kotlinx.serialization.json.encodeToJsonElement
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.put
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive

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
    val styleClass: String,
    val obstacleVariant: String?,
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

data class AirspaceDisplayDecoration(
    val colorKey: String,
    val widthPx: Double,
    val lineCap: String,
    val paths: List<AirspaceDisplaySubpath>,
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
    val styleClass: String,
    val obstacleVariant: String?,
    val towered: Boolean,
    val fuelAvailable: Boolean,
    val hasPavedRunway: Boolean?,
    val heliport: Boolean?,
    val hasWaterRunway: Boolean?,
    val runwayLengthRatio: Double,
    val longestRunwayHeadingTrueDeg: Double?,
)

data class MapOverlayWarning(
    val code: String,
    val message: String,
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
    val selectedFamilyId: MapChartFamily,
    val selectedFamilyLabel: String,
    val selectedFamilyLauncherLabel: String,
    val minZoom: Double,
    val maxZoom: Double,
    val initialViewport: MapViewportSeed,
    val familyOptions: List<MapFamilyOption>,
)

data class MapFamilyOption(
    val id: MapChartFamily,
    val label: String,
    val launcherLabel: String,
    val enabled: Boolean,
    val active: Boolean,
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
    val warnings: List<MapOverlayWarning>,
)

data class MapSelectionQueryResult(
    val clickLat: Double,
    val clickLon: Double,
    val categories: List<MapSelectionCategory>,
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
    data class Spot(val lat: Double, val lon: Double) : MapSelectionHighlight
}

data class MapSelectionAction(
    val id: String,
    val label: String,
    val enabled: Boolean,
    val displayOnly: Boolean,
    val airspaceLimit: AirspaceLimitGlyph?,
    val sessionAction: String?,
    val flightPlanRowAction: MapSelectionFlightPlanRowAction?,
)

data class MapSelectionFlightPlanRowAction(
    val rowUid: String,
    val actionUid: String,
)

sealed interface TerrainOverlayStatus {
    data object Hidden : TerrainOverlayStatus
    data object NoPosition : TerrainOverlayStatus
    data object NoAltitude : TerrainOverlayStatus
    data class TooManyTiles(val count: Int) : TerrainOverlayStatus
    data class Ready(val count: Int) : TerrainOverlayStatus
}

data class TerrainOverlayTileRequest(
    val key: String,
    val z: Int,
    val x: Int,
    val yTms: Int,
    val left: Double,
    val top: Double,
    val size: Double,
)

data class TerrainOverlayQueryResult(
    val status: TerrainOverlayStatus,
    val tileRequests: List<TerrainOverlayTileRequest>,
)

class NativeAppCoreAdapter(
    private val vectorManifestJson: String,
    private val navKvStore: NavKvStore? = null,
    private val bridge: NativeBridge = NativeBindings,
    private val json: Json = Json {
        encodeDefaults = true
        ignoreUnknownKeys = true
    },
) : AppCoreAdapter {
    fun situationRingCandidates(): List<SituationRingCandidate> =
        json.decodeFromString<List<WireSituationRingCandidate>>(bridge.situationRingCandidatesJson())
            .map { it.toUi() }

    fun emptyFlightPlan(): FlightPlan =
        json.decodeFromString<WireFlightPlan>(bridge.emptyFlightPlanJson()).toUiFlightPlan()

    fun createUiSession(
        plan: FlightPlan,
        recentAirportIds: List<String>,
        selectedAirportId: String?,
        selectedChartId: String?,
    ): NativeUiSession {
        val resultJson = bridge.createUiSessionJson(
            vectorManifestJson,
            json.encodeToString(plan.toWire()),
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
            initialSnapshot = result.snapshot.toUi(),
        )
        navKvStore?.attachToSession(result.handle)
        session.loadRasterMapCatalog()
        return session.apply {
            syncGuidanceGeometry()
        }
    }

    fun deriveChartPageState(
        plan: FlightPlan,
        recentAirportIds: List<String>,
        selectedAirportId: String?,
        selectedChartId: String?,
    ): DerivedChartPageState {
        val result = runHadOperationElement(
            buildJsonObject {
                put("kind", "chart_page_state")
                put("plan", json.encodeToJsonElement(plan.toWire()))
                put("recent_airport_ids", json.encodeToJsonElement(recentAirportIds))
                put("selected_airport_id", json.encodeToJsonElement(selectedAirportId))
                put("selected_chart_id", json.encodeToJsonElement(selectedChartId))
            },
        )
        return json.decodeFromJsonElement<WireDerivedChartPageState>(result).toUi()
    }

    fun removeFlightPlanLeg(plan: FlightPlan, index: Int): FlightPlan {
        val planJson = json.encodeToString(plan.toWire())
        val nextJson = bridge.removeFlightPlanLegJson(planJson, index)
        return json.decodeFromString<WireFlightPlan>(nextJson).toUiFlightPlan()
    }

    override fun replaceFlightPlan(state: AppState, plan: FlightPlan): AppState {
        val stateJson = json.encodeToString(state.toWire())
        val planJson = json.encodeToString(plan.toWire())
        val nextJson = bridge.replaceFlightPlanStateJson(stateJson, planJson)
        return json.decodeFromString<WireAppState>(nextJson).toUi()
    }

    override fun setContentPolicy(state: AppState, policy: ContentPolicy): AppState {
        val stateJson = json.encodeToString(state.toWire())
        val policyJson = json.encodeToString(policy.toWire())
        val nextJson = bridge.setContentPolicyStateJson(stateJson, policyJson)
        return json.decodeFromString<WireAppState>(nextJson).toUi()
    }

    override fun refreshContent(state: AppState, inventory: ContentInventory): AppState {
        val stateJson = json.encodeToString(state.toWire())
        val inventoryJson = json.encodeToString(inventory.toWire())
        val nextJson = bridge.refreshContentStateJson(stateJson, inventoryJson)
        return json.decodeFromString<WireAppState>(nextJson).toUi()
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

    fun suggestWaypointIdentifiers(
        plan: FlightPlan,
        componentIndex: Int,
        before: Boolean,
        prefix: String,
        limit: Int = 8,
    ): List<WaypointIdentifierSuggestion> {
        val result = runHadOperationElement(
            buildJsonObject {
                put("kind", "suggest_waypoint_identifiers")
                put("plan", json.encodeToJsonElement(plan.toWire()))
                put("component_index", componentIndex)
                put("before", before)
                put("prefix", prefix)
                put("limit", limit)
            },
        )
        return json.decodeFromJsonElement<List<WireWaypointIdentifierSuggestion>>(result).map { it.toUi() }
    }

    fun suggestWaypointIdentifiersNear(
        anchor: LatLonPoint,
        prefix: String,
        limit: Int = 8,
    ): List<WaypointIdentifierSuggestion> {
        val result = runHadOperationElement(
            buildJsonObject {
                put("kind", "suggest_waypoint_identifiers_near")
                put("anchor", json.encodeToJsonElement(WireLatLon.serializer(), anchor.toWire()))
                put("prefix", prefix)
                put("limit", limit)
            },
        )
        return json.decodeFromJsonElement<List<WireWaypointIdentifierSuggestion>>(result).map { it.toUi() }
    }

    fun previewFlightPlanEntry(plan: FlightPlan, input: String): FlightPlanEntryPreview {
        val result = runHadOperationElement(
            buildJsonObject {
                put("kind", "preview_flight_plan_entry")
                put("plan", json.encodeToJsonElement(plan.toWire()))
                put("input", input)
            },
        )
        return json.decodeFromJsonElement<WireFlightPlanEntryPreview>(result).toUi()
    }

    fun appendFlightPlanEntry(plan: FlightPlan, input: String): FlightPlanUiMutation {
        val result = runHadOperationElement(
            buildJsonObject {
                put("kind", "append_flight_plan_entry")
                put("plan", json.encodeToJsonElement(plan.toWire()))
                put("input", input)
            },
        )
        return json.decodeFromJsonElement<WireFlightPlanUiMutation>(result).toUi()
    }

    fun prepareAirwayPresentationForAnchors(
        airwayName: String,
        originAnchor: NavRef,
        destinationAnchor: NavRef?,
    ): AirwayPresentationPlan {
        val result = runHadOperationElement(
            buildJsonObject {
                put("kind", "prepare_airway_presentation_for_anchors")
                put("airway_name", airwayName)
                put("origin_anchor", json.encodeToJsonElement(originAnchor.toWire()))
                put("destination_anchor", json.encodeToJsonElement(destinationAnchor?.toWire()))
            },
        )
        return json.decodeFromJsonElement<WireAirwayPresentationPlan>(result).toUi()
    }

    fun materializeAirwayPresentationSelection(
        startComponentIndex: Int,
        presentation: AirwayPresentationPlan,
        entryIndex: Int,
        exitIndex: Int,
        originAnchor: NavRef,
        destinationAnchor: NavRef?,
    ): MaterializedAirway {
        val result = runHadOperationElement(
            buildJsonObject {
                put("kind", "materialize_airway_presentation_selection")
                put("start_component_index", startComponentIndex)
                put("presentation", json.encodeToJsonElement(presentation.toWire()))
                put("entry_index", entryIndex)
                put("exit_index", exitIndex)
                put("origin_anchor", json.encodeToJsonElement(originAnchor.toWire()))
                put("destination_anchor", json.encodeToJsonElement(destinationAnchor?.toWire()))
            },
        )
        return json.decodeFromJsonElement<WireMaterializedAirway>(result).toUi()
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

    fun describePlateProcedureLoads(plan: FlightPlan, plateId: String): List<ProcedureLoadOption> {
        val result = runHadOperationElement(
            buildJsonObject {
                put("kind", "describe_plate_procedure_loads")
                put("plan", json.encodeToJsonElement(plan.toWire()))
                put("plate_id", plateId)
            },
        )
        return json.decodeFromJsonElement<List<WireProcedureLoadOption>>(result).map { it.toUi() }
    }

    fun materializeProcedureSelection(
        airportId: String,
        procedureId: String,
        kind: ProcedureKind,
        runwayTransition: String?,
        enrouteTransition: String?,
        componentIndex: Int,
    ): MaterializedProcedure {
        val result = runHadOperationElement(
            buildJsonObject {
                put("kind", "materialize_procedure")
                put("airport_id", airportId)
                put("procedure_id", procedureId)
                put("procedure_kind", json.encodeToJsonElement(kind.toWire()))
                put("runway_transition", json.encodeToJsonElement(runwayTransition))
                put("enroute_transition", json.encodeToJsonElement(enrouteTransition))
                put("component_index", componentIndex)
            },
        )
        return runCatching {
            json.decodeFromJsonElement<WireMaterializedProcedure>(result).toUi()
        }.getOrElse { error ->
            Log.e(
                "AerobagProcedure",
                "materializeProcedureSelection decode failed airport=$airportId procedure=$procedureId runway=$runwayTransition enroute=$enrouteTransition json=$result",
                error,
            )
            throw error
        }
    }

    private fun runHadOperationElement(operation: kotlinx.serialization.json.JsonObject): JsonElement =
        navKvStore?.runCoreOperationElement(operation)
            ?: error("nav_kv store is required for core data operation ${operation["kind"]}")

    fun activateLegUi(plan: FlightPlan, legIndex: Int): FlightPlanUiMutation {
        val nextJson = bridge.activateLegUiJson(json.encodeToString(plan.toWire()), legIndex)
        return json.decodeFromString<WireFlightPlanUiMutation>(nextJson).toUi()
    }

    fun prepareAirwayPresentation(
        airwayName: String,
        branches: List<AirwayBranch>,
        originPosition: LatLonPoint,
        destinationPosition: LatLonPoint?,
    ): AirwayPresentationPlan {
        val nextJson =
            bridge.prepareAirwayPresentationJson(
                airwayName,
                json.encodeToString(branches.map { it.toWire() }),
                json.encodeToString(originPosition.toWire()),
                json.encodeToString(destinationPosition?.toWire()),
            )
        return json.decodeFromString<WireAirwayPresentationPlan>(nextJson).toUi()
    }

    fun sortAirwaySuggestionsForUi(suggestions: List<AirwaySuggestion>): List<AirwaySuggestion> {
        val nextJson = bridge.sortAirwaySuggestionsForUiJson(json.encodeToString(suggestions.map { it.toWire() }))
        return json.decodeFromString<List<WireAirwaySuggestion>>(nextJson).map { it.toUi() }
    }

    fun insertAirwayMaterializedUi(
        plan: FlightPlan,
        startComponentIndex: Int,
        endComponentIndex: Int?,
        selection: AirwayAutoSelection,
        airway: AirwaySegment,
        resolvedLegs: List<ResolvedLeg>,
    ): FlightPlanUiMutation {
        val nextJson =
            bridge.insertAirwayMaterializedUiJson(
                json.encodeToString(plan.toWire()),
                startComponentIndex,
                json.encodeToString(endComponentIndex),
                json.encodeToString(selection.toWire()),
                json.encodeToString(airway.toWire()),
                json.encodeToString(resolvedLegs.map { it.toWire() }),
            )
        return json.decodeFromString<WireAirwayPlanUiMutation>(nextJson).toUi()
    }

    fun replaceAirwayMaterializedUi(
        plan: FlightPlan,
        componentIndex: Int,
        selection: AirwayAutoSelection,
        airway: AirwaySegment,
        resolvedLegs: List<ResolvedLeg>,
    ): FlightPlanUiMutation {
        val nextJson =
            bridge.replaceAirwayMaterializedUiJson(
                json.encodeToString(plan.toWire()),
                componentIndex,
                json.encodeToString(selection.toWire()),
                json.encodeToString(airway.toWire()),
                json.encodeToString(resolvedLegs.map { it.toWire() }),
            )
        return json.decodeFromString<WireAirwayPlanUiMutation>(nextJson).toUi()
    }

    fun insertProcedureMaterializedUi(
        plan: FlightPlan,
        startComponentIndex: Int,
        endComponentIndex: Int,
        built: MaterializedProcedure,
    ): FlightPlanUiMutation {
        val nextJson =
            bridge.insertProcedureMaterializedUiJson(
                json.encodeToString(plan.toWire()),
                startComponentIndex,
                endComponentIndex,
                json.encodeToString(built.toWire()),
            )
        val mutation = runCatching {
            json.decodeFromString<WireProcedurePlanUiMutation>(nextJson)
        }.getOrElse { error ->
            Log.e("AerobagProcedure", "insertProcedureMaterializedUi decode failed json=$nextJson", error)
            throw error
        }
        return FlightPlanUiMutation(
            plan = mutation.mutation.plan.toUiForTesting(),
            uiState = mutation.ui_state.toUi(),
        )
    }

    fun replaceProcedureMaterializedUi(
        plan: FlightPlan,
        componentIndex: Int,
        built: MaterializedProcedure,
    ): FlightPlanUiMutation {
        val nextJson =
            bridge.replaceProcedureMaterializedUiJson(
                json.encodeToString(plan.toWire()),
                componentIndex,
                json.encodeToString(built.toWire()),
            )
        val mutation = runCatching {
            json.decodeFromString<WireProcedurePlanUiMutation>(nextJson)
        }.getOrElse { error ->
            Log.e("AerobagProcedure", "replaceProcedureMaterializedUi decode failed json=$nextJson", error)
            throw error
        }
        return FlightPlanUiMutation(
            plan = mutation.mutation.plan.toUiForTesting(),
            uiState = mutation.ui_state.toUi(),
        )
    }
}

class NativeUiSession internal constructor(
    private val handle: Long,
    private val bridge: NativeBridge,
    private val json: Json,
    private val navKvStore: NavKvStore?,
    initialSnapshot: UiSessionSnapshot,
) {
    var snapshot: UiSessionSnapshot = initialSnapshot
        private set

    fun syncGuidanceGeometry(): UiSessionSnapshot {
        val store = navKvStore ?: return snapshot
        snapshot = json.decodeFromJsonElement<WireUiSessionSnapshot>(
            store.runPagedSessionOperationElement {
                bridge.syncGuidanceGeometryInSessionJson(handle)
            },
        ).toUi()
        return snapshot
    }

    fun projectFlightPlanRoute(): List<FlightPlanRouteSegment> {
        val store = navKvStore ?: return emptyList()
        return json.decodeFromJsonElement<List<WireFlightPlanRouteSegment>>(
            store.runPagedSessionOperationElement {
                bridge.projectFlightPlanRouteInSessionJson(handle)
            },
        ).map { it.toUi() }
    }

    fun performMapSelectionAction(action: String): UiSessionSnapshot {
        val store = navKvStore ?: error("nav_kv store is required to perform map selection action")
        val result =
            store.runPagedSessionOperationElement {
                bridge.performMapSelectionActionInSessionJson(handle, action)
            }
        snapshot = json.decodeFromJsonElement<WireUiSessionSnapshot>(result).toUi()
        return syncGuidanceGeometry()
    }

    fun describePlateProcedureLoads(plan: FlightPlan, plateId: String): List<ProcedureLoadOption> {
        val store = navKvStore ?: error("nav_kv store is required to describe plate procedure loads")
        return store.runCoreOperation(
            buildJsonObject {
                put("kind", "describe_plate_procedure_loads")
                put("plan", json.encodeToJsonElement(plan.toWire()))
                put("plate_id", plateId)
            },
            ListSerializer(WireProcedureLoadOption.serializer()),
        ).map { it.toUi() }
    }

    fun loadPlateProcedure(loadId: String): UiSessionSnapshot {
        val store = navKvStore ?: error("nav_kv store is required to load a plate procedure")
        val result =
            store.runPagedSessionOperationElement {
                bridge.loadPlateProcedureInSessionJson(handle, loadId)
            }
        snapshot = json.decodeFromJsonElement<WireUiSessionSnapshot>(result).toUi()
        return syncGuidanceGeometry()
    }

    fun insertWaypointAtFlightPlanRow(rowUid: String, before: Boolean, waypoint: NavRef): UiSessionSnapshot {
        val store = navKvStore ?: error("nav_kv store is required to insert waypoint at flight plan row")
        snapshot = json.decodeFromJsonElement<WireUiSessionSnapshot>(
            store.runPagedSessionOperationElement {
                bridge.insertWaypointAtFlightPlanRowInSessionJson(
                    handle,
                    rowUid,
                    before,
                    json.encodeToString(waypoint.toWire()),
                )
            },
        ).toUi()
        return syncGuidanceGeometry()
    }

    fun suggestWaypointIdentifiersAtFlightPlanRow(
        rowUid: String,
        before: Boolean,
        prefix: String,
        limit: Int = 8,
    ): List<WaypointIdentifierSuggestion> {
        val store = navKvStore ?: error("nav_kv store is required to suggest waypoints")
        val result =
            store.runPagedSessionOperationElement {
                bridge.suggestWaypointIdentifiersAtFlightPlanRowInSessionJson(
                    handle,
                    rowUid,
                    before,
                    prefix,
                    limit,
                )
            }
        return json.decodeFromJsonElement<List<WireWaypointIdentifierSuggestion>>(result).map { it.toUi() }
    }

    fun insertAirwayAtFlightPlanRow(
        rowUid: String,
        presentation: AirwayPresentationPlan,
        entryIndex: Int,
        exitIndex: Int,
    ): UiSessionSnapshot {
        val store = navKvStore ?: error("nav_kv store is required to insert airway")
        val result =
            store.runPagedSessionOperationElement {
                bridge.insertAirwayAtFlightPlanRowInSessionJson(
                    handle,
                    rowUid,
                    json.encodeToString(presentation.toWire()),
                    entryIndex,
                    exitIndex,
                )
            }
        snapshot = json.decodeFromJsonElement<WireUiSessionSnapshot>(result).toUi()
        return syncGuidanceGeometry()
    }

    fun selectProcedureAtFlightPlanRow(
        rowUid: String,
        airportId: String,
        procedureId: String,
        kind: ProcedureKind,
        runwayTransition: String?,
        enrouteTransition: String?,
    ): UiSessionSnapshot {
        val store = navKvStore ?: error("nav_kv store is required to select procedure")
        val result =
            store.runPagedSessionOperationElement {
                bridge.selectProcedureAtFlightPlanRowInSessionJson(
                    handle,
                    rowUid,
                    airportId,
                    procedureId,
                    json.encodeToString(kind.toWire()),
                    json.encodeToString(runwayTransition),
                    json.encodeToString(enrouteTransition),
                )
            }
        snapshot = json.decodeFromJsonElement<WireUiSessionSnapshot>(result).toUi()
        return syncGuidanceGeometry()
    }

    fun registerOwnshipSource(registration: OwnshipSourceRegistration): UiSessionSnapshot {
        snapshot = decodeSnapshot(bridge.registerOwnshipSourceInSessionJson(handle, registration.toCoreJson(json)))
        return snapshot
    }

    fun updateOwnshipSourceStatus(update: OwnshipSourceStatusUpdate): UiSessionSnapshot {
        snapshot = decodeSnapshot(bridge.updateOwnshipSourceStatusInSessionJson(handle, update.toCoreJson(json)))
        return snapshot
    }

    fun pushSituationSample(sample: SituationSample): UiSessionSnapshot {
        snapshot = decodeSnapshot(bridge.pushSituationSampleInSessionJson(handle, sample.toCoreJson(json)))
        return snapshot
    }

    fun selectOwnshipSource(selection: OwnshipSelection): UiSessionSnapshot {
        snapshot = decodeSnapshot(bridge.selectOwnshipSourceInSessionJson(handle, selection.toCoreJson(json)))
        return snapshot
    }

    fun applySituationControlInput(input: SituationControlInput, nowEpochMs: Double): UiSessionSnapshot {
        snapshot =
            decodeSnapshot(
                bridge.applySituationControlInputInSessionJson(
                    handle,
                    input.toCoreJson(json),
                    nowEpochMs,
                ),
            )
        return snapshot
    }

    fun engageMapFollow(viewport: MapViewportState): UiSessionSnapshot {
        snapshot = decodeSnapshot(bridge.engageMapFollowInSessionJson(handle, viewport.toCoreViewport().toCoreJson(json)))
        return snapshot
    }

    fun disengageMapFollow(viewport: MapViewportState): UiSessionSnapshot {
        snapshot = decodeSnapshot(bridge.disengageMapFollowInSessionJson(handle, viewport.toCoreViewport().toCoreJson(json)))
        return snapshot
    }

    fun setMapFollowOffset(viewport: MapViewportState, offsetXPx: Double, offsetYPx: Double): UiSessionSnapshot {
        snapshot =
            decodeSnapshot(
                bridge.setMapFollowOffsetInSessionJson(
                    handle,
                    viewport.toCoreViewport().toCoreJson(json),
                    offsetXPx,
                    offsetYPx,
                ),
            )
        return snapshot
    }

    fun loadPlaybackTrace(sourcePath: String, traceJson: String): UiSessionSnapshot {
        snapshot =
            decodeSnapshot(
                bridge.loadPlaybackTraceInSessionJson(
                    handle,
                    json.encodeToString(sourcePath),
                    traceJson,
                ),
            )
        return snapshot
    }

    fun playPlayback(nowEpochMs: Double): UiSessionSnapshot {
        snapshot = decodeSnapshot(bridge.playPlaybackInSessionJson(handle, nowEpochMs))
        return snapshot
    }

    fun pausePlayback(nowEpochMs: Double): UiSessionSnapshot {
        snapshot = decodeSnapshot(bridge.pausePlaybackInSessionJson(handle, nowEpochMs))
        return snapshot
    }

    fun seekPlayback(cursorSeconds: Double, nowEpochMs: Double): UiSessionSnapshot {
        snapshot = decodeSnapshot(bridge.seekPlaybackInSessionJson(handle, cursorSeconds, nowEpochMs))
        return snapshot
    }

    fun setPlaybackRate(rate: Double, nowEpochMs: Double): UiSessionSnapshot {
        snapshot = decodeSnapshot(bridge.setPlaybackRateInSessionJson(handle, rate, nowEpochMs))
        return snapshot
    }

    fun tickPlayback(nowEpochMs: Double): UiSessionSnapshot {
        snapshot = decodeSnapshot(bridge.tickPlaybackInSessionJson(handle, nowEpochMs))
        return snapshot
    }

    fun selectAirport(airportId: String): UiSessionSnapshot {
        snapshot = decodeSnapshot(bridge.selectAirportInSessionJson(handle, json.encodeToString(airportId)))
        return snapshot
    }

    fun selectChart(chartId: String): UiSessionSnapshot {
        snapshot = decodeSnapshot(bridge.selectChartInSessionJson(handle, json.encodeToString(chartId)))
        return snapshot
    }

    fun setMapLayerVisibility(layerId: MapLayerId, visible: Boolean): UiSessionSnapshot {
        snapshot = decodeSnapshot(bridge.setMapLayerVisibilityInSessionJson(handle, json.encodeToString(layerId.toWire()), visible))
        return snapshot
    }

    fun setMapLayerEnabled(layerId: MapLayerId, enabled: Boolean): UiSessionSnapshot {
        snapshot = decodeSnapshot(bridge.setMapLayerEnabledInSessionJson(handle, json.encodeToString(layerId.toWire()), enabled))
        return snapshot
    }

    fun setDebugFlag(flagId: String, enabled: Boolean): UiSessionSnapshot {
        snapshot = decodeSnapshot(bridge.setDebugFlagInSessionJson(handle, json.encodeToString(flagId), enabled))
        return snapshot
    }

    fun setRasterResourceMode(mode: String): UiSessionSnapshot {
        snapshot = decodeSnapshot(bridge.setRasterResourceModeInSessionJson(handle, json.encodeToString(mode)))
        return snapshot
    }

    fun loadRasterMapCatalog(): UiSessionSnapshot {
        val store = navKvStore ?: return snapshot
        snapshot = json.decodeFromJsonElement<WireUiSessionSnapshot>(
            store.runPagedSessionOperationElement {
                bridge.loadRasterMapCatalogInSessionJson(handle)
            },
        ).toUi()
        return snapshot
    }

    fun selectMapFamily(familyId: MapChartFamily): UiSessionSnapshot {
        val store = navKvStore ?: return snapshot
        snapshot = json.decodeFromJsonElement<WireUiSessionSnapshot>(
            store.runPagedSessionOperationElement {
                bridge.selectMapFamilyInSessionJson(handle, json.encodeToString(familyId.toWireName()))
            },
        ).toUi()
        return snapshot
    }

    fun selectRasterMap(selectedMapId: String): UiSessionSnapshot {
        snapshot = decodeSnapshot(
            bridge.selectRasterMapInSessionJson(handle, json.encodeToString(selectedMapId)),
        )
        return snapshot
    }

    fun refreshSnapshot(): UiSessionSnapshot {
        snapshot = decodeSnapshot(bridge.getSessionSnapshotJson(handle))
        return syncGuidanceGeometry()
    }

    fun replaceFlightPlan(plan: FlightPlan): UiSessionSnapshot {
        snapshot = decodeSnapshot(bridge.replaceFlightPlanInSessionJson(handle, json.encodeToString(plan.toWire())))
        return syncGuidanceGeometry()
    }

    fun performFlightPlanRowAction(rowUid: String, actionUid: String): UiSessionSnapshot {
        val store = navKvStore ?: error("nav_kv store is required to perform flight plan row action")
        snapshot = json.decodeFromJsonElement<WireUiSessionSnapshot>(
            store.runPagedSessionOperationElement {
                bridge.performFlightPlanRowActionInSessionJson(handle, rowUid, actionUid)
            },
        ).toUi()
        return syncGuidanceGeometry()
    }

    fun activateNextLeg(): UiSessionSnapshot {
        snapshot = decodeSnapshot(bridge.activateNextLegInSessionJson(handle))
        return syncGuidanceGeometry()
    }

    fun suspendSequencing(): UiSessionSnapshot {
        snapshot = decodeSnapshot(bridge.suspendSequencingInSessionJson(handle))
        return syncGuidanceGeometry()
    }

    fun unsuspendSequencing(): UiSessionSnapshot {
        snapshot = decodeSnapshot(bridge.unsuspendSequencingInSessionJson(handle))
        return syncGuidanceGeometry()
    }

    fun sequenceActiveLeg(): UiSessionSnapshot {
        snapshot = decodeSnapshot(bridge.sequenceActiveLegInSessionJson(handle))
        return syncGuidanceGeometry()
    }

    fun restoreChartPageState(
        recentAirportIds: List<String>,
        selectedAirportId: String?,
        selectedChartId: String?,
    ): UiSessionSnapshot {
        snapshot =
            decodeSnapshot(
                bridge.restoreChartPageStateInSessionJson(
                    handle,
                    json.encodeToString(recentAirportIds),
                    json.encodeToString(selectedAirportId),
                    json.encodeToString(selectedChartId),
                ),
            )
        return snapshot
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

    fun queryMapOverlay(
        viewport: MapViewportState,
        widthPx: Double,
        heightPx: Double,
        fetchResource: (CoreResourceRequest) -> ByteArray,
    ): MapOverlayQueryResult {
        val viewportJson = json.encodeToString(viewport.toWire())
        val store = navKvStore ?: error("session missing nav_db for map overlay")
        return json.decodeFromJsonElement<WireMapOverlayQueryResult>(
            store.runPagedSessionOperationElement(
                operation = {
                    bridge.getMapOverlayInSessionJson(handle, viewportJson, widthPx, heightPx)
                },
                fetchSessionResource = fetchResource,
                ingestSessionResource = { resource, bytes ->
                    bridge.ingestResourceInSession(handle, resource.id, bytes)
                },
            ),
        ).toUi()
    }

    fun queryMapSelection(
        viewport: MapViewportState,
        widthPx: Double,
        heightPx: Double,
        click: LatLonPoint,
        hitRadiusPx: Double,
    ): MapSelectionQueryResult {
        val viewportJson = json.encodeToString(viewport.toWire())
        val clickJson = json.encodeToString(click.toWire())
        val store = navKvStore ?: error("session missing nav_db for map selection")
        return json.decodeFromJsonElement<WireMapSelectionQueryResult>(
            store.runPagedSessionOperationElement {
                bridge.getMapSelectionInSessionJson(handle, viewportJson, widthPx, heightPx, clickJson, hitRadiusPx)
            },
        ).toUi()
    }

    fun queryTerrainOverlay(
        viewport: MapViewportState,
        widthPx: Double,
        heightPx: Double,
        fetchResource: (CoreResourceRequest) -> ByteArray,
    ): TerrainOverlayQueryResult {
        val viewportJson = json.encodeToString(viewport.toWire())
        val store = navKvStore ?: error("session missing nav_db for terrain overlay")
        return json.decodeFromJsonElement<WireTerrainOverlayQueryResult>(
            store.runPagedSessionOperationElement(
                operation = {
                    bridge.getTerrainOverlayInSessionJson(handle, viewportJson, widthPx, heightPx)
                },
                fetchSessionResource = fetchResource,
                ingestSessionResource = { resource, bytes ->
                    bridge.ingestResourceInSession(handle, resource.id, bytes)
                },
            ),
        ).toUi()
    }

    fun queryRasterTilePlanJson(
        viewport: MapViewportState,
        widthPx: Double,
        heightPx: Double,
    ): String {
        val viewportJson = json.encodeToString(viewport.toWire())
        return bridge.getRasterTilePlanInSessionJson(handle, viewportJson, widthPx, heightPx)
    }

    fun renderTerrainOverlayTileByKey(tileKey: String, aircraftAltitudeFt: Double): ByteArray =
        bridge.renderTerrainOverlayTileByKeyInSession(handle, tileKey, aircraftAltitudeFt)

    fun syncMapFollow(viewport: MapViewportState, widthPx: Double, heightPx: Double): UiSessionSnapshot {
        val viewportJson = json.encodeToString(viewport.toWire())
        snapshot = decodeSnapshot(bridge.syncMapFollowInSessionJson(handle, viewportJson, widthPx, heightPx))
        return snapshot
    }

    fun destroy() {
        bridge.destroySession(handle)
    }

    private fun decodeSnapshot(snapshotJson: String): UiSessionSnapshot =
        json.decodeFromString<WireUiSessionSnapshot>(snapshotJson).toUi()
}

private fun MapViewportState.toWire(): WireMapViewport {
    val (lat, lon) = viewportCenterLatLon(this)
    return WireMapViewport(
        center = WireLatLon(lat = lat, lon = lon),
        zoom = zoom,
        rotation_deg = 0.0,
        pitch_deg = 0.0,
    )
}

private fun FlightPlan.toWire() = WireFlightPlan(
    id = id,
    name = name,
    legs = legs.map { it.toWire() },
    route_components = routeComponents.map { it.toWire() },
    route_component_uids = routeComponentUids,
    route_component_uid_counter = routeComponentUidCounter,
    resolved_legs = resolvedLegs.map { it.toWire() },
    guidance = guidance?.toWire(),
    departure = departure,
    destination = destination,
    alternate = alternate,
    cruise_altitude_ft = cruiseAltitudeFt,
    notes = notes,
    updated_at_epoch_ms = updatedAtEpochMs,
    version = version,
)

private fun FlightPlanLeg.toWire() = WirePlanLeg(
    from = from.toWire(),
    to = to.toWire(),
    airway = airway,
)

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

private fun ContentPolicy.toWire() = when (this) {
    ContentPolicy.OfflineRequired -> WireContentPolicy.OfflineRequired
    ContentPolicy.PreferLocal -> WireContentPolicy.PreferLocal
    ContentPolicy.StreamAllowed -> WireContentPolicy.StreamAllowed
}

private fun ContentInventory.toWire() = WireContentInventory(
    installed_packages = installedPackages.map {
        WireInstalledPackage(
            package_id = it.packageId.toWire(),
            integrity_ok = it.integrityOk,
        )
    },
)

private fun AppState.toWire() = WireAppState(
    active_plan = activePlan?.toWire(),
    ownship = WireOwnshipState(),
    content_policy = contentPolicy.toWire(),
    last_content_report = lastContentReport?.let { report ->
        WireContentReport(
            fully_satisfied = report.fullySatisfied,
            items = report.items.map { item ->
                WireContentReportItem(
                    label = item.label,
                    availability = WireAvailabilityDetail(
                        availability = item.availability.availability.toWire(),
                        cycle_current = item.availability.cycleCurrent,
                        integrity_ok = item.availability.integrityOk,
                        cached = item.availability.cached,
                        offline_usable = item.availability.offlineUsable,
                    ),
                )
            },
        )
    },
)

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
    orientation_deg = orientationDeg,
    speed_kt = speedKt,
)

private fun OwnshipControlModel.toWire() = WireOwnshipControlModel(
    mode = mode.toWire(),
    selection = selection.toWire(),
    launcher_label = launcherLabel,
    launcher_tone = launcherTone.toWire(),
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
    active = active,
    status_label = statusLabel,
)

private fun SituationControlMenuItem.toWire() = WireSituationControlMenuItem(
    input = input.toWire(),
    label = label,
    enabled = enabled,
)

private fun SituationControlInput.toWire(): WireSituationControlInput = when (this) {
    SituationControlInput.SkipBackward -> WireSituationControlInput.SkipBackward
    SituationControlInput.FastRewind -> WireSituationControlInput.FastRewind
    SituationControlInput.FastForward -> WireSituationControlInput.FastForward
    SituationControlInput.SkipForward -> WireSituationControlInput.SkipForward
}

private fun WireAppState.toUi() = AppState(
    activePlan = active_plan?.toUiFlightPlan(),
    contentPolicy = content_policy.toUi(),
    lastContentReport = last_content_report?.let { report ->
        ContentReport(
            fullySatisfied = report.fully_satisfied,
            items = report.items.map { item ->
                ContentReportItem(
                    label = item.label,
                    availability = AvailabilityDetail(
                        availability = item.availability.availability.toUi(),
                        cycleCurrent = item.availability.cycle_current,
                        integrityOk = item.availability.integrity_ok,
                        cached = item.availability.cached,
                        offlineUsable = item.availability.offline_usable,
                    ),
                )
            },
        )
    },
)

private fun WireAppUiState.toUi() = AppUiState(
    activePlan = active_plan?.toUi(),
    ownship = ownship.toUi(),
    contentPolicy = content_policy.toUi(),
    lastContentReport = last_content_report?.let { report ->
        ContentReport(
            fullySatisfied = report.fully_satisfied,
            items = report.items.map { item ->
                ContentReportItem(
                    label = item.label,
                    availability =
                        AvailabilityDetail(
                            availability = item.availability.availability.toUi(),
                            cycleCurrent = item.availability.cycle_current,
                            integrityOk = item.availability.integrity_ok,
                            cached = item.availability.cached,
                            offlineUsable = item.availability.offline_usable,
                        ),
                )
            },
        )
    },
)

private fun WireUiSnapshotAppState.toUi() = UiSnapshotAppState(
    activePlan = active_plan?.toUiFlightPlan(),
    contentPolicy = content_policy.toUi(),
    lastContentReport = last_content_report?.let { report ->
        ContentReport(
            fullySatisfied = report.fully_satisfied,
            items = report.items.map { item ->
                ContentReportItem(
                    label = item.label,
                    availability =
                        AvailabilityDetail(
                            availability = item.availability.availability.toUi(),
                            cycleCurrent = item.availability.cycle_current,
                            integrityOk = item.availability.integrity_ok,
                            cached = item.availability.cached,
                            offlineUsable = item.availability.offline_usable,
                        ),
                )
            },
        )
    },
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
    orientationDeg = orientation_deg,
    speedKt = speed_kt,
)

private fun WireOwnshipControlModel.toUi() = OwnshipControlModel(
    mode = mode.toUi(),
    selection = selection.toUi(),
    launcherLabel = launcher_label,
    launcherTone = launcher_tone.toUi(),
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
    active = active,
    statusLabel = status_label,
)

private fun WireSituationControlMenuItem.toUi() = SituationControlMenuItem(
    input = input.toUi(),
    label = label,
    enabled = enabled,
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

private fun WireOwnshipSourceKind.toUi(): OwnshipSourceKind = when (this) {
    WireOwnshipSourceKind.DeviceGps -> OwnshipSourceKind.DeviceGps
    WireOwnshipSourceKind.ExternalGps -> OwnshipSourceKind.ExternalGps
    WireOwnshipSourceKind.ExternalAhrs -> OwnshipSourceKind.ExternalAhrs
    WireOwnshipSourceKind.GpxPlayback -> OwnshipSourceKind.GpxPlayback
    WireOwnshipSourceKind.AdsbTrackPlayback -> OwnshipSourceKind.AdsbTrackPlayback
    WireOwnshipSourceKind.LiveNetworkTrack -> OwnshipSourceKind.LiveNetworkTrack
    WireOwnshipSourceKind.FlightPlanSimulator -> OwnshipSourceKind.FlightPlanSimulator
    WireOwnshipSourceKind.DebugOwnshipDriver -> OwnshipSourceKind.DebugOwnshipDriver
}

private fun OwnshipSourceKind.toWire(): WireOwnshipSourceKind = when (this) {
    OwnshipSourceKind.DeviceGps -> WireOwnshipSourceKind.DeviceGps
    OwnshipSourceKind.ExternalGps -> WireOwnshipSourceKind.ExternalGps
    OwnshipSourceKind.ExternalAhrs -> WireOwnshipSourceKind.ExternalAhrs
    OwnshipSourceKind.GpxPlayback -> WireOwnshipSourceKind.GpxPlayback
    OwnshipSourceKind.AdsbTrackPlayback -> WireOwnshipSourceKind.AdsbTrackPlayback
    OwnshipSourceKind.LiveNetworkTrack -> WireOwnshipSourceKind.LiveNetworkTrack
    OwnshipSourceKind.FlightPlanSimulator -> WireOwnshipSourceKind.FlightPlanSimulator
    OwnshipSourceKind.DebugOwnshipDriver -> WireOwnshipSourceKind.DebugOwnshipDriver
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
        put("track_deg_true", trackDegTrue?.let { kotlinx.serialization.json.JsonPrimitive(it) } ?: kotlinx.serialization.json.JsonNull)
        put("heading_deg_true", headingDegTrue?.let { kotlinx.serialization.json.JsonPrimitive(it) } ?: kotlinx.serialization.json.JsonNull)
        put("ground_speed_kt", groundSpeedKt?.let { kotlinx.serialization.json.JsonPrimitive(it) } ?: kotlinx.serialization.json.JsonNull)
        put("altitude_msl_ft", altitudeMslFt?.let { kotlinx.serialization.json.JsonPrimitive(it) } ?: kotlinx.serialization.json.JsonNull)
        put("pressure_altitude_ft", pressureAltitudeFt?.let { kotlinx.serialization.json.JsonPrimitive(it) } ?: kotlinx.serialization.json.JsonNull)
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
    OwnshipSourceKind.DebugOwnshipDriver -> "debug_ownship_driver"
}

private fun SourceConnectionState.toWireName(): String = when (this) {
    SourceConnectionState.Unavailable -> "unavailable"
    SourceConnectionState.Searching -> "searching"
    SourceConnectionState.Connected -> "connected"
    SourceConnectionState.Stale -> "stale"
    SourceConnectionState.Failed -> "failed"
}

internal fun WireFlightPlan.toUiFlightPlan() = FlightPlan(
    id = id,
    name = name,
    legs = legs.map { it.toUi() },
    routeComponents = route_components.map { it.toUi() },
    routeComponentUids = route_component_uids,
    routeComponentUidCounter = route_component_uid_counter,
    resolvedLegs = resolved_legs.map { it.toUi() },
    guidance = guidance?.toUi(),
    departure = departure,
    destination = destination,
    alternate = alternate,
    cruiseAltitudeFt = cruise_altitude_ft,
    notes = notes,
    updatedAtEpochMs = updated_at_epoch_ms,
    version = version,
)

@kotlinx.serialization.Serializable
internal data class WireDerivedChartPage(
    val airports: List<WireDerivedChartAirport>,
)

@kotlinx.serialization.Serializable
private data class WireDerivedChartPageState(
    val airports: List<WireDerivedChartAirport>,
    val recent_airport_ids: List<String>,
    val selected_airport_id: String,
    val selected_chart_id: String,
)

@kotlinx.serialization.Serializable
private data class WireUiChartPageState(
    val ordered_airport_ids: List<String>,
    val recent_airport_ids: List<String>,
    val selected_airport_id: String,
    val selected_chart_id: String,
)

@kotlinx.serialization.Serializable
private data class WireUiMapLayerToggleState(
    val visible: Boolean = false,
    val enabled: Boolean = false,
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
private data class WireUiDebugState(
    val tile_labels: Boolean = false,
    val playback_visible: Boolean = false,
    val fast_tiles: Boolean = false,
    val offline_simulated_clock_buttons: Boolean = false,
)

@kotlinx.serialization.Serializable
private data class WireUiSessionSnapshot(
    val app_state: WireUiSnapshotAppState,
    val app_ui_state: WireAppUiState = WireAppUiState(),
    val playback_ui_state: WirePlaybackUiState = WirePlaybackUiState(),
    val map_follow_ui_state: WireMapFollowUiState = WireMapFollowUiState(),
    val map_follow_target_viewport: WireMapViewport? = null,
    val chart_page_state: WireUiChartPageState,
    val map_layer_state: WireUiMapLayerState = WireUiMapLayerState(),
    val debug_state: WireUiDebugState = WireUiDebugState(),
    val raster_map: WireRasterMapUiState? = null,
)

@kotlinx.serialization.Serializable
private data class WireRasterMapUiState(
    val selected_map_id: String = "",
    val selected_map_label: String = "",
    val selected_family_id: WireChartFamilyId,
    val selected_family_label: String = "",
    val selected_family_launcher_label: String = "",
    val min_zoom: Double = 0.0,
    val max_zoom: Double = 0.0,
    val initial_viewport: WireMapViewportSeed,
    val family_options: List<WireMapFamilyOption> = emptyList(),
)

@kotlinx.serialization.Serializable
private data class WireMapFamilyOption(
    val id: WireChartFamilyId,
    val label: String,
    val launcher_label: String,
    val enabled: Boolean,
    val active: Boolean,
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

@kotlinx.serialization.Serializable
internal data class WireDerivedChartAsset(
    val id: String,
    val airport_id: String,
    val package_id: String,
    val label: String,
    val kind: String,
    val folder_category: String,
    val source_asset_path: String,
    val asset_path: String,
    val thumbnail_source_path: String? = null,
    val thumbnail_path: String? = null,
)

internal fun WireDerivedChartPage.toUi() = ChartPageFixture(
    airports = airports.map { it.toUi() },
)

data class DerivedChartPageState(
    val airports: List<ChartAirport>,
    val recentAirportIds: List<String>,
    val selectedAirportId: String,
    val selectedChartId: String,
)

data class UiSessionSnapshot(
    val appState: UiSnapshotAppState,
    val appUiState: AppUiState,
    val playbackUiState: PlaybackUiState,
    val mapFollowUiState: MapFollowUiState,
    val mapFollowTargetViewport: CoreMapViewport?,
    val chartPageState: UiChartPageState,
    val mapLayerState: UiMapLayerState,
    val debugState: UiDebugState,
    val rasterMap: RasterMapUiState?,
)

data class UiDebugState(
    val tileLabels: Boolean,
    val playbackVisible: Boolean,
    val fastTiles: Boolean,
    val offlineSimulatedClockButtons: Boolean,
)

data class UiChartPageState(
    val orderedAirportIds: List<String>,
    val recentAirportIds: List<String>,
    val selectedAirportId: String,
    val selectedChartId: String,
)

private fun WireRasterMapUiState.toUi() = RasterMapUiState(
    selectedMapId = selected_map_id,
    selectedMapLabel = selected_map_label,
    selectedFamilyId = selected_family_id.toUi(),
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
    id = id.toUi(),
    label = label,
    launcherLabel = launcher_label,
    enabled = enabled,
    active = active,
)

private fun WireChartFamilyId.toUi() = when (this) {
    WireChartFamilyId.Sec -> MapChartFamily.Sec
    WireChartFamilyId.Tac -> MapChartFamily.Tac
    WireChartFamilyId.EnrL -> MapChartFamily.EnrL
    WireChartFamilyId.EnrH -> MapChartFamily.EnrH
    WireChartFamilyId.ShadedRelief -> MapChartFamily.ShadedRelief
    WireChartFamilyId.WorldBasemap -> MapChartFamily.WorldBasemap
}

private fun MapChartFamily.toWireName(): String = when (this) {
    MapChartFamily.Sec -> "sec"
    MapChartFamily.Tac -> "tac"
    MapChartFamily.EnrL -> "enr-l"
    MapChartFamily.EnrH -> "enr-h"
    MapChartFamily.ShadedRelief -> "shaded-relief"
    MapChartFamily.WorldBasemap -> "world-basemap"
}

private fun WireDerivedChartPageState.toUi() = DerivedChartPageState(
    airports = airports.map { it.toUi() },
    recentAirportIds = recent_airport_ids,
    selectedAirportId = selected_airport_id,
    selectedChartId = selected_chart_id,
)

private fun WireUiChartPageState.toUi() = UiChartPageState(
    orderedAirportIds = ordered_airport_ids,
    recentAirportIds = recent_airport_ids,
    selectedAirportId = selected_airport_id,
    selectedChartId = selected_chart_id,
)

private fun WireUiMapLayerToggleState.toUi() = UiMapLayerToggleState(
    visible = visible,
    enabled = enabled,
)

private fun WireUiMapLayerState.toUi() = UiMapLayerState(
    worldBasemap = world_basemap.toUi(),
    vectors = vectors.toUi(),
    metars = metars.toUi(),
    nexrad = nexrad.toUi(),
    terrainWarning = terrain_warning.toUi(),
    offlineRegions = offline_regions.toUi(),
)

private fun WireUiDebugState.toUi() = UiDebugState(
    tileLabels = tile_labels,
    playbackVisible = playback_visible,
    fastTiles = fast_tiles,
    offlineSimulatedClockButtons = offline_simulated_clock_buttons,
)

private fun WireUiSessionSnapshot.toUi() = UiSessionSnapshot(
    appState = app_state.toUi(),
    appUiState = app_ui_state.toUi(),
    playbackUiState = playback_ui_state.toUi(),
    mapFollowUiState = map_follow_ui_state.toUi(),
    mapFollowTargetViewport = map_follow_target_viewport?.toUi(),
    chartPageState = chart_page_state.toUi(),
    mapLayerState = map_layer_state.toUi(),
    debugState = debug_state.toUi(),
    rasterMap = raster_map?.toUi(),
)

internal fun WireDerivedChartAirport.toUi() = ChartAirport(
    id = id,
    label = label,
    charts = charts.map { it.toUi() },
)

internal fun WireDerivedChartAsset.toUi() = ChartAsset(
    id = id,
    airportId = airport_id,
    packageId = package_id,
    label = label,
    kind = kind,
    folderCategory = folder_category,
    sourceAssetPath = source_asset_path,
    assetPath = asset_path,
    thumbnailSourceAssetPath = thumbnail_source_path,
    thumbnailAssetPath = thumbnail_path,
)

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
    rotationDeg = 0.0,
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
    warnings = warnings.map { it.toUi() },
)

private fun WireAirspaceFeatureRequest.toUi() = AirspaceFeatureRequest(
    id = id,
    path = path,
)

private fun WireTerrainOverlayQueryResult.toUi() = TerrainOverlayQueryResult(
    status = status.toUi(),
    tileRequests = tile_requests.map { it.toUi() },
)

private fun WireTerrainOverlayStatus.toUi(): TerrainOverlayStatus = when (this) {
    is WireTerrainOverlayStatusHidden -> TerrainOverlayStatus.Hidden
    is WireTerrainOverlayStatusNoPosition -> TerrainOverlayStatus.NoPosition
    is WireTerrainOverlayStatusNoAltitude -> TerrainOverlayStatus.NoAltitude
    is WireTerrainOverlayStatusTooManyTiles -> TerrainOverlayStatus.TooManyTiles(count)
    is WireTerrainOverlayStatusReady -> TerrainOverlayStatus.Ready(count)
}

private fun WireTerrainOverlayTileRequest.toUi() = TerrainOverlayTileRequest(
    key = key,
    z = z,
    x = x,
    yTms = y_tms,
    left = left,
    top = top,
    size = size,
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
    styleClass = style_class,
    obstacleVariant = obstacle_variant,
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
)

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
    categories = categories.map { it.toUi() },
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
    is WireMapSelectionHighlightSpot -> MapSelectionHighlight.Spot(lat, lon)
    is WireMapSelectionHighlight.FeatureRef -> MapSelectionHighlight.FeatureRef(id)
    is WireMapSelectionHighlight.Metar -> MapSelectionHighlight.Metar(station_id)
    is WireMapSelectionHighlight.Pirep -> MapSelectionHighlight.Pirep(id)
    is WireMapSelectionHighlight.Spot -> MapSelectionHighlight.Spot(lat, lon)
}

private fun WireMapSelectionAction.toUi() = MapSelectionAction(
    id = id,
    label = label,
    enabled = enabled,
    displayOnly = display_only,
    airspaceLimit = airspace_limit?.toUi(),
    sessionAction = session_action,
    flightPlanRowAction = flight_plan_row_action?.toUi(),
)

private fun WireMapSelectionFlightPlanRowAction.toUi() = MapSelectionFlightPlanRowAction(
    rowUid = row_uid,
    actionUid = action_uid,
)

private fun WireNavSymbolFeature.toUi() = NavSymbolFeature(
    kind = kind,
    label = label,
    styleClass = style_class,
    obstacleVariant = obstacle_variant,
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
    style_class = styleClass,
    obstacle_variant = obstacleVariant,
    towered = towered,
    fuel_available = fuelAvailable,
    has_paved_runway = hasPavedRunway,
    heliport = heliport,
    has_water_runway = hasWaterRunway,
    runway_length_ratio = runwayLengthRatio,
    longest_runway_heading_true_deg = longestRunwayHeadingTrueDeg,
)

private fun WireMapOverlayWarning.toUi() = MapOverlayWarning(
    code = code,
    message = message,
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

private fun WireAirwayEntryCandidate.toUi() = AirwayEntryCandidate(
    airwayName = airway_name,
    branchKey = branch_key,
    branchPointIndex = branch_point_index,
    sequence = sequence,
    navRef = nav_ref.toUi(),
    distanceFromAnchorNm = distance_from_anchor_nm,
    previousNavRef = previous_nav_ref?.toUi(),
    nextNavRef = next_nav_ref?.toUi(),
)

private fun AirwayEntryCandidate.toWire() = WireAirwayEntryCandidate(
    airway_name = airwayName,
    branch_key = branchKey,
    branch_point_index = branchPointIndex,
    sequence = sequence,
    nav_ref = navRef.toWire(),
    distance_from_anchor_nm = distanceFromAnchorNm,
    previous_nav_ref = previousNavRef?.toWire(),
    next_nav_ref = nextNavRef?.toWire(),
)

private fun WireAirwayExitCandidate.toUi() = AirwayExitCandidate(
    airwayName = airway_name,
    branchKey = branch_key,
    branchPointIndex = branch_point_index,
    sequence = sequence,
    navRef = nav_ref.toUi(),
    legOffsetFromEntry = leg_offset_from_entry,
    isEntry = is_entry,
    distanceFromTargetNm = distance_from_target_nm,
)

private fun AirwayExitCandidate.toWire() = WireAirwayExitCandidate(
    airway_name = airwayName,
    branch_key = branchKey,
    branch_point_index = branchPointIndex,
    sequence = sequence,
    nav_ref = navRef.toWire(),
    leg_offset_from_entry = legOffsetFromEntry,
    is_entry = isEntry,
    distance_from_target_nm = distanceFromTargetNm,
)

private fun AirwayAutoSelection.toWire() = WireAirwayAutoSelection(
    airway_name = airwayName,
    branch_key = branchKey,
    entry = entry.toWire(),
    exit = exit.toWire(),
    origin_distance_nm = originDistanceNm,
    destination_distance_nm = destinationDistanceNm,
    total_anchor_distance_nm = totalAnchorDistanceNm,
)

private fun WireAirwayAutoSelection.toUi() = AirwayAutoSelection(
    airwayName = airway_name,
    branchKey = branch_key,
    entry = entry.toUi(),
    exit = exit.toUi(),
    originDistanceNm = origin_distance_nm,
    destinationDistanceNm = destination_distance_nm,
    totalAnchorDistanceNm = total_anchor_distance_nm,
)

private fun AirwaySegment.toWire() = WireAirwaySegment(
    name = name,
    branch_key = branchKey,
    entry = entry.toWire(),
    exit = exit.toWire(),
)

private fun WireAirwaySegment.toUi() = AirwaySegment(
    name = name,
    branchKey = branch_key,
    entry = entry.toUi(),
    exit = exit.toUi(),
)

private fun WireAirwayFixPoint.toUi() = AirwayFixPoint(
    airwayName = airway_name,
    sequence = sequence,
    position = position.toUi(),
    navRef = nav_ref.toUi(),
)

private fun AirwayFixPoint.toWire() = WireAirwayFixPoint(
    airway_name = airwayName,
    sequence = sequence,
    position = position.toWire(),
    nav_ref = navRef.toWire(),
)

private fun WireAirwayBranch.toUi() = AirwayBranch(
    displayName = display_name,
    branchKey = branch_key,
    points = points.map { it.toUi() },
)

private fun AirwayBranch.toWire() = WireAirwayBranch(
    display_name = displayName,
    branch_key = branchKey,
    points = points.map { it.toWire() },
)

private fun WireAirwayPresentationPlan.toUi() = AirwayPresentationPlan(
    airwayName = airway_name,
    branchKey = branch_key,
    points = points.map { it.toUi() },
    suggestedEntryIndex = suggested_entry_index,
    suggestedExitIndex = suggested_exit_index,
)

private fun AirwayPresentationPlan.toWire() = WireAirwayPresentationPlan(
    airway_name = airwayName,
    branch_key = branchKey,
    points = points.map { it.toWire() },
    suggested_entry_index = suggestedEntryIndex,
    suggested_exit_index = suggestedExitIndex,
)

private fun WireAirwayPresentationPoint.toUi() = AirwayPresentationPoint(
    branchPointIndex = branch_point_index,
    sequence = sequence,
    navRef = nav_ref.toUi(),
)

private fun AirwayPresentationPoint.toWire() = WireAirwayPresentationPoint(
    branch_point_index = branchPointIndex,
    sequence = sequence,
    nav_ref = navRef.toWire(),
)

private fun WireMaterializedAirway.toUi() = MaterializedAirway(
    selection = selection.toUi(),
    airway = airway.toUi(),
    resolvedLegs = resolvedLegs.map { it.toUi() },
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

private fun ProcedureSegment.toWire() = WireProcedureSegment(
    airport_id = airportId,
    procedure_id = procedureId,
    kind = kind.toWire(),
    runway_transition = runwayTransition,
    enroute_transition = enrouteTransition,
    terminal_discontinuity = terminalDiscontinuity?.toWire(),
)

private fun WireProcedureSegment.toUi() = ProcedureSegment(
    airportId = airport_id,
    procedureId = procedure_id,
    kind = kind.toUi(),
    runwayTransition = runway_transition,
    enrouteTransition = enroute_transition,
    terminalDiscontinuity = terminal_discontinuity?.toUi(),
)

private fun ProcedureDiscontinuity.toWire(): WireProcedureDiscontinuity = when (this) {
    ProcedureDiscontinuity.Vectors -> WireProcedureDiscontinuity.Vectors
    ProcedureDiscontinuity.Hold -> WireProcedureDiscontinuity.Hold
    is ProcedureDiscontinuity.Other -> WireProcedureDiscontinuity.Other(value)
}

private fun WireProcedureDiscontinuity.toUi(): ProcedureDiscontinuity = when (this) {
    WireProcedureDiscontinuity.Vectors -> ProcedureDiscontinuity.Vectors
    WireProcedureDiscontinuity.Hold -> ProcedureDiscontinuity.Hold
    is WireProcedureDiscontinuity.Other -> ProcedureDiscontinuity.Other(value)
}

private fun ResolvedLeg.toWire() = WireResolvedLeg(
    id = id,
    from = from.toWire(),
    to = to.toWire(),
    source = source.toWire(),
    procedure_airport_id = procedureAirportId,
)

private fun WireResolvedLeg.toUi() = ResolvedLeg(
    id = id,
    from = from.toUi(),
    to = to.toUi(),
    source = source.toUi(),
    procedureAirportId = procedure_airport_id,
)

private fun RouteComponent.toWire(): WireRouteComponent = when (this) {
    is RouteComponent.Waypoint -> WireRouteComponent.Waypoint(waypoint = waypoint.toWire())
    is RouteComponent.Airway -> WireRouteComponent.Airway(airway = airway.toWire())
    is RouteComponent.Procedure -> WireRouteComponent.Procedure(procedure = procedure.toWire())
}

private fun WireRouteComponent.toUi(): RouteComponent = when (this) {
    is WireRouteComponent.Waypoint -> RouteComponent.Waypoint(waypoint = waypoint.toUi())
    is WireRouteComponent.Airway -> RouteComponent.Airway(airway = airway.toUi())
    is WireRouteComponent.Procedure -> RouteComponent.Procedure(procedure = procedure.toUi())
}

private fun GuidanceState.toWire() = WireGuidanceState(
    active_leg_index = activeLegIndex,
    active_detail_index = activeDetailIndex,
    display_split_leg_id = displaySplitLegId,
    sequencing_mode = sequencingMode.toWire(),
    direct_to = directTo?.toWire(),
    suspend_reason = suspendReason?.toWire(),
)

private fun WireGuidanceState.toUi() = GuidanceState(
    activeLegIndex = active_leg_index,
    activeDetailIndex = active_detail_index,
    displaySplitLegId = display_split_leg_id,
    sequencingMode = sequencing_mode.toUi(),
    directTo = direct_to?.toUi(),
    suspendReason = suspend_reason?.toUi(),
)

private fun DirectToState.toWire() = WireDirectToState(
    start = start.toWire(),
    target = target.toWire(),
    target_component_uid = targetComponentUid,
    target_leg_id = targetLegId,
    resume_leg_id = resumeLegId,
)

private fun WireDirectToState.toUi() = DirectToState(
    start = start.toUi(),
    target = target.toUi(),
    targetComponentUid = target_component_uid,
    targetLegId = target_leg_id,
    resumeLegId = resume_leg_id,
)

private fun WirePlanLeg.toUiPlanLeg() = PlanLeg(
    from = from.toUi(),
    to = to.toUi(),
    airway = airway,
)

private fun ResolvedLegSource.toWire(): WireResolvedLegSource = when (this) {
    is ResolvedLegSource.LegacyPlanLeg -> WireResolvedLegSource.LegacyPlanLeg(leg_index = legIndex)
    is ResolvedLegSource.RouteComponent -> WireResolvedLegSource.RouteComponent(component_index = componentIndex)
    is ResolvedLegSource.SyntheticBridge -> WireResolvedLegSource.SyntheticBridge(
        from_component_index = fromComponentIndex,
        to_component_index = toComponentIndex,
    )
}

private fun WireResolvedLegSource.toUi(): ResolvedLegSource = when (this) {
    is WireResolvedLegSource.LegacyPlanLeg -> ResolvedLegSource.LegacyPlanLeg(legIndex = leg_index)
    is WireResolvedLegSource.RouteComponent -> ResolvedLegSource.RouteComponent(componentIndex = component_index)
    is WireResolvedLegSource.SyntheticBridge -> ResolvedLegSource.SyntheticBridge(
        fromComponentIndex = from_component_index,
        toComponentIndex = to_component_index,
    )
}

private fun WireRouteComponentUiView.toUi() = RouteComponentUiView(
    uid = uid,
    componentIndex = component_index,
    kind = kind.toUi(),
    summary = summary,
    procedureId = procedure_id,
    procedureKind = procedure_kind?.toUi(),
    chartAirportId = chart_airport_id,
    items = items.map { it.toUi() },
    active = active,
    canAddAirwayAfter = can_add_airway_after,
    canAddProcedureBefore = can_add_procedure_before,
    canRemove = can_remove,
    canReorder = can_reorder,
    canReorderUp = can_reorder_up,
    canReorderDown = can_reorder_down,
    precedingWaypoint = preceding_waypoint?.toUi(),
    followingWaypoint = following_waypoint?.toUi(),
)

private fun RouteComponentUiView.toWire() = WireRouteComponentUiView(
    uid = uid,
    component_index = componentIndex,
    kind = kind.toWire(),
    summary = summary,
    procedure_id = procedureId,
    procedure_kind = procedureKind?.toWire(),
    chart_airport_id = chartAirportId,
    items = items.map { it.toWire() },
    active = active,
    can_add_airway_after = canAddAirwayAfter,
    can_add_procedure_before = canAddProcedureBefore,
    can_remove = canRemove,
    can_reorder = canReorder,
    can_reorder_up = canReorderUp,
    can_reorder_down = canReorderDown,
    preceding_waypoint = precedingWaypoint?.toWire(),
    following_waypoint = followingWaypoint?.toWire(),
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

private fun WireConcretizedNavItem.toUi(): ConcretizedNavItem = when (this) {
    is WireConcretizedNavItem.Waypoint -> ConcretizedNavItem.Waypoint(navRef = nav_ref.toUi())
    is WireConcretizedNavItem.Discontinuity -> ConcretizedNavItem.Discontinuity(
        discontinuity = discontinuity.toUi(),
        label = label,
    )
}

private fun ConcretizedNavItem.toWire(): WireConcretizedNavItem = when (this) {
    is ConcretizedNavItem.Waypoint -> WireConcretizedNavItem.Waypoint(nav_ref = navRef.toWire())
    is ConcretizedNavItem.Discontinuity -> WireConcretizedNavItem.Discontinuity(
        discontinuity = discontinuity.toWire(),
        label = label,
    )
}

private fun WireResolvedLegUiView.toUi() = ResolvedLegUiView(
    legIndex = leg_index,
    legId = leg_id,
    componentIndex = component_index,
    from = from.toUi(),
    to = to.toUi(),
    active = active,
    suspendBoundaryAfter = suspend_boundary_after,
)

private fun ResolvedLegUiView.toWire() = WireResolvedLegUiView(
    leg_index = legIndex,
    leg_id = legId,
    component_index = componentIndex,
    from = from.toWire(),
    to = to.toWire(),
    active = active,
    suspend_boundary_after = suspendBoundaryAfter,
)

private fun WireDirectToUiView.toUi() = DirectToUiView(
    start = start.toUi(),
    target = target.toUi(),
    targetComponentUid = target_component_uid,
    targetLegId = target_leg_id,
    resumeLegId = resume_leg_id,
    onPlanTarget = on_plan_target,
)

private fun DirectToUiView.toWire() = WireDirectToUiView(
    start = start.toWire(),
    target = target.toWire(),
    target_component_uid = targetComponentUid,
    target_leg_id = targetLegId,
    resume_leg_id = resumeLegId,
    on_plan_target = onPlanTarget,
)

private fun WireGuidanceUiView.toUi() = GuidanceUiView(
    sequencingMode = sequencing_mode.toUi(),
    activeLegIndex = active_leg_index,
    displaySplitLegIndex = display_split_leg_index,
    activeFromRowUid = active_from_row_uid,
    activeToRowUid = active_to_row_uid,
    activeComponentIndex = active_component_index,
    activeLeg = active_leg?.toUiPlanLeg(),
    navElement = nav_element.toUi(),
    directTo = direct_to?.toUi(),
    canSequenceActiveLeg = can_sequence_active_leg,
    canActivateNextLeg = can_activate_next_leg,
    canSuspend = can_suspend,
    canUnsuspend = can_unsuspend,
    suspendBoundaryAfterActiveLeg = suspend_boundary_after_active_leg,
)

private fun GuidanceUiView.toWire() = WireGuidanceUiView(
    sequencing_mode = sequencingMode.toWire(),
    active_leg_index = activeLegIndex,
    display_split_leg_index = displaySplitLegIndex,
    active_from_row_uid = activeFromRowUid,
    active_to_row_uid = activeToRowUid,
    active_component_index = activeComponentIndex,
    active_leg = activeLeg?.toWire(),
    nav_element = navElement.toWire(),
    direct_to = directTo?.toWire(),
    can_sequence_active_leg = canSequenceActiveLeg,
    can_activate_next_leg = canActivateNextLeg,
    can_suspend = canSuspend,
    can_unsuspend = canUnsuspend,
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

private fun WireSuspendReason.toUi() = when (this) {
    WireSuspendReason.Manual -> SuspendReason.Manual
    WireSuspendReason.Boundary -> SuspendReason.Boundary
    WireSuspendReason.RouteEnd -> SuspendReason.RouteEnd
    WireSuspendReason.DirectToComplete -> SuspendReason.DirectToComplete
}

private fun SuspendReason.toWire() = when (this) {
    SuspendReason.Manual -> WireSuspendReason.Manual
    SuspendReason.Boundary -> WireSuspendReason.Boundary
    SuspendReason.RouteEnd -> WireSuspendReason.RouteEnd
    SuspendReason.DirectToComplete -> WireSuspendReason.DirectToComplete
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

private fun WireRouteSegmentStatus.toUi() = when (this) {
    WireRouteSegmentStatus.Completed -> RouteSegmentStatus.Completed
    WireRouteSegmentStatus.Active -> RouteSegmentStatus.Active
    WireRouteSegmentStatus.ActiveLegRemaining -> RouteSegmentStatus.ActiveLegRemaining
    WireRouteSegmentStatus.Remaining -> RouteSegmentStatus.Remaining
}

private fun WireFlightPlanUiState.toUi() = FlightPlanUiState(
    components = components.map { it.toUi() },
    resolvedLegs = resolved_legs.map { it.toUi() },
    displayRows = display_rows.map { it.toUi() },
    guidance = guidance?.toUi(),
)

private fun FlightPlanUiState.toWire() = WireFlightPlanUiState(
    components = components.map { it.toWire() },
    resolved_legs = resolvedLegs.map { it.toWire() },
    display_rows = displayRows.map { it.toWire() },
    guidance = guidance?.toWire(),
)

private fun WireFlightPlanDisplayRowUiView.toUi() = FlightPlanDisplayRowUiView(
    uid = uid,
    label = label,
    rowKind = row_kind.toUi(),
    componentKind = component_kind?.toUi(),
    componentUid = component_uid,
    componentIndex = component_index,
    procedureId = procedure_id,
    procedureKind = procedure_kind?.toUi(),
    legIndex = leg_index,
    distanceNm = distance_nm,
    courseDeg = course_deg,
    etaText = eta_text,
    legTimeText = leg_time_text,
    fuelGalText = fuel_gal_text,
    showPlateTargetId = show_plate_target_id,
    chartAirportId = chart_airport_id,
    navRef = nav_ref?.toUi(),
    symbolFeature = symbol_feature?.toUi(),
    depth = depth,
    active = active,
    enabled = enabled,
    syntheticDirectTo = synthetic_direct_to,
    canAddAirwayAfter = can_add_airway_after,
    canAddProcedureBefore = can_add_procedure_before,
    canRemoveComponent = can_remove_component,
    canReorderComponent = can_reorder_component,
    canReorderUp = can_reorder_up,
    canReorderDown = can_reorder_down,
    replaceProcedureComponentIndex = replace_procedure_component_index,
    startComponentIndex = start_component_index,
    endComponentIndex = end_component_index,
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
    component_index = componentIndex,
    procedure_id = procedureId,
    procedure_kind = procedureKind?.toWire(),
    leg_index = legIndex,
    distance_nm = distanceNm,
    course_deg = courseDeg,
    eta_text = etaText,
    leg_time_text = legTimeText,
    fuel_gal_text = fuelGalText,
    show_plate_target_id = showPlateTargetId,
    chart_airport_id = chartAirportId,
    nav_ref = navRef?.toWire(),
    symbol_feature = symbolFeature?.toWire(),
    depth = depth,
    active = active,
    enabled = enabled,
    synthetic_direct_to = syntheticDirectTo,
    can_add_airway_after = canAddAirwayAfter,
    can_add_procedure_before = canAddProcedureBefore,
    can_remove_component = canRemoveComponent,
    can_reorder_component = canReorderComponent,
    can_reorder_up = canReorderUp,
    can_reorder_down = canReorderDown,
    replace_procedure_component_index = replaceProcedureComponentIndex,
    start_component_index = startComponentIndex,
    end_component_index = endComponentIndex,
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
}

private fun FlightPlanDisplayRowKind.toWire() = when (this) {
    FlightPlanDisplayRowKind.Waypoint -> WireFlightPlanDisplayRowKind.Waypoint
    FlightPlanDisplayRowKind.Group -> WireFlightPlanDisplayRowKind.Group
    FlightPlanDisplayRowKind.Discontinuity -> WireFlightPlanDisplayRowKind.Discontinuity
}

private fun WireFlightPlanRowActionUiView.toUi() = FlightPlanRowActionUiView(
    id = id,
    uid = uid,
    label = label,
    enabled = enabled,
    execution = execution,
    dismissTrayOnSuccess = dismiss_tray_on_success,
)

private fun FlightPlanRowActionUiView.toWire() = WireFlightPlanRowActionUiView(
    id = id,
    uid = uid,
    label = label,
    enabled = enabled,
    execution = execution,
    dismiss_tray_on_success = dismissTrayOnSuccess,
)

private fun WireFlightPlanUiMutation.toUi() = FlightPlanUiMutation(
    plan = plan.toUiFlightPlan(),
    uiState = ui_state.toUi(),
)

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

private fun WireAirwayPlanUiMutation.toUi() = FlightPlanUiMutation(
    plan = mutation.plan.toUiFlightPlan(),
    uiState = ui_state.toUi(),
)

private fun MaterializedProcedure.toWire() = WireMaterializedProcedure(
    procedure = procedure.toWire(),
    concretized_items = concretizedItems.map { it.toWire() },
    resolved_legs = resolvedLegs.map { it.toWire() },
)

private fun WireMaterializedProcedure.toUi() = MaterializedProcedure(
    procedure = procedure.toUi(),
    concretizedItems = concretized_items.map { it.toUi() },
    resolvedLegs = resolved_legs.map { it.toUi() },
)

private fun WirePlanLeg.toUi() = FlightPlanLeg(
    from = from.toUi(),
    to = to.toUi(),
    airway = airway,
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

private fun PackageId.toWire() = WirePackageId(
    region = region.toWireRegion(),
    family = when (family) {
        "sec" -> WireChartFamilyId.Sec
        "tac" -> WireChartFamilyId.Tac
        "enr-l" -> WireChartFamilyId.EnrL
        "enr-h" -> WireChartFamilyId.EnrH
        "shaded-relief" -> WireChartFamilyId.ShadedRelief
        "world-basemap" -> WireChartFamilyId.WorldBasemap
        else -> error("Unsupported family: $family")
    },
    cycle = cycle,
)

private fun WirePackageId.toUi() = PackageId(
    region = region.toUiRegion(),
    family = when (family) {
        WireChartFamilyId.Sec -> "sec"
        WireChartFamilyId.Tac -> "tac"
        WireChartFamilyId.EnrL -> "enr-l"
        WireChartFamilyId.EnrH -> "enr-h"
        WireChartFamilyId.ShadedRelief -> "shaded-relief"
        WireChartFamilyId.WorldBasemap -> "world-basemap"
    },
    cycle = cycle,
)

private fun String.toWireRegion() = when (lowercase()) {
    "ne" -> WireRegionId.Ne
    "nc" -> WireRegionId.Nc
    "nw" -> WireRegionId.Nw
    "se" -> WireRegionId.Se
    "sc" -> WireRegionId.Sc
    "sw" -> WireRegionId.Sw
    "ec" -> WireRegionId.Ec
    "ak" -> WireRegionId.Ak
    "pac" -> WireRegionId.Pac
    "world" -> WireRegionId.World
    else -> error("Unsupported region: $this")
}

private fun WireRegionId.toUiRegion() = when (this) {
    WireRegionId.Ne -> "ne"
    WireRegionId.Nc -> "nc"
    WireRegionId.Nw -> "nw"
    WireRegionId.Se -> "se"
    WireRegionId.Sc -> "sc"
    WireRegionId.Sw -> "sw"
    WireRegionId.Ec -> "ec"
    WireRegionId.Ak -> "ak"
    WireRegionId.Pac -> "pac"
    WireRegionId.World -> "world"
}

private fun regionDisplayName(regionId: String) = when (regionId.lowercase()) {
    "ne" -> "Northeast"
    "nc" -> "North Central"
    "nw" -> "Northwest"
    "se" -> "Southeast"
    "sc" -> "South Central"
    "sw" -> "Southwest"
    "ec" -> "East Coast"
    "ak" -> "Alaska"
    "pac" -> "Pacific"
    else -> error("Unsupported region: $regionId")
}

private fun regionSortOrder(regionId: String) = when (regionId.lowercase()) {
    "ne" -> 0
    "nc" -> 1
    "nw" -> 2
    "se" -> 3
    "sc" -> 4
    "sw" -> 5
    "ec" -> 6
    "ak" -> 7
    "pac" -> 8
    else -> error("Unsupported region: $regionId")
}

private fun ContentAvailability.toWire() = when (this) {
    ContentAvailability.LocalOnly -> WireContentAvailability.LocalOnly
    ContentAvailability.RemoteOnly -> WireContentAvailability.RemoteOnly
    ContentAvailability.LocalAndRemote -> WireContentAvailability.LocalAndRemote
    ContentAvailability.Unavailable -> WireContentAvailability.Unavailable
}

private fun WireContentAvailability.toUi() = when (this) {
    WireContentAvailability.LocalOnly -> ContentAvailability.LocalOnly
    WireContentAvailability.RemoteOnly -> ContentAvailability.RemoteOnly
    WireContentAvailability.LocalAndRemote -> ContentAvailability.LocalAndRemote
    WireContentAvailability.Unavailable -> ContentAvailability.Unavailable
}

private fun WireContentPolicy.toUi() = when (this) {
    WireContentPolicy.OfflineRequired -> ContentPolicy.OfflineRequired
    WireContentPolicy.PreferLocal -> ContentPolicy.PreferLocal
    WireContentPolicy.StreamAllowed -> ContentPolicy.StreamAllowed
}

internal fun FlightPlan.toWireForTesting() = toWire()

internal fun ContentInventory.toWireForTesting() = toWire()

internal fun AppState.toWireForTesting() = toWire()

internal fun WireAppState.toUiForTesting() = toUi()

internal fun WireFlightPlan.toUiForTesting() = toUiFlightPlan()

internal fun WireContentInventory.toUiInventory() = ContentInventory(
    installedPackages = installed_packages.map {
        InstalledPackage(
            packageId = it.package_id.toUi(),
            integrityOk = it.integrity_ok,
        )
    },
)
