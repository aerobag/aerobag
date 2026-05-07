package net.jonh.aerobag.prototype.domain

import android.content.Context
import android.os.SystemClock
import android.util.Log
import java.net.HttpURLConnection
import java.net.URL
import java.time.Instant
import kotlinx.serialization.Serializable
import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.decodeFromJsonElement
import kotlinx.serialization.json.intOrNull
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.put
import kotlinx.serialization.json.jsonPrimitive

data class BootstrapFixture(
    val packageManagementNowEpochMsOverride: Long?,
    val packageManagementDiscoveryFilenames: List<String>,
    val samplePlan: FlightPlan,
)

data class ContentFixture(
    val bootstrap: BootstrapFixture,
    val vectorManifestJson: String,
    val vectorPackageId: String?,
    val mapView: MapView,
    val mapViews: List<MapViewOption>,
    val chartPage: ChartPageFixture,
    val mapTileView: MapTileView,
    val samplePlan: FlightPlan,
    val remoteOnlyInventory: ContentInventory,
    val installedInventory: ContentInventory,
    val navKvStore: NavKvStore,
)

data class NavDbArtifactStatus(
    val packageId: String,
    val filename: String,
    val readable: Boolean,
    val message: String? = null,
)

data class NavDbStatus(
    val installed: List<NavDbArtifactStatus>,
)

@Serializable
private data class WireDevBootstrap(
    val content_policy: String,
    val flight_plan: WireFlightPlan,
    val recent_airport_ids: List<String> = emptyList(),
    val selected_airport_id: String? = null,
    val selected_chart_id: String? = null,
    val package_management_now_utc: String? = null,
    val package_management_discovery_filenames: List<String> = emptyList(),
)

object SampleData {
    private const val BOOTSTRAP_ASSET_PATH = "fixtures/dev-bootstrap.json"
    private const val DEV_SERVER_BASE_URL_ASSET_PATH = "fixtures/android-dev-server-base-url.txt"
    private const val DEFAULT_ANDROID_DEV_SERVER_BASE_URL = "http://10.0.2.2:8082"
    private const val TAG = "SampleData"
    private const val FALLBACK_VECTOR_MANIFEST_JSON =
        """{"airspace":{"reference_tile_min_zoom":0,"reference_tile_max_zoom":12,"label_tile_min_zoom":0,"label_tile_max_zoom":12}}"""

    private val json = Json {
        encodeDefaults = true
        ignoreUnknownKeys = true
    }

    fun loadBootstrap(context: Context): BootstrapFixture {
        val bootstrapPayload = context.assets.open(BOOTSTRAP_ASSET_PATH).bufferedReader().use { it.readText() }
        val bootstrap = json.decodeFromString<WireDevBootstrap>(bootstrapPayload)
        return BootstrapFixture(
            packageManagementNowEpochMsOverride = bootstrap.package_management_now_utc?.let {
                Instant.parse(it).toEpochMilli()
            },
            packageManagementDiscoveryFilenames = bootstrap.package_management_discovery_filenames,
            samplePlan = bootstrap.flight_plan.toUiFlightPlan(),
        )
    }

    fun loadRuntime(context: Context, bootstrapFixture: BootstrapFixture): ContentFixture {
        val navKvOpenStartMs = SystemClock.elapsedRealtime()
        val navDbArtifact = latestReadableInstalledNavDbArtifact(context)
        val navKvStore = NavKvStore.open(navDbZip = navDbArtifact.file)
        val navKvOpenMs = SystemClock.elapsedRealtime() - navKvOpenStartMs
        return loadRuntime(
            context = context,
            bootstrapFixture = bootstrapFixture,
            navKvStore = navKvStore,
            navKvOpenMs = navKvOpenMs,
        )
    }

