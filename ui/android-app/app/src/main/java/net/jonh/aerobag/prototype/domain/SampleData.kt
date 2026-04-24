package net.jonh.aerobag.prototype.domain

import android.content.Context
import java.time.Instant
import kotlinx.serialization.Serializable
import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.decodeFromJsonElement
import kotlinx.serialization.json.put

data class BootstrapFixture(
    val currentArtifactsJson: String,
    val cycleBundleJson: String,
    val packageManagementDiscoveryJsons: List<String>,
    val packageManagementBundleJsonsByFilename: Map<String, String>,
    val packageManagementNowEpochMsOverride: Long?,
    val samplePlan: FlightPlan,
)

data class ContentFixture(
    val bootstrap: BootstrapFixture,
    val vectorManifestJson: String,
    val vectorPackageId: String,
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
    val package_management_now_utc: String? = null,
)

object SampleData {
    private const val BOOTSTRAP_ASSET_PATH = "fixtures/dev-bootstrap.json"
    private const val CURRENT_ARTIFACTS_ASSET_PATH = "fixtures/current-artifacts.json"
    private const val CYCLE_BUNDLE_ASSET_PATH = "fixtures/cycle-bundle.json"
    private const val PACKAGE_MANAGEMENT_DISCOVERY_ASSET_PATH = "fixtures/package-management/discovery"
    private const val PACKAGE_MANAGEMENT_BUNDLES_ASSET_PATH = "fixtures/package-management/bundles"

    private val json = Json {
        encodeDefaults = true
        ignoreUnknownKeys = true
    }

    fun loadBootstrap(context: Context): BootstrapFixture {
        val bootstrapPayload = context.assets.open(BOOTSTRAP_ASSET_PATH).bufferedReader().use { it.readText() }
        val currentArtifactsPayload =
            context.assets.open(CURRENT_ARTIFACTS_ASSET_PATH).bufferedReader().use { it.readText() }
        val cycleBundlePayload = context.assets.open(CYCLE_BUNDLE_ASSET_PATH).bufferedReader().use { it.readText() }
        val bootstrap = json.decodeFromString<WireDevBootstrap>(bootstrapPayload)
        val cycleBundle = json.decodeFromString<WireBundleManifest>(cycleBundlePayload)
        val packageManagementDiscoveryJsons = context.assets
            .list(PACKAGE_MANAGEMENT_DISCOVERY_ASSET_PATH)
            ?.sorted()
            ?.map { filename ->
                context.assets.open("$PACKAGE_MANAGEMENT_DISCOVERY_ASSET_PATH/$filename")
                    .bufferedReader()
                    .use { it.readText() }
            }
            .orEmpty()
        val packageManagementBundleJsonsByFilename = context.assets
            .list(PACKAGE_MANAGEMENT_BUNDLES_ASSET_PATH)
            ?.sorted()
            ?.associateWith { filename ->
                context.assets.open("$PACKAGE_MANAGEMENT_BUNDLES_ASSET_PATH/$filename")
                    .bufferedReader()
                    .use { it.readText() }
            }
            .orEmpty()
        return BootstrapFixture(
            currentArtifactsJson = currentArtifactsPayload,
            cycleBundleJson = cycleBundlePayload,
            packageManagementDiscoveryJsons = packageManagementDiscoveryJsons,
            packageManagementBundleJsonsByFilename = packageManagementBundleJsonsByFilename,
            packageManagementNowEpochMsOverride = bootstrap.package_management_now_utc?.let {
                Instant.parse(it).toEpochMilli()
            },
            samplePlan = bootstrap.flight_plan.toUiFlightPlan(),
        )
    }

    fun loadRuntime(context: Context, bootstrapFixture: BootstrapFixture): ContentFixture {
        val cycleBundle = json.decodeFromString<WireBundleManifest>(bootstrapFixture.cycleBundleJson)
        val navDbPackageId = cycleBundle.singlePackageId("nav-db")
        val vectorsPackageId = cycleBundle.singlePackageId("vectors")
        val navKvStore = NavKvStore.open(context, navDbPackageId)
        val vectorManifestJson = InstalledPackages.readZipEntryText(
            context,
            InstalledPackageKind.Data,
            vectorsPackageId,
            "vectors",
        )
        val mapViews =
            json.decodeFromJsonElement<List<WireMapViewOption>>(
                navKvStore.runCoreOperationElement(
                    buildJsonObject {
                        put("kind", "chart_catalog")
                    },
                ),
            ).map { it.toUi() }
        val mapView = mapViews.first().mapView
        val samplePlan = bootstrapFixture.samplePlan
        val airportIds = buildSet {
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
            bootstrap = bootstrapFixture,
            vectorManifestJson = vectorManifestJson,
            vectorPackageId = vectorsPackageId,
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
}

@Serializable
private data class WireBundleManifest(
    val packages: List<WireBundlePackage>,
)

@Serializable
private data class WireBundlePackage(
    val id: String,
    val family_id: String,
)

private fun WireBundleManifest.singlePackageId(familyId: String): String =
    packages.singleOrNull { it.family_id == familyId }?.id
        ?: error("expected exactly one $familyId package in cycle bundle")

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
