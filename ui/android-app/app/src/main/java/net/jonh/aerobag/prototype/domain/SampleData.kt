package net.jonh.aerobag.prototype.domain

import android.content.Context
import kotlinx.serialization.Serializable
import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.Json

data class ContentFixture(
    val catalogJson: String,
    val chartCatalogJson: String,
    val resourceIndexJson: String,
    val mapView: MapView,
    val mapViews: List<MapViewOption>,
    val chartPage: ChartPageFixture,
    val mapTileView: MapTileView,
    val samplePlan: FlightPlan,
    val remoteOnlyInventory: ContentInventory,
    val installedInventory: ContentInventory,
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

    private val json = Json {
        encodeDefaults = true
        ignoreUnknownKeys = true
    }

    fun load(context: Context): ContentFixture {
        val bootstrapPayload = context.assets.open(BOOTSTRAP_ASSET_PATH).bufferedReader().use { it.readText() }
        val resourceIndexPayload = context.assets.open(RESOURCE_INDEX_ASSET_PATH).bufferedReader().use { it.readText() }
        val bootstrap = json.decodeFromString<WireDevBootstrap>(bootstrapPayload)
        val resourceIndex = json.decodeFromString<WireResourceIndex>(resourceIndexPayload)
        val mapViews = deriveMapViews(resourceIndex, emptyList())
        val mapView = mapViews.first().mapView
        val samplePlan = bootstrap.flight_plan.toUiFlightPlan()
        val chartPage = deriveChartPage(resourceIndex = resourceIndex, samplePlan = samplePlan)
        val defaultLevel = mapView.levels.maxBy { it.zoom }
        return ContentFixture(
            catalogJson = json.encodeToString(deriveWireCatalog(resourceIndex)),
            chartCatalogJson = json.encodeToString(chartPage.toWire()),
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
        )
    }
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
