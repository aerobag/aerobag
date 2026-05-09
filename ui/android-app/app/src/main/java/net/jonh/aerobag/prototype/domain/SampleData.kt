package net.jonh.aerobag.prototype.domain

import android.content.Context
import android.os.SystemClock
import android.util.Log
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
)

data class ContentFixture(
    val bootstrap: BootstrapFixture,
    val vectorManifestJson: String,
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
    val recent_airport_ids: List<String> = emptyList(),
    val selected_airport_id: String? = null,
    val selected_chart_id: String? = null,
    val package_management_now_utc: String? = null,
)

object SampleData {
    private const val BOOTSTRAP_ASSET_PATH = "fixtures/dev-bootstrap.json"
    private const val TAG = "SampleData"
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
        return ContentFixture(
            bootstrap = bootstrapFixture,
            vectorManifestJson = vectorManifestJson,
            navKvStore = navKvStore,
        ).also {
            Log.i(
                TAG,
                "loadRuntime completed in ${SystemClock.elapsedRealtime() - startMs}ms " +
                    "(navKvOpen=${navKvOpenMs}ms vectorManifest=${vectorManifestMs}ms)",
            )
        }
    }

    private fun augmentVectorManifestWithDynamicPointLayers(context: Context, vectorManifestJson: String): String =
        runCatching {
            val metarManifestJson = InstalledPackages.readZipEntryText(
                context,
                InstalledPackageKind.Data,
                "metars",
                "manifest.json",
            )
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
