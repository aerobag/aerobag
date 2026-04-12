package net.jonh.aerobag.prototype.domain

import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.Json

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

    fun setSituation(situation: Situation): UiSessionSnapshot {
        snapshot = decodeSnapshot(bridge.setSituationInSessionJson(handle, json.encodeToString(situation.toWire())))
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

    fun destroy() {
        bridge.destroySession(handle)
    }

    private fun decodeSnapshot(snapshotJson: String): UiSessionSnapshot =
        json.decodeFromString<WireUiSessionSnapshot>(snapshotJson).toUi()
}

private fun FlightPlan.toWire() = WireFlightPlan(
    id = id,
    name = name,
    legs = legs.map { it.toWire() },
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
    situation = situation.toWire(),
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

private fun WireAppState.toUi() = AppState(
    activePlan = active_plan?.toUiFlightPlan(),
    situation = situation.toUi(),
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

private fun Situation.toWire() = WireSituation(
    position = position.toWire(),
    orientation_deg = orientationDeg,
    speed_kt = speedKt,
)

private fun WireSituation.toUi() = Situation(
    position = position.toUi(),
    orientationDeg = orientation_deg,
    speedKt = speed_kt,
)

private fun SituationPosition.toWire(): WireSituationPosition = when (this) {
    SituationPosition.Unknown -> WireSituationPosition.Unknown
    is SituationPosition.LatLon -> WireSituationPosition.LatLon(lat = lat, lon = lon)
    is SituationPosition.FlightPlanLocation -> WireSituationPosition.FlightPlanLocation(leg_index = legIndex, lat = lat, lon = lon)
}

private fun WireSituationPosition.toUi(): SituationPosition = when (this) {
    WireSituationPosition.Unknown -> SituationPosition.Unknown
    is WireSituationPosition.LatLon -> SituationPosition.LatLon(lat = lat, lon = lon)
    is WireSituationPosition.FlightPlanLocation -> SituationPosition.FlightPlanLocation(legIndex = leg_index, lat = lat, lon = lon)
}

internal fun WireFlightPlan.toUiFlightPlan() = FlightPlan(
    id = id,
    name = name,
    legs = legs.map { it.toUi() },
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
    val app_state: WireAppState,
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
    val appState: AppState,
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
