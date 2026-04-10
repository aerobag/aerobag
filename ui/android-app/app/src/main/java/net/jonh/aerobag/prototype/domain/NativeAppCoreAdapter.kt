package net.jonh.aerobag.prototype.domain

import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.Json

class NativeAppCoreAdapter(
    private val catalog: Catalog,
    private val bridge: NativeBridge = NativeBindings,
    private val json: Json = Json {
        encodeDefaults = true
        ignoreUnknownKeys = true
    },
) : AppCoreAdapter {
    fun removeFlightPlanLeg(plan: FlightPlan, index: Int): FlightPlan {
        val planJson = json.encodeToString(plan.toWire())
        val nextJson = bridge.removeFlightPlanLegJson(planJson, index)
        return json.decodeFromString<WireFlightPlan>(nextJson).toUiFlightPlan()
    }

    override fun replaceFlightPlan(state: AppState, catalog: Catalog, plan: FlightPlan): AppState {
        val stateJson = json.encodeToString(state.toWire())
        val catalogJson = json.encodeToString(catalog.toWire())
        val planJson = json.encodeToString(plan.toWire())
        val nextJson = bridge.replaceFlightPlanStateJson(stateJson, catalogJson, planJson)
        return json.decodeFromString<WireAppState>(nextJson).toUi()
    }

    override fun setContentPolicy(state: AppState, policy: ContentPolicy): AppState {
        val stateJson = json.encodeToString(state.toWire())
        val catalogJson = json.encodeToString(catalog.toWire())
        val policyJson = json.encodeToString(policy.toWire())
        val nextJson = bridge.setContentPolicyStateJson(stateJson, catalogJson, policyJson)
        return json.decodeFromString<WireAppState>(nextJson).toUi()
    }

    override fun refreshContent(state: AppState, inventory: ContentInventory): AppState {
        val stateJson = json.encodeToString(state.toWire())
        val catalogJson = json.encodeToString(catalog.toWire())
        val inventoryJson = json.encodeToString(inventory.toWire())
        val nextJson = bridge.refreshContentStateJson(stateJson, catalogJson, inventoryJson)
        return json.decodeFromString<WireAppState>(nextJson).toUi()
    }
}

private fun Catalog.toWire() = WireCatalog(
    schema_version = 1,
    cycle = cycle,
    catalog_revision = "2026-04-05T22:00:00Z",
    families = listOf(
        WireCatalogFamily(
            id = WireChartFamilyId.Sectional,
            display_name = "VFR Sectional Charts",
            kind = "tiled_raster",
            max_zoom = 10,
            tile_size = 512,
        ),
    ),
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
            family_id = WireChartFamilyId.Sectional,
            region_id = pkg.regionId.toWireRegion(),
            cycle = pkg.id.cycle,
            artifact_kind = "zip",
            relative_url = "/${pkg.id.cycle}/${pkg.packageName}.zip",
            manifest_name = pkg.packageName,
        )
    },
    plates = plates.map { plate ->
        WirePlateRecord(
            id = WirePlateId(
                airport_id = plate.airportId,
                procedure_code = "IAP-ILS-RWY-04R",
                page = 1,
                cycle = cycle,
            ),
            airport_id = plate.airportId,
            region_id = plate.regionId.toWireRegion(),
            cycle = cycle,
            procedure_code = "IAP-ILS-RWY-04R",
            display_name = "ILS OR LOC RWY 04R",
            kind = "approach",
            georeferenced = true,
            page_count = 1,
            asset_base_path = "plates/${plate.airportId}/IAP-ILS-RWY-04R",
        )
    },
)

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
    from = WireNavRef.Airport(fromAirport),
    to = WireNavRef.Airport(toAirport),
    airway = airway,
)

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

private fun WirePlanLeg.toUi() = FlightPlanLeg(
    fromAirport = (from as WireNavRef.Airport).code,
    toAirport = (to as WireNavRef.Airport).code,
    airway = airway,
)

private fun PackageId.toWire() = WirePackageId(
    region = region.toWireRegion(),
    family = WireChartFamilyId.Sectional,
    cycle = cycle,
)

private fun WirePackageId.toUi() = PackageId(
    region = region.toUiRegion(),
    family = "sectional",
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

internal fun Catalog.toWireForTesting() = toWire()

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