    fun loadRuntime(
        context: Context,
        bootstrapFixture: BootstrapFixture,
        navKvStore: NavKvStore,
        navKvOpenMs: Long,
    ): ContentFixture {
        val startMs = SystemClock.elapsedRealtime()
        val vectorManifestStartMs = SystemClock.elapsedRealtime()
        val vectorManifestJson = navKvStore.runCoreOperationElement(
            buildJsonObject {
                put("kind", "vector_manifest")
            },
        ).toString().let { augmentVectorManifestWithDynamicPointLayers(context, it) }
        val vectorManifestMs = SystemClock.elapsedRealtime() - vectorManifestStartMs
        val chartCatalogStartMs = SystemClock.elapsedRealtime()
        val mapViews =
            json.decodeFromJsonElement<List<WireMapViewOption>>(
                navKvStore.runCoreOperationElement(
                    buildJsonObject {
                        put("kind", "chart_catalog")
                    },
                ),
            ).map { it.toUi() }
        val fullCoverageCount = mapViews.count { it.mapView.fullCoverageZoom != null }
        Log.i(TAG, "chartCatalog mapViews=${mapViews.size} fullCoverageZoom=$fullCoverageCount")
        val chartCatalogMs = SystemClock.elapsedRealtime() - chartCatalogStartMs
        val mapView = mapViews.first().mapView
        val samplePlan = bootstrapFixture.samplePlan
        val airportIds = buildSet {
            samplePlan.departure?.let(::add)
            samplePlan.destination?.let(::add)
        }
        val plateAirportStartMs = SystemClock.elapsedRealtime()
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
        val plateAirportMs = SystemClock.elapsedRealtime() - plateAirportStartMs
        val defaultLevel = mapView.levels.maxBy { it.zoom }
        return ContentFixture(
            bootstrap = bootstrapFixture,
            vectorManifestJson = vectorManifestJson,
            vectorPackageId = null,
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
        ).also {
            Log.i(
                TAG,
                "loadRuntime completed in ${SystemClock.elapsedRealtime() - startMs}ms " +
                    "(navKvOpen=${navKvOpenMs}ms vectorManifest=${vectorManifestMs}ms chartCatalog=${chartCatalogMs}ms plateAirport=${plateAirportMs}ms)",
            )
        }
    }

    private fun augmentVectorManifestWithDynamicPointLayers(context: Context, vectorManifestJson: String): String =
        runCatching {
            val devServerBaseUrl = loadAndroidDevServerBaseUrl(context)
            val metarManifestJson = fetchJsonOrNull(resolveDevServerUrl("/fast-products/metars/manifest.json", devServerBaseUrl))
                ?: return vectorManifestJson
            val metarManifest = json.parseToJsonElement(metarManifestJson).jsonObject
            val mapView = metarManifest["map_view"]?.jsonObject ?: return vectorManifestJson
            val availableZooms = mapView["levels"]
                ?.jsonArray
                ?.mapNotNull { level -> level.jsonObject["zoom"]?.jsonPrimitive?.intOrNull }
                ?.distinct()
                ?.sorted()
                .orEmpty()
            if (availableZooms.isEmpty()) {
                return vectorManifestJson
            }

            val tilePathTemplate = mapView["tile_path_template"]?.jsonPrimitive?.contentOrNull
                ?: "points/metars/{z}/{x}/{y}.json"
            val metarLayer = buildJsonObject {
                put("min_zoom", mapView["min_zoom"] ?: JsonPrimitive(availableZooms.first()))
                put("max_zoom", mapView["max_zoom"] ?: JsonPrimitive(availableZooms.last()))
                put("available_zooms", JsonArray(availableZooms.map(::JsonPrimitive)))
                put("tile_path_template", tilePathTemplate)
            }

            val baseManifest = json.parseToJsonElement(vectorManifestJson).jsonObject
            val merged = baseManifest.toMutableMap()
            val pointLayers = (baseManifest["point_layers"] as? JsonObject)?.toMutableMap() ?: mutableMapOf()
            pointLayers["metars"] = metarLayer
            merged["point_layers"] = JsonObject(pointLayers)

            val files = (baseManifest["files"] as? JsonObject)?.toMutableMap() ?: mutableMapOf()
            files["point_tiles_metars"] = JsonPrimitive(tilePathTemplate)
            files["metars"] = JsonPrimitive("metars.json")
            merged["files"] = JsonObject(files)

            Log.i(TAG, "vectorManifest dynamic metars zooms=${availableZooms.joinToString(",")}")
            JsonObject(merged).toString()
        }.getOrElse { error ->
            Log.w(TAG, "dynamic METAR layer metadata unavailable", error)
            vectorManifestJson
        }

