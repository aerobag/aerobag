package net.jonh.aerobag.prototype.domain

import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.Json
import android.util.Log

data class VectorTileRequest(
    val layer: String,
    val z: Int,
    val x: Int,
    val y: Int,
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
    @kotlinx.serialization.SerialName("longest_runway_length_ft")
    val longestRunwayLengthFt: Double? = null,
    @kotlinx.serialization.SerialName("longest_runway_heading_true_deg")
    val longestRunwayHeadingTrueDeg: Double? = null,
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
    val screenX: Double,
    val screenY: Double,
    val towered: Boolean,
    val fuelAvailable: Boolean,
    val runwayLengthRatio: Double,
    val longestRunwayHeadingTrueDeg: Double?,
)

data class MapOverlayWarning(
    val code: String,
    val message: String,
)

data class MapOverlayQueryResult(
    val neededPointTiles: List<VectorTileRequest>,
    val visibleFeatures: List<VisibleMapFeature>,
    val warnings: List<MapOverlayWarning>,
)

class NativeAppCoreAdapter(
    private val catalogJson: String,
    private val chartCatalogJson: String,
    private val bridge: NativeBridge = NativeBindings,
    private val json: Json = Json {
        encodeDefaults = true
        ignoreUnknownKeys = true
    },
) : AppCoreAdapter {
    fun createUiSession(
        plan: FlightPlan,
        recentAirportIds: List<String>,
        selectedAirportId: String?,
        selectedChartId: String?,
    ): NativeUiSession {
        val resultJson = bridge.createUiSessionJson(
            catalogJson,
            chartCatalogJson,
            json.encodeToString(plan.toWire()),
            json.encodeToString(recentAirportIds),
            json.encodeToString(selectedAirportId),
            json.encodeToString(selectedChartId),
        )
        val result = json.decodeFromString<WireUiSessionInitResult>(resultJson)
        return NativeUiSession(
            handle = result.handle,
            bridge = bridge,
            json = json,
            chartCatalog = result.chart_catalog.toUi(),
            initialSnapshot = result.snapshot.toUi(),
        )
    }

    fun deriveChartPageState(
        resourceIndexJson: String,
        plan: FlightPlan,
        recentAirportIds: List<String>,
        selectedAirportId: String?,
        selectedChartId: String?,
    ): DerivedChartPageState {
        val planJson = json.encodeToString(plan.toWire())
        val nextJson = bridge.deriveChartPageStateJson(
            resourceIndexJson,
            planJson,
            json.encodeToString(recentAirportIds),
            json.encodeToString(selectedAirportId),
            json.encodeToString(selectedChartId),
        )
        return json.decodeFromString<WireDerivedChartPageState>(nextJson).toUi()
    }

    fun deriveChartPage(resourceIndexJson: String, plan: FlightPlan): ChartPageFixture {
        val planJson = json.encodeToString(plan.toWire())
        val nextJson = bridge.deriveChartPageJson(resourceIndexJson, planJson)
        return json.decodeFromString<WireDerivedChartPage>(nextJson).toUi()
    }

    fun removeFlightPlanLeg(plan: FlightPlan, index: Int): FlightPlan {
        val planJson = json.encodeToString(plan.toWire())
        val nextJson = bridge.removeFlightPlanLegJson(planJson, index)
        return json.decodeFromString<WireFlightPlan>(nextJson).toUiFlightPlan()
    }

    override fun replaceFlightPlan(state: AppState, catalog: Catalog, plan: FlightPlan): AppState {
        val stateJson = json.encodeToString(state.toWire())
        val planJson = json.encodeToString(plan.toWire())
        val nextJson = bridge.replaceFlightPlanStateJson(stateJson, catalogJson, planJson)
        return json.decodeFromString<WireAppState>(nextJson).toUi()
    }

    override fun setContentPolicy(state: AppState, policy: ContentPolicy): AppState {
        val stateJson = json.encodeToString(state.toWire())
        val policyJson = json.encodeToString(policy.toWire())
        val nextJson = bridge.setContentPolicyStateJson(stateJson, catalogJson, policyJson)
        return json.decodeFromString<WireAppState>(nextJson).toUi()
    }

    override fun refreshContent(state: AppState, inventory: ContentInventory): AppState {
        val stateJson = json.encodeToString(state.toWire())
        val inventoryJson = json.encodeToString(inventory.toWire())
        val nextJson = bridge.refreshContentStateJson(stateJson, catalogJson, inventoryJson)
        return json.decodeFromString<WireAppState>(nextJson).toUi()
    }

    fun suggestAirwaysNear(dbPath: String, anchor: NavRef, limit: Int = 5): List<AirwaySuggestion> {
        val nextJson = bridge.suggestAirwaysNearJson(dbPath, json.encodeToString(anchor.toWire()), limit)
        return json.decodeFromString<List<WireAirwaySuggestion>>(nextJson).map { it.toUi() }
    }

    fun resolveNavRefPosition(dbPath: String, navRef: NavRef): LatLonPoint {
        val nextJson = bridge.resolveNavRefPositionJson(dbPath, json.encodeToString(navRef.toWire()))
        return json.decodeFromString<WireLatLon>(nextJson).toUi()
    }

    fun resolveNavRefIdentifier(dbPath: String, identifier: String): NavRef {
        val nextJson = bridge.resolveNavRefIdentifierJson(dbPath, identifier)
        return json.decodeFromString<WireNavRef>(nextJson).toUi()
    }

    fun suggestWaypointIdentifiers(
        dbPath: String,
        plan: FlightPlan,
        componentIndex: Int,
        before: Boolean,
        prefix: String,
        limit: Int = 8,
    ): List<WaypointIdentifierSuggestion> {
        val nextJson =
            bridge.suggestWaypointIdentifiersJson(
                dbPath,
                json.encodeToString(plan.toWire()),
                componentIndex,
                before,
                prefix,
                limit,
            )
        return json.decodeFromString<List<WireWaypointIdentifierSuggestion>>(nextJson).map { it.toUi() }
    }

    fun resolveNavRefPosition(dbPath: String, navRef: NavRef, procedureAirportId: String?): LatLonPoint {
        val nextJson =
            bridge.resolveNavRefPositionWithAirportJson(
                dbPath,
                json.encodeToString(navRef.toWire()),
                json.encodeToString(procedureAirportId),
            )
        return json.decodeFromString<WireLatLon>(nextJson).toUi()
    }

    fun projectFlightPlanRoute(dbPath: String, plan: FlightPlan): List<FlightPlanRouteSegment> {
        val nextJson = bridge.projectFlightPlanRouteJson(dbPath, json.encodeToString(plan.toWire()))
        return json.decodeFromString<List<WireFlightPlanRouteSegment>>(nextJson).map { it.toUi() }
    }

    fun loadAirwayBranches(dbPath: String, airwayName: String): List<AirwayBranch> {
        val nextJson = bridge.loadAirwayBranchesJson(dbPath, airwayName)
        return json.decodeFromString<List<WireAirwayBranch>>(nextJson).map { it.toUi() }
    }

    fun listAirwayEntryCandidates(dbPath: String, airwayName: String, originAnchor: NavRef): List<AirwayEntryCandidate> {
        val nextJson = bridge.listAirwayEntryCandidatesJson(dbPath, airwayName, json.encodeToString(originAnchor.toWire()))
        return json.decodeFromString<List<WireAirwayEntryCandidate>>(nextJson).map { it.toUi() }
    }

    fun listAirwayExitCandidates(
        dbPath: String,
        airwayName: String,
        entry: AirwayEntryCandidate,
        destinationAnchor: NavRef,
    ): List<AirwayExitCandidate> {
        val nextJson =
            bridge.listAirwayExitCandidatesJson(
                dbPath,
                airwayName,
                json.encodeToString(entry.toWire()),
                json.encodeToString(destinationAnchor.toWire()),
            )
        return json.decodeFromString<List<WireAirwayExitCandidate>>(nextJson).map { it.toUi() }
    }

    fun listProcedures(dbPath: String, airportId: String, kind: ProcedureKind): List<ProcedureSummary> {
        val nextJson = bridge.listProceduresJson(dbPath, airportId, json.encodeToString(kind.toWire()))
        return json.decodeFromString<List<WireProcedureSummary>>(nextJson).map { it.toUi() }
    }

    fun describeProcedureOptions(dbPath: String, airportId: String, procedureId: String, kind: ProcedureKind): ProcedureOptions {
        val nextJson = bridge.describeProcedureOptionsJson(dbPath, airportId, procedureId, json.encodeToString(kind.toWire()))
        return runCatching {
            json.decodeFromString<WireProcedureOptions>(nextJson).toUi()
        }.getOrElse { error ->
            Log.e("AerobagProcedure", "describeProcedureOptions decode failed airport=$airportId procedure=$procedureId json=$nextJson", error)
            throw error
        }
    }

    fun materializeProcedureSelection(
        dbPath: String,
        airportId: String,
        procedureId: String,
        kind: ProcedureKind,
        runwayTransition: String?,
        enrouteTransition: String?,
        componentIndex: Int,
    ): MaterializedProcedure {
        val nextJson =
            bridge.materializeProcedureSelectionJson(
                dbPath,
                airportId,
                procedureId,
                json.encodeToString(kind.toWire()),
                json.encodeToString(runwayTransition),
                json.encodeToString(enrouteTransition),
                componentIndex,
            )
        return runCatching {
            json.decodeFromString<WireMaterializedProcedure>(nextJson).toUi()
        }.getOrElse { error ->
            Log.e(
                "AerobagProcedure",
                "materializeProcedureSelection decode failed airport=$airportId procedure=$procedureId runway=$runwayTransition enroute=$enrouteTransition json=$nextJson",
                error,
            )
            throw error
        }
    }

    fun buildFlightPlanUi(plan: FlightPlan): FlightPlanUiState {
        val nextJson = bridge.buildFlightPlanUiJson(json.encodeToString(plan.toWire()))
        return json.decodeFromString<WireFlightPlanUiState>(nextJson).toUi()
    }

    fun activateLegUi(plan: FlightPlan, legIndex: Int): FlightPlanUiMutation {
        val nextJson = bridge.activateLegUiJson(json.encodeToString(plan.toWire()), legIndex)
        return json.decodeFromString<WireFlightPlanUiMutation>(nextJson).toUi()
    }

    fun activateNextLegUi(plan: FlightPlan): FlightPlanUiMutation {
        val nextJson = bridge.activateNextLegUiJson(json.encodeToString(plan.toWire()))
        return json.decodeFromString<WireFlightPlanUiMutation>(nextJson).toUi()
    }

    fun deleteComponentUi(plan: FlightPlan, componentIndex: Int): FlightPlanUiMutation {
        val nextJson = bridge.deleteComponentUiJson(json.encodeToString(plan.toWire()), componentIndex)
        return json.decodeFromString<WireFlightPlanUiMutation>(nextJson).toUi()
    }

    fun moveComponentUi(plan: FlightPlan, componentIndex: Int, delta: Int): FlightPlanUiMutation {
        val nextJson = bridge.moveComponentUiJson(json.encodeToString(plan.toWire()), componentIndex, delta)
        return json.decodeFromString<WireFlightPlanUiMutation>(nextJson).toUi()
    }

    fun insertWaypointUi(plan: FlightPlan, componentIndex: Int, before: Boolean, waypoint: NavRef): FlightPlanUiMutation {
        val nextJson = bridge.insertWaypointUiJson(json.encodeToString(plan.toWire()), componentIndex, before, json.encodeToString(waypoint.toWire()))
        return json.decodeFromString<WireFlightPlanUiMutation>(nextJson).toUi()
    }

    fun suspendSequencingUi(plan: FlightPlan): FlightPlanUiMutation {
        val nextJson = bridge.suspendSequencingUiJson(json.encodeToString(plan.toWire()))
        return json.decodeFromString<WireFlightPlanUiMutation>(nextJson).toUi()
    }

    fun unsuspendSequencingUi(plan: FlightPlan): FlightPlanUiMutation {
        val nextJson = bridge.unsuspendSequencingUiJson(json.encodeToString(plan.toWire()))
        return json.decodeFromString<WireFlightPlanUiMutation>(nextJson).toUi()
    }

    fun sequenceActiveLegUi(plan: FlightPlan): FlightPlanUiMutation {
        val nextJson = bridge.sequenceActiveLegUiJson(json.encodeToString(plan.toWire()))
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

    fun insertAirwayFromSelectionUi(
        dbPath: String,
        plan: FlightPlan,
        startComponentIndex: Int,
        endComponentIndex: Int,
        entry: AirwayEntryCandidate,
        exit: AirwayExitCandidate,
    ): FlightPlanUiMutation {
        val nextJson =
            bridge.insertAirwayFromSelectionUiJson(
                dbPath,
                json.encodeToString(plan.toWire()),
                startComponentIndex,
                endComponentIndex,
                json.encodeToString(entry.toWire()),
                json.encodeToString(exit.toWire()),
            )
        return json.decodeFromString<WireFlightPlanUiMutation>(nextJson).toUi()
    }

    fun replaceAirwayFromSelectionUi(
        dbPath: String,
        plan: FlightPlan,
        componentIndex: Int,
        entry: AirwayEntryCandidate,
        exit: AirwayExitCandidate,
    ): FlightPlanUiMutation {
        val nextJson =
            bridge.replaceAirwayFromSelectionUiJson(
                dbPath,
                json.encodeToString(plan.toWire()),
                componentIndex,
                json.encodeToString(entry.toWire()),
                json.encodeToString(exit.toWire()),
            )
        return json.decodeFromString<WireFlightPlanUiMutation>(nextJson).toUi()
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
    val chartCatalog: ChartPageFixture,
    initialSnapshot: UiSessionSnapshot,
) {
    var snapshot: UiSessionSnapshot = initialSnapshot
        private set

    fun removeLeg(index: Int): UiSessionSnapshot {
        snapshot = decodeSnapshot(bridge.removeLegInSessionJson(handle, index))
        return snapshot
    }

    fun moveWaypoint(index: Int, delta: Int): UiSessionSnapshot {
        snapshot = decodeSnapshot(bridge.moveWaypointInSessionJson(handle, index, delta))
        return snapshot
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

    fun refreshSnapshot(): UiSessionSnapshot {
        snapshot = decodeSnapshot(bridge.getSessionSnapshotJson(handle))
        return snapshot
    }

    fun replaceFlightPlan(plan: FlightPlan): UiSessionSnapshot {
        snapshot = decodeSnapshot(bridge.replaceFlightPlanInSessionJson(handle, json.encodeToString(plan.toWire())))
        return snapshot
    }

    fun setGuidanceLegGeometry(geometries: List<GuidanceLegGeometry>): UiSessionSnapshot {
        snapshot =
            decodeSnapshot(
                bridge.setGuidanceLegGeometryInSessionJson(
                    handle,
                    json.encodeToString(geometries.map { it.toWire() }),
                ),
            )
        return snapshot
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

    fun queryMapOverlay(viewport: MapViewportState, widthPx: Double, heightPx: Double): MapOverlayQueryResult {
        val viewportJson = json.encodeToString(viewport.toWire())
        val resultJson = bridge.getMapOverlayInSessionJson(handle, viewportJson, widthPx, heightPx)
        return json.decodeFromString<WireMapOverlayQueryResult>(resultJson).toUi()
    }

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

private fun NavRef.toWire(): WireNavRef = when (this) {
    is NavRef.Airport -> WireNavRef.Airport(code)
    is NavRef.Navaid -> WireNavRef.Navaid(code)
    is NavRef.Fix -> WireNavRef.Fix(code)
    is NavRef.LatLon -> WireNavRef.LatLon(WireLatLon(lat, lon))
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
    last_content_requirements = lastContentRequirements.map { requirement ->
        WireContentRequirement(
            package_ids = requirement.packageIds.map { it.toWire() },
        )
    },
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
    sources = sources.map { it.toWire() },
)

private fun OwnshipSourceMenuItem.toWire() = WireOwnshipSourceMenuItem(
    source_id = sourceId,
    label = label,
    enabled = enabled,
    active = active,
    status_label = statusLabel,
)

private fun WireAppState.toUi() = AppState(
    activePlan = active_plan?.toUiFlightPlan(),
    contentPolicy = content_policy.toUi(),
    lastContentRequirements = last_content_requirements.map { requirement ->
        ContentRequirement(
            packageIds = requirement.package_ids.map { it.toUi() },
        )
    },
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
    lastContentRequirements = last_content_requirements.map { requirement ->
        ContentRequirement(
            packageIds = requirement.package_ids.map { it.toUi() },
        )
    },
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
    lastContentRequirements = last_content_requirements.map { requirement ->
        ContentRequirement(
            packageIds = requirement.package_ids.map { it.toUi() },
        )
    },
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
    sources = sources.map { it.toUi() },
)

private fun WireOwnshipUiState.toUi() = OwnshipUiState(
    render = render.toUi(),
    controls = controls.toUi(),
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
    gapSpans = gap_spans.map { PlaybackGapSpan(it.start_seconds, it.end_seconds, it.start_ratio, it.end_ratio) },
)

private fun WireOwnshipSourceMenuItem.toUi() = OwnshipSourceMenuItem(
    sourceId = source_id,
    label = label,
    enabled = enabled,
    active = active,
    statusLabel = status_label,
)

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

private fun WireOwnshipSourceKind.toUi(): OwnshipSourceKind = when (this) {
    WireOwnshipSourceKind.DeviceGps -> OwnshipSourceKind.DeviceGps
    WireOwnshipSourceKind.ExternalGps -> OwnshipSourceKind.ExternalGps
    WireOwnshipSourceKind.ExternalAhrs -> OwnshipSourceKind.ExternalAhrs
    WireOwnshipSourceKind.GpxPlayback -> OwnshipSourceKind.GpxPlayback
    WireOwnshipSourceKind.AdsbTrackPlayback -> OwnshipSourceKind.AdsbTrackPlayback
    WireOwnshipSourceKind.LiveNetworkTrack -> OwnshipSourceKind.LiveNetworkTrack
    WireOwnshipSourceKind.FlightPlanSimulator -> OwnshipSourceKind.FlightPlanSimulator
}

private fun OwnshipSourceKind.toWire(): WireOwnshipSourceKind = when (this) {
    OwnshipSourceKind.DeviceGps -> WireOwnshipSourceKind.DeviceGps
    OwnshipSourceKind.ExternalGps -> WireOwnshipSourceKind.ExternalGps
    OwnshipSourceKind.ExternalAhrs -> WireOwnshipSourceKind.ExternalAhrs
    OwnshipSourceKind.GpxPlayback -> WireOwnshipSourceKind.GpxPlayback
    OwnshipSourceKind.AdsbTrackPlayback -> WireOwnshipSourceKind.AdsbTrackPlayback
    OwnshipSourceKind.LiveNetworkTrack -> WireOwnshipSourceKind.LiveNetworkTrack
    OwnshipSourceKind.FlightPlanSimulator -> WireOwnshipSourceKind.FlightPlanSimulator
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
private data class WireUiSessionSnapshot(
    val app_state: WireUiSnapshotAppState,
    val app_ui_state: WireAppUiState = WireAppUiState(),
    val playback_ui_state: WirePlaybackUiState = WirePlaybackUiState(),
    val map_follow_ui_state: WireMapFollowUiState = WireMapFollowUiState(),
    val map_follow_target_viewport: WireMapViewport? = null,
    val chart_page_state: WireUiChartPageState,
)

@kotlinx.serialization.Serializable
private data class WireUiSessionInitResult(
    val handle: Long,
    val chart_catalog: WireDerivedChartPage,
    val snapshot: WireUiSessionSnapshot,
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
    val asset_url: String,
    val thumbnail_source_path: String? = null,
    val thumbnail_path: String? = null,
    val thumbnail_url: String? = null,
)

private fun WireDerivedChartPage.toUi() = ChartPageFixture(
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
)

data class UiChartPageState(
    val orderedAirportIds: List<String>,
    val recentAirportIds: List<String>,
    val selectedAirportId: String,
    val selectedChartId: String,
)

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

private fun WireUiSessionSnapshot.toUi() = UiSessionSnapshot(
    appState = app_state.toUi(),
    appUiState = app_ui_state.toUi(),
    playbackUiState = playback_ui_state.toUi(),
    mapFollowUiState = map_follow_ui_state.toUi(),
    mapFollowTargetViewport = map_follow_target_viewport?.toUi(),
    chartPageState = chart_page_state.toUi(),
)

private fun WireDerivedChartAirport.toUi() = ChartAirport(
    id = id,
    label = label,
    charts = charts.map { it.toUi() },
)

private fun WireDerivedChartAsset.toUi() = ChartAsset(
    id = id,
    airportId = airport_id,
    packageId = package_id,
    label = label,
    kind = kind,
    folderCategory = folder_category,
    sourceAssetPath = source_asset_path,
    assetPath = asset_path,
    assetUrl = asset_url,
    thumbnailSourceAssetPath = thumbnail_source_path,
    thumbnailAssetPath = thumbnail_path,
    thumbnailUrl = thumbnail_url,
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
    longest_runway_heading_true_deg = longestRunwayHeadingTrueDeg,
)

private fun WireMapOverlayQueryResult.toUi() = MapOverlayQueryResult(
    neededPointTiles = needed_point_tiles.map { it.toUi() },
    visibleFeatures = visible_features.map { it.toUi() },
    warnings = warnings.map { it.toUi() },
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
    screenX = screen_x,
    screenY = screen_y,
    towered = towered,
    fuelAvailable = fuel_available,
    runwayLengthRatio = runway_length_ratio,
    longestRunwayHeadingTrueDeg = longest_runway_heading_true_deg,
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

private fun WireAirwayPresentationPoint.toUi() = AirwayPresentationPoint(
    branchPointIndex = branch_point_index,
    sequence = sequence,
    navRef = nav_ref.toUi(),
)

private fun LatLonPoint.toWire() = WireLatLon(lat = lat, lon = lon)

private fun WireLatLon.toUi() = LatLonPoint(lat = lat, lon = lon)

private fun GuidanceLegGeometry.toWire() = WireGuidanceLegGeometry(
    legId = legId,
    from = from.toWire(),
    to = to.toWire(),
)

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
    display_split_leg_id = displaySplitLegId,
    sequencing_mode = sequencingMode.toWire(),
    direct_to = directTo?.toWire(),
    suspend_reason = suspendReason?.toWire(),
)

private fun WireGuidanceState.toUi() = GuidanceState(
    activeLegIndex = active_leg_index,
    displaySplitLegId = display_split_leg_id,
    sequencingMode = sequencing_mode.toUi(),
    directTo = direct_to?.toUi(),
    suspendReason = suspend_reason?.toUi(),
)

private fun DirectToState.toWire() = WireDirectToState(
    start = start.toWire(),
    target = target.toWire(),
    target_leg_id = targetLegId,
    resume_leg_id = resumeLegId,
)

private fun WireDirectToState.toUi() = DirectToState(
    start = start.toUi(),
    target = target.toUi(),
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
    componentIndex = component_index,
    kind = kind.toUi(),
    summary = summary,
    items = items.map { it.toUi() },
    active = active,
    canAddAirwayAfter = can_add_airway_after,
    canAddProcedureBefore = can_add_procedure_before,
    canChangeAirway = can_change_airway,
    canRemove = can_remove,
    canReorder = can_reorder,
    canReorderUp = can_reorder_up,
    canReorderDown = can_reorder_down,
    precedingWaypoint = preceding_waypoint?.toUi(),
    followingWaypoint = following_waypoint?.toUi(),
)

private fun WireRouteComponentViewKind.toUi() = when (this) {
    WireRouteComponentViewKind.Waypoint -> RouteComponentViewKind.Waypoint
    WireRouteComponentViewKind.Airway -> RouteComponentViewKind.Airway
    WireRouteComponentViewKind.Procedure -> RouteComponentViewKind.Procedure
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

private fun WireDirectToUiView.toUi() = DirectToUiView(
    start = start.toUi(),
    target = target.toUi(),
    targetLegId = target_leg_id,
    resumeLegId = resume_leg_id,
    onPlanTarget = on_plan_target,
)

private fun WireGuidanceUiView.toUi() = GuidanceUiView(
    sequencingMode = sequencing_mode.toUi(),
    activeLegIndex = active_leg_index,
    displaySplitLegIndex = display_split_leg_index,
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

private fun WireNavElementUiView.toUi() = NavElementUiView(
    activeLegSummary = active_leg_summary,
    cdiIndicatorDots = cdi_indicator_dots,
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

private fun WireFlightPlanRouteSegment.toUi() = FlightPlanRouteSegment(
    id = id,
    from = from.toUi(),
    to = to.toUi(),
    status = status.toUi(),
)

private fun WireRouteSegmentStatus.toUi() = when (this) {
    WireRouteSegmentStatus.Completed -> RouteSegmentStatus.Completed
    WireRouteSegmentStatus.Active -> RouteSegmentStatus.Active
    WireRouteSegmentStatus.Remaining -> RouteSegmentStatus.Remaining
}

private fun WireFlightPlanUiState.toUi() = FlightPlanUiState(
    components = components.map { it.toUi() },
    resolvedLegs = resolved_legs.map { it.toUi() },
    displayRows = display_rows.map { it.toUi() },
    guidance = guidance?.toUi(),
)

private fun WireFlightPlanDisplayRowUiView.toUi() = FlightPlanDisplayRowUiView(
    label = label,
    rowKind = row_kind.toUi(),
    componentKind = component_kind?.toUi(),
    componentIndex = component_index,
    legIndex = leg_index,
    chartAirportId = chart_airport_id,
    navRef = nav_ref?.toUi(),
    depth = depth,
    active = active,
    canAddAirwayAfter = can_add_airway_after,
    canAddProcedureBefore = can_add_procedure_before,
    canChangeAirway = can_change_airway,
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
    actions = actions.map { it.toUi() },
)

private fun WireFlightPlanDisplayRowKind.toUi() = when (this) {
    WireFlightPlanDisplayRowKind.Waypoint -> FlightPlanDisplayRowKind.Waypoint
    WireFlightPlanDisplayRowKind.Group -> FlightPlanDisplayRowKind.Group
    WireFlightPlanDisplayRowKind.Discontinuity -> FlightPlanDisplayRowKind.Discontinuity
}

private fun WireFlightPlanRowActionUiView.toUi() = FlightPlanRowActionUiView(
    id = id.toUi(),
    enabled = enabled,
)

private fun WireFlightPlanRowActionId.toUi() = when (this) {
    WireFlightPlanRowActionId.ActivateLeg -> FlightPlanRowActionId.ActivateLeg
    WireFlightPlanRowActionId.Remove -> FlightPlanRowActionId.Remove
    WireFlightPlanRowActionId.InsertBefore -> FlightPlanRowActionId.InsertBefore
    WireFlightPlanRowActionId.InsertAfter -> FlightPlanRowActionId.InsertAfter
    WireFlightPlanRowActionId.Reorder -> FlightPlanRowActionId.Reorder
    WireFlightPlanRowActionId.WaypointInfo -> FlightPlanRowActionId.WaypointInfo
    WireFlightPlanRowActionId.AddAirway -> FlightPlanRowActionId.AddAirway
    WireFlightPlanRowActionId.SelectProcedure -> FlightPlanRowActionId.SelectProcedure
    WireFlightPlanRowActionId.Plates -> FlightPlanRowActionId.Plates
    WireFlightPlanRowActionId.ChangeAirway -> FlightPlanRowActionId.ChangeAirway
    WireFlightPlanRowActionId.RemoveAirway -> FlightPlanRowActionId.RemoveAirway
    WireFlightPlanRowActionId.RemoveProcedure -> FlightPlanRowActionId.RemoveProcedure
}

private fun WireFlightPlanUiMutation.toUi() = FlightPlanUiMutation(
    plan = plan.toUiFlightPlan(),
    uiState = ui_state.toUi(),
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
    is WireNavRef.Fix -> NavRef.Fix(code)
    is WireNavRef.LatLon -> NavRef.LatLon(value.lat, value.lon)
}

private fun PackageId.toWire() = WirePackageId(
    region = region.toWireRegion(),
    family = when (family) {
        "sec" -> WireChartFamilyId.Sec
        "tac" -> WireChartFamilyId.Tac
        "enr-l" -> WireChartFamilyId.EnrL
        "enr-h" -> WireChartFamilyId.EnrH
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

internal fun Catalog.toWireForTesting() = WireCatalog(
    schema_version = 1,
    cycle = cycle,
    catalog_revision = "test",
    families = packages.map { it.id.family }.distinct().map { family ->
        WireCatalogFamily(
            id = when (family) {
                "sec" -> WireChartFamilyId.Sec
                "tac" -> WireChartFamilyId.Tac
                "enr-l" -> WireChartFamilyId.EnrL
                "enr-h" -> WireChartFamilyId.EnrH
                else -> error("Unsupported family: $family")
            },
            display_name = family,
            kind = "tiled_raster",
        )
    },
    regions = packages
        .map { it.regionId.lowercase() }
        .distinct()
        .sortedBy { regionSortOrder(it) }
        .map { regionId ->
            WireCatalogRegion(
                id = regionId.toWireRegion(),
                display_name = regionDisplayName(regionId),
                sort_order = regionSortOrder(regionId),
            )
        },
    packages = packages.map { pkg ->
        WireCatalogPackage(
            id = pkg.id.toWire(),
            package_name = pkg.packageName,
            family_id = pkg.id.toWire().family,
            region_id = pkg.regionId.toWireRegion(),
            cycle = pkg.id.cycle,
            artifact_kind = "zip",
            relative_url = pkg.packageName,
            manifest_name = pkg.packageName,
        )
    },
    charts = emptyList(),
    plates = plates.mapIndexed { index, plate ->
        WirePlateRecord(
            id = WirePlateId(
                airport_id = plate.airportId,
                procedure_code = "plate-$index",
                page = 1,
                cycle = cycle,
            ),
            airport_id = plate.airportId,
            region_id = plate.regionId.toWireRegion(),
            cycle = cycle,
            procedure_code = "plate-$index",
            display_name = "plate-$index",
            kind = "approach",
            georeferenced = true,
            page_count = 1,
            asset_base_path = "plates/${plate.airportId}/plate-$index",
        )
    },
    supplements = emptyList(),
)

internal fun FlightPlan.toWireForTesting() = toWire()

internal fun ContentInventory.toWireForTesting() = toWire()

internal fun AppState.toWireForTesting() = toWire()

internal fun WireAppState.toUiForTesting() = toUi()

internal fun WireCatalog.toUiCatalog() = Catalog(
    cycle = cycle,
    packages = packages.map { pkg ->
        CatalogPackage(
            id = pkg.id.toUi(),
            packageName = pkg.package_name,
            regionId = pkg.region_id.toUiRegion(),
        )
    },
    plates = plates.map { plate ->
        PlateRecord(
            airportId = plate.airport_id,
            regionId = plate.region_id.toUiRegion(),
        )
    },
)

internal fun WireFlightPlan.toUiForTesting() = toUiFlightPlan()

internal fun WireContentInventory.toUiInventory() = ContentInventory(
    installedPackages = installed_packages.map {
        InstalledPackage(
            packageId = it.package_id.toUi(),
            integrityOk = it.integrity_ok,
        )
    },
)

internal fun WireCatalog.toUiForTesting() = toUiCatalog()
