package net.jonh.aerobag.prototype.domain

import kotlinx.serialization.Serializable

@Serializable
data class WireResourceIndex(
    val schema_version: Int,
    val cycle: String? = null,
    val generated_at_utc: String,
    val families: List<WireResourceFamily>,
    val regions: List<WireCatalogRegion>,
    val packages: List<WireResourcePackage>,
    val chart_collections: List<WireChartCollection>,
    val airports: List<WireResourceAirport>,
    val airport_resources: List<WireAirportResources>,
    val plates: List<WireResourcePlate>,
    val csups: List<WireResourceCsup>,
)

@Serializable
data class WireResourceFamily(
    val id: String,
    val display_name: String,
    val kind: String,
)

@Serializable
data class WireResourcePackage(
    val id: String,
    val family_id: String,
    val region_id: WireRegionId,
    val artifact_path: String,
    val size_bytes: Long,
    val checksum_sha256: String,
)

@Serializable
data class WireChartCollection(
    val id: String,
    val family_id: WireChartFamilyId,
    val region_id: WireRegionId,
    val package_id: String,
    val chart_index: Int,
    val tile_path_template: String,
    val levels: List<WireChartCollectionLevel>,
    val coverage_bounds: WireCoverageBounds,
    val default_view: WireDefaultView,
)

@Serializable
data class WireChartCollectionLevel(
    val zoom: Int,
    val x_min: Int,
    val x_max: Int,
    val y_tms_min: Int,
    val y_tms_max: Int,
)

@Serializable
data class WireCoverageBounds(
    val lat_min: Double,
    val lat_max: Double,
    val lon_min: Double,
    val lon_max: Double,
)

@Serializable
data class WireDefaultView(
    val lat: Double,
    val lon: Double,
    val zoom: Double,
)

@Serializable
data class WireResourceAirport(
    val id: String,
    val facility_name: String,
    val lat: Double,
    val lon: Double,
    val airport_type: String,
)

@Serializable
data class WireAirportResources(
    val airport_id: String,
    val plate_ids: List<String> = emptyList(),
    val csup_ids: List<String> = emptyList(),
    val package_ids: List<String> = emptyList(),
)

@Serializable
data class WireResourcePlate(
    val id: String,
    val airport_id: String,
    val region_id: WireRegionId,
    val package_id: String,
    val asset_path: String,
    val label: String,
    val asset_kind: String,
)

@Serializable
data class WireResourceCsup(
    val id: String,
    val airport_id: String,
    val region_id: WireRegionId,
    val package_id: String,
    val asset_path: String,
    val label: String,
    val asset_kind: String,
)

private fun WireChartFamilyId.toUiMapFamily() = when (this) {
    WireChartFamilyId.Sectional -> MapChartFamily.Sectional
    WireChartFamilyId.Tac -> MapChartFamily.Tac
    WireChartFamilyId.IfrLow -> MapChartFamily.IfrLow
    WireChartFamilyId.IfrHigh -> MapChartFamily.IfrHigh
}

private fun WireChartFamilyId.toResourceId() = when (this) {
    WireChartFamilyId.Sectional -> "sectional"
    WireChartFamilyId.Tac -> "tac"
    WireChartFamilyId.IfrLow -> "ifr_low"
    WireChartFamilyId.IfrHigh -> "ifr_high"
}