    private fun loadAndroidDevServerBaseUrl(context: Context): String =
        runCatching {
            context.assets.open(DEV_SERVER_BASE_URL_ASSET_PATH)
                .bufferedReader()
                .use { it.readText().trim() }
                .takeIf { it.isNotBlank() }
        }.getOrNull() ?: DEFAULT_ANDROID_DEV_SERVER_BASE_URL

    private fun resolveDevServerUrl(sourcePath: String, devServerBaseUrl: String): String =
        when {
            sourcePath.startsWith("http://") || sourcePath.startsWith("https://") -> sourcePath
            sourcePath.startsWith("/") -> "$devServerBaseUrl$sourcePath"
            else -> "$devServerBaseUrl/$sourcePath"
        }

    private fun fetchJsonOrNull(url: String): String? =
        runCatching {
            val connection = URL(url).openConnection() as HttpURLConnection
            connection.connectTimeout = 1500
            connection.readTimeout = 2500
            connection.inputStream.bufferedReader().use { it.readText() }
        }.getOrNull()

    fun inspectNavDbStatus(
        context: Context,
        bridge: NativeBridge = NativeBindings,
    ): NavDbStatus {
        val appContext = context.applicationContext
        val installed = InstalledPackages.listInstalledArtifacts(appContext, InstalledPackageKind.Data)
            .filter { it.artifactId.startsWith("NAV_DB_") }
            .sortedWith(compareByDescending<InstalledPackageArtifact> { it.file.lastModified() }.thenByDescending { it.filename })
            .map { artifact ->
                val status = runCatching {
                    NavKvStore.open(navDbZip = artifact.file, bridge = bridge).use { }
                    NavDbArtifactStatus(
                        packageId = artifact.artifactId,
                        filename = artifact.filename,
                        readable = true,
                    )
                }.getOrElse { error ->
                    NavDbArtifactStatus(
                        packageId = artifact.artifactId,
                        filename = artifact.filename,
                        readable = false,
                        message = error.message ?: error::class.simpleName,
                    )
                }
                status
            }
        return NavDbStatus(installed = installed)
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

private fun latestReadableInstalledNavDbArtifact(context: Context): InstalledPackageArtifact =
    InstalledPackages.listInstalledArtifacts(context, InstalledPackageKind.Data)
        .filter { it.artifactId.startsWith("NAV_DB_") }
        .sortedWith(compareByDescending<InstalledPackageArtifact> { it.file.lastModified() }.thenByDescending { it.filename })
        .firstOrNull { artifact ->
            runCatching { NavKvStore.open(navDbZip = artifact.file).use { } }.isSuccess
        }
        ?: error("missing readable installed data package with prefix NAV_DB_")

private fun latestInstalledDataPackageIdOrNull(context: Context, prefix: String): String? =
    InstalledPackages.listInstalledPackageIds(context, InstalledPackageKind.Data)
        .filter { it.startsWith(prefix) }
        .maxOrNull()

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
    maxSourceZoom = max_source_zoom,
    maxDisplayZoom = max_display_zoom,
    storageKind = storage_kind.toUi(),
    packageName = package_name,
    fullCoverageZoom = full_coverage_zoom,
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
    WireChartFamilyId.WorldBasemap -> MapChartFamily.WorldBasemap
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
    WireRegionId.World -> "world"
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
