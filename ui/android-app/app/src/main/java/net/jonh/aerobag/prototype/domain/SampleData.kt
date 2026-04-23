package net.jonh.aerobag.prototype.domain

import android.content.Context
import kotlinx.serialization.Serializable
import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.decodeFromJsonElement
import kotlinx.serialization.json.put
import java.io.BufferedInputStream
import java.util.zip.ZipInputStream

data class ContentFixture(
    val catalogJson: String,
    val vectorManifestJson: String,
    val resourceIndexJson: String,
    val mapView: MapView,
    val mapViews: List<MapViewOption>,
    val chartPage: ChartPageFixture,
    val mapTileView: MapTileView,
    val samplePlan: FlightPlan,
    val remoteOnlyInventory: ContentInventory,
    val installedInventory: ContentInventory,
    val navKvStore: NavKvStore,
)

@Serializable
private data class WireDevBootstrap(
    val content_policy: String,
    val flight_plan: WireFlightPlan,
    val recent_airport_ids: List<String> = emptyList(),
    val selected_airport_id: String? = null,
    val selected_chart_id: String? = null,
)

object SampleData {
    private const val BOOTSTRAP_ASSET_PATH = "fixtures/dev-bootstrap.json"
    private const val RESOURCE_INDEX_ASSET_PATH = "fixtures/resource-index.json"
    private const val VECTORS_ASSET_PATH = "fixtures/vectors.zip"

    private val json = Json {
        encodeDefaults = true
        ignoreUnknownKeys = true
    }

    fun load(context: Context): ContentFixture {
        val bootstrapPayload = context.assets.open(BOOTSTRAP_ASSET_PATH).bufferedReader().use { it.readText() }
        val resourceIndexPayload = context.assets.open(RESOURCE_INDEX_ASSET_PATH).bufferedReader().use { it.readText() }
        val vectorManifestJson = readZipTextAsset(context, VECTORS_ASSET_PATH, "vectors")
        val bootstrap = json.decodeFromString<WireDevBootstrap>(bootstrapPayload)
        val navKvStore = NavKvStore.open(context)
        val mapViews =
            json.decodeFromJsonElement<List<WireMapViewOption>>(
                navKvStore.runCoreOperationElement(
                    buildJsonObject {
                        put("kind", "chart_catalog")
                    },
                ),
            ).map { it.toUi() }
        val mapView = mapViews.first().mapView
        val samplePlan = bootstrap.flight_plan.toUiFlightPlan()
        val resourceIndex = json.decodeFromString<WireResourceIndex>(resourceIndexPayload)
        val catalogJson = json.encodeToString(deriveWireCatalog(resourceIndex))
        val airportIds = buildSet {
            addAll(bootstrap.recent_airport_ids)
            bootstrap.selected_airport_id?.let(::add)
            samplePlan.departure?.let(::add)
            samplePlan.destination?.let(::add)
        }
        val chartPage = WireDerivedChartPage(
            airports = airportIds.mapNotNull { airportId ->
                json.decodeFromJsonElement<WireDerivedChartAirport?>(
                    navKvStore.runCoreOperationElement(
                        buildJsonObject {
                            put("kind", "plate_airport")
                            put("airport_id", airportId)
                        },
                    ),
                )
            },
        ).toUi()
        val defaultLevel = mapView.levels.maxBy { it.zoom }
        return ContentFixture(
            catalogJson = catalogJson,
            vectorManifestJson = vectorManifestJson,
            resourceIndexJson = resourceIndexPayload,
            mapView = mapView,
            mapViews = mapViews,
            chartPage = chartPage,
            mapTileView = MapTileView(
                chartFamily = mapView.chartFamily,
                chartName = mapView.chartName,
                chartIndex = mapView.chartIndex,
                tileRoot = mapView.tileRoot,
                zoom = defaultLevel.zoom,
                tileSize = mapView.tileSize,
                radius = 0,
                centerX = (defaultLevel.xMin + defaultLevel.xMax) / 2,
                centerYTms = (defaultLevel.yTmsMin + defaultLevel.yTmsMax) / 2,
                probeOffsetX = 0.0,
                probeOffsetY = 0.0,
            ),
            samplePlan = samplePlan,
            remoteOnlyInventory = ContentInventory(installedPackages = emptyList()),
            installedInventory = ContentInventory(installedPackages = emptyList()),
            navKvStore = navKvStore,
        )
    }

    private fun readZipTextAsset(context: Context, assetPath: String, entryName: String): String {
        context.assets.open(assetPath).use { assetStream ->
            ZipInputStream(BufferedInputStream(assetStream)).use { zipStream ->
                while (true) {
                    val entry = zipStream.nextEntry ?: break
                    if (!entry.isDirectory && entry.name == entryName) {
                        return zipStream.readBytes().decodeToString()
                    }
                }
            }
        }
        error("missing $entryName in $assetPath")
    }
}

private fun WireMapViewOption.toUi() = MapViewOption(
    id = id,
    label = label,
    regionId = region_id.toCode(),
    mapView = map_view.toUi(),
)

private fun WireMapView.toUi() = MapView(
    chartFamily = chart_family.toUi(),
    chartName = chart_name,
    chartIndex = chart_index,
    tileRoot = tile_root,
    tileUrlRoot = tile_url_root,
    tileSize = tile_size,
    minZoom = min_zoom,
    maxZoom = max_zoom,
    storageKind = storage_kind.toUi(),
    packageName = package_name,
    initialViewport = MapViewportSeed(
        lat = initial_viewport.lat,
        lon = initial_viewport.lon,
        zoom = initial_viewport.zoom,
    ),
    levels = levels.map { level ->
        TileLevelAvailability(
            zoom = level.zoom,
            xMin = level.x_min,
            xMax = level.x_max,
            yTmsMin = level.y_tms_min,
            yTmsMax = level.y_tms_max,
        )
    },
)

private fun WireChartFamilyId.toUi() = when (this) {
    WireChartFamilyId.Sec -> MapChartFamily.Sec
    WireChartFamilyId.Tac -> MapChartFamily.Tac
    WireChartFamilyId.EnrL -> MapChartFamily.EnrL
    WireChartFamilyId.EnrH -> MapChartFamily.EnrH
    WireChartFamilyId.ShadedRelief -> MapChartFamily.ShadedRelief
}

private fun WireTileStorageKind.toUi() = when (this) {
    WireTileStorageKind.AssetTree -> TileStorageKind.AssetTree
    WireTileStorageKind.SectionalPackage -> TileStorageKind.SectionalPackage
    WireTileStorageKind.StaticProduct -> TileStorageKind.StaticProduct
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

private fun ChartPageFixture.toWire() = WireDerivedChartPage(
    airports = airports.map { airport ->
        WireDerivedChartAirport(
            id = airport.id,
            label = airport.label,
            charts = airport.charts.map { chart ->
                WireDerivedChartAsset(
                    id = chart.id,
                    airport_id = chart.airportId,
                    package_id = chart.packageId,
                    label = chart.label,
                    kind = chart.kind,
                    folder_category = chart.folderCategory,
                    source_asset_path = chart.sourceAssetPath,
                    asset_path = chart.assetPath,
                    asset_url = chart.assetUrl,
                    thumbnail_source_path = chart.thumbnailSourceAssetPath,
                    thumbnail_path = chart.thumbnailAssetPath,
                    thumbnail_url = chart.thumbnailUrl,
                )
            },
        )
    },
)