private fun WireRegionId.toCode() = when (this) {
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

private fun regionDisplayName(regions: List<WireCatalogRegion>, regionId: WireRegionId): String =
    regions.firstOrNull { it.id == regionId }?.display_name ?: regionId.toCode().uppercase()

private fun familyDisplayName(families: List<WireResourceFamily>, familyId: String): String =
    families.firstOrNull { it.id == familyId }?.display_name ?: familyId

private fun minZoomForLevels(levels: List<TileLevelAvailability>): Double =
    (levels.minOfOrNull { it.zoom } ?: 4) - 2.8

private fun maxZoomForLevels(levels: List<TileLevelAvailability>): Double =
    (levels.maxOfOrNull { it.zoom } ?: 10) + 0.8

private fun tileSizeForFamily(): Int = 512

fun deriveMapViews(
    resourceIndex: WireResourceIndex,
    preferredIds: List<String>,
): List<MapViewOption> {
    val supported = resourceIndex.chart_collections.filter {
        it.family_id == WireChartFamilyId.Sectional ||
            it.family_id == WireChartFamilyId.Tac ||
            it.family_id == WireChartFamilyId.IfrLow ||
            it.family_id == WireChartFamilyId.IfrHigh
    }
    val selected = if (preferredIds.isNotEmpty()) {
        preferredIds.mapNotNull { id -> supported.firstOrNull { it.id == id } }
    } else {
        supported
    }
    return selected.map { collection ->
        val levels = collection.levels.map { level ->
            TileLevelAvailability(
                zoom = level.zoom,
                xMin = level.x_min,
                xMax = level.x_max,
                yTmsMin = level.y_tms_min,
                yTmsMax = level.y_tms_max,
            )
        }
        MapViewOption(
            id = collection.id,
            label = "${regionDisplayName(resourceIndex.regions, collection.region_id)} ${familyDisplayName(resourceIndex.families, collection.family_id.toResourceId())}",
            regionId = collection.region_id.toCode(),
            mapView = MapView(
                chartFamily = collection.family_id.toUiMapFamily(),
                chartName = "${regionDisplayName(resourceIndex.regions, collection.region_id)} ${familyDisplayName(resourceIndex.families, collection.family_id.toResourceId())}",
                chartIndex = collection.chart_index,
                tileRoot = "tiles",
                tileUrlRoot = "/sectional-packages/${collection.package_id}/tiles",
                tileSize = tileSizeForFamily(),
                minZoom = minZoomForLevels(levels),
                maxZoom = maxZoomForLevels(levels),
                storageKind = TileStorageKind.SectionalPackage,
                packageName = collection.package_id,
                initialViewport = MapViewportSeed(
                    lat = collection.default_view.lat,
                    lon = collection.default_view.lon,
                    zoom = collection.default_view.zoom,
                ),
                levels = levels,
            ),
        )
    }
}

private fun airportIdsFromPlan(plan: FlightPlan): List<String> {
    val result = linkedSetOf<String>()
    plan.departure?.let(result::add)
    plan.destination?.let(result::add)
    plan.alternate?.let(result::add)
    plan.legs.forEach { leg ->
        result.add(leg.fromAirport)
        result.add(leg.toAirport)
    }
    return result.toList()
}

private fun chartAsset(airportId: String, kind: String, label: String, assetPath: String): ChartAsset {
    val filename = assetPath.substringAfterLast('/')
    return ChartAsset(
        id = "$kind:$airportId:$filename",
        airportId = airportId,
        label = if (kind == "csup") "CSup" else label,
        kind = kind,
        assetPath = "chart-assets/$airportId/$filename",
        assetUrl = "/chart-assets/$airportId/$filename",
    )
}

fun deriveChartPage(
    resourceIndex: WireResourceIndex,
    recentAirportIds: List<String>,
    initialAirportIdHint: String?,
    initialChartIdHint: String?,
    samplePlan: FlightPlan,
): ChartPageFixture {
    val plateById = resourceIndex.plates.associateBy { it.id }
    val csupById = resourceIndex.csups.associateBy { it.id }
    val airportResourcesByAirportId = resourceIndex.airport_resources.associateBy { it.airport_id }
    val airportIds = linkedSetOf<String>()
    recentAirportIds.forEach(airportIds::add)
    airportIdsFromPlan(samplePlan).forEach(airportIds::add)
    val airports = airportIds.mapNotNull { airportId ->
        val airportResources = airportResourcesByAirportId[airportId] ?: return@mapNotNull null
        val charts = buildList {
            airportResources.plate_ids.mapNotNull(plateById::get).forEach { record ->
                add(chartAsset(airportId, "plate", record.label, record.asset_path))
            }
            airportResources.csup_ids.mapNotNull(csupById::get).forEach { record ->
                add(chartAsset(airportId, "csup", record.label, record.asset_path))
            }
        }
        if (charts.isEmpty()) {
            null
        } else {
            ChartAirport(
                id = airportId,
                label = airportId,
                charts = charts,
            )
        }
    }
    val initialAirportId =
        initialAirportIdHint?.takeIf { airportId -> airports.any { it.id == airportId } }
            ?: airports.firstOrNull()?.id
            ?: ""
    val initialChartId =
        initialChartIdHint?.takeIf { chartId -> airports.any { airport -> airport.charts.any { it.id == chartId } } }
            ?: airports.firstOrNull { it.id == initialAirportId }?.charts?.firstOrNull()?.id
            ?: ""
    return ChartPageFixture(
        recentAirportIds = airports.map { it.id },
        initialAirportId = initialAirportId,
        initialChartId = initialChartId,
        airports = airports,
    )
}
