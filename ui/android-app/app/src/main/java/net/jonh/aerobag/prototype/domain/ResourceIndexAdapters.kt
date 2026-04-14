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
    val thumbnail_path: String? = null,
    val label: String,
    val asset_kind: String,
    val document_type: String,
)

@Serializable
data class WireResourceCsup(
    val id: String,
    val airport_id: String,
    val region_id: WireRegionId,
    val package_id: String,
    val asset_path: String,
    val thumbnail_path: String? = null,
    val label: String,
    val asset_kind: String,
    val document_type: String,
)

private fun WireChartFamilyId.toUiMapFamily() = when (this) {
    WireChartFamilyId.Sec -> MapChartFamily.Sec
    WireChartFamilyId.Tac -> MapChartFamily.Tac
    WireChartFamilyId.EnrL -> MapChartFamily.EnrL
    WireChartFamilyId.EnrH -> MapChartFamily.EnrH
}

private fun WireChartFamilyId.toResourceId() = when (this) {
    WireChartFamilyId.Sec -> "sec"
    WireChartFamilyId.Tac -> "tac"
    WireChartFamilyId.EnrL -> "enr-l"
    WireChartFamilyId.EnrH -> "enr-h"
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

fun deriveWireCatalog(resourceIndex: WireResourceIndex): WireCatalog {
    val cycle = resourceIndex.cycle ?: "unknown"
    val supportedFamilies = setOf("sec", "tac", "enr-l", "enr-h")
    val familyById = resourceIndex.families.associateBy { it.id }
    val packageById = resourceIndex.packages.associateBy { it.id }

    val families = resourceIndex.families.mapNotNull { family ->
        val familyId = when (family.id) {
            "sec" -> WireChartFamilyId.Sec
            "tac" -> WireChartFamilyId.Tac
            "enr-l" -> WireChartFamilyId.EnrL
            "enr-h" -> WireChartFamilyId.EnrH
            else -> null
        } ?: return@mapNotNull null
        val maxZoom = resourceIndex.chart_collections
            .filter { it.family_id == familyId }
            .maxOfOrNull { collection -> collection.levels.maxOfOrNull { level -> level.zoom } ?: 0 }
        WireCatalogFamily(
            id = familyId,
            display_name = family.display_name,
            kind = family.kind,
            max_zoom = maxZoom,
            tile_size = if (family.kind == "tiled_raster") tileSizeForFamily() else null,
        )
    }

    val packages = resourceIndex.packages.mapNotNull { pkg ->
        val familyId = when (pkg.family_id) {
            "sec" -> WireChartFamilyId.Sec
            "tac" -> WireChartFamilyId.Tac
            "enr-l" -> WireChartFamilyId.EnrL
            "enr-h" -> WireChartFamilyId.EnrH
            else -> null
        } ?: return@mapNotNull null
        WireCatalogPackage(
            id = WirePackageId(
                region = pkg.region_id,
                family = familyId,
                cycle = cycle,
            ),
            package_name = pkg.id,
            family_id = familyId,
            region_id = pkg.region_id,
            cycle = cycle,
            artifact_kind = "zip",
            relative_url = pkg.id,
            manifest_name = pkg.id,
            size_bytes = pkg.size_bytes,
            checksum_sha256 = pkg.checksum_sha256,
        )
    }

    val charts = resourceIndex.chart_collections.filter { it.family_id.toResourceId() in supportedFamilies }.map { collection ->
        val familyDisplay = familyById[collection.family_id.toResourceId()]?.display_name ?: collection.family_id.toResourceId()
        val regionDisplay = regionDisplayName(resourceIndex.regions, collection.region_id)
        WireChartRecord(
            id = WireChartId(
                family = collection.family_id,
                name = collection.id,
                cycle = cycle,
            ),
            family_id = collection.family_id,
            name = collection.id,
            display_name = "$regionDisplay $familyDisplay",
            cycle = cycle,
            region_ids = listOf(collection.region_id),
            max_zoom = collection.levels.maxOfOrNull { it.zoom } ?: 0,
            tile_path_template = collection.tile_path_template,
        )
    }

    val plates = resourceIndex.plates.mapNotNull { plate ->
        val packageRecord = packageById[plate.package_id] ?: return@mapNotNull null
        WirePlateRecord(
            id = WirePlateId(
                airport_id = plate.airport_id,
                procedure_code = plate.id,
                page = 1,
                cycle = cycle,
            ),
            airport_id = plate.airport_id,
            region_id = plate.region_id,
            cycle = cycle,
            procedure_code = plate.id,
            display_name = plate.label,
            kind = plate.asset_kind,
            georeferenced = true,
            page_count = 1,
            asset_base_path = "${packageRecord.id}/${plate.asset_path.removeSuffix(".png")}",
        )
    }

    val supplements = resourceIndex.csups.mapNotNull { csup ->
        val packageRecord = packageById[csup.package_id] ?: return@mapNotNull null
        WireSupplementRecord(
            airport_id = csup.airport_id,
            region_id = csup.region_id,
            cycle = cycle,
            page_count = 1,
            asset_base_path = "${packageRecord.id}/${csup.asset_path.removeSuffix(".png")}",
        )
    }

    return WireCatalog(
        schema_version = resourceIndex.schema_version,
        cycle = cycle,
        catalog_revision = resourceIndex.generated_at_utc,
        families = families,
        regions = resourceIndex.regions,
        packages = packages,
        charts = charts,
        plates = plates,
        supplements = supplements,
    )
}

fun deriveMapViews(
    resourceIndex: WireResourceIndex,
    preferredIds: List<String>,
): List<MapViewOption> {
    val supported = resourceIndex.chart_collections.filter {
        it.family_id == WireChartFamilyId.Sec ||
            it.family_id == WireChartFamilyId.Tac ||
            it.family_id == WireChartFamilyId.EnrL ||
            it.family_id == WireChartFamilyId.EnrH
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
    plan.routeComponents.forEach { component ->
        when (component) {
            is RouteComponent.Waypoint -> airportCode(component.waypoint)?.let(result::add)
            is RouteComponent.Procedure -> result.add(component.procedure.airportId)
            is RouteComponent.Airway -> {}
        }
    }
    return result.toList()
}

private fun airportCode(ref: NavRef): String? = when (ref) {
    is NavRef.Airport -> ref.code
    else -> null
}

private fun folderCategory(documentType: String): String = when (documentType) {
    "airport_diagram" -> "airport-diagram"
    "takeoff_minimums", "alternate_minimums", "minimums" -> "takeoff-mins"
    "departure" -> "departure"
    "star" -> "star"
    "csup" -> "csup"
    "approach", "other" -> "approach"
    else -> "approach"
}

private fun folderCategoryRank(category: String): Int = when (category) {
    "airport-diagram" -> 0
    "csup" -> 1
    "takeoff-mins" -> 2
    "approach" -> 3
    "departure" -> 4
    "star" -> 5
    else -> 6
}

private fun chartAsset(
    airportId: String,
    packageId: String,
    kind: String,
    label: String,
    documentType: String,
    assetPath: String,
    thumbnailPath: String?,
): ChartAsset {
    return ChartAsset(
        id = "$kind:$airportId:${assetPath.substringAfterLast('/')}",
        airportId = airportId,
        packageId = packageId,
        label = if (kind == "csup") "CSup" else label,
        kind = kind,
        folderCategory = folderCategory(if (kind == "csup") "csup" else documentType),
        sourceAssetPath = assetPath,
        assetPath = assetPath,
        assetUrl = "/$assetPath",
        thumbnailSourceAssetPath = thumbnailPath,
        thumbnailAssetPath = thumbnailPath,
        thumbnailUrl = thumbnailPath?.let { "/$it" },
    )
}

fun deriveChartPage(
    resourceIndex: WireResourceIndex,
    samplePlan: FlightPlan,
    allowedPackageIds: Set<String>? = null,
): ChartPageFixture {
    val plateById = resourceIndex.plates.associateBy { it.id }
    val csupById = resourceIndex.csups.associateBy { it.id }
    val airportResourcesByAirportId = resourceIndex.airport_resources.associateBy { it.airport_id }
    val airportIds = linkedSetOf<String>()
    airportIdsFromPlan(samplePlan).forEach(airportIds::add)
    val airports = airportIds.mapNotNull { airportId ->
        val airportResources = airportResourcesByAirportId[airportId] ?: return@mapNotNull null
        val charts = buildList {
            airportResources.plate_ids.mapNotNull(plateById::get).filter { record ->
                allowedPackageIds == null || allowedPackageIds.contains(record.package_id)
            }.forEach { record ->
                add(chartAsset(airportId, record.package_id, "plate", record.label, record.document_type, record.asset_path, record.thumbnail_path))
            }
            airportResources.csup_ids.mapNotNull(csupById::get).filter { record ->
                allowedPackageIds == null || allowedPackageIds.contains(record.package_id)
            }.forEach { record ->
                add(chartAsset(airportId, record.package_id, "csup", record.label, record.document_type, record.asset_path, record.thumbnail_path))
            }
        }.sortedWith(compareBy<ChartAsset>({ folderCategoryRank(it.folderCategory) }, { it.label }))
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
    return ChartPageFixture(
        airports = airports,
    )
}
