package net.jonh.aerobag.prototype.domain

import android.content.Context
import kotlinx.serialization.Serializable
import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.jsonObject

data class ContentFixture(
    val catalog: Catalog,
    val catalogJson: String,
    val geometryJson: String,
    val initialProbe: MapProbe,
    val mapTileView: MapTileView,
    val samplePlan: FlightPlan,
    val remoteOnlyInventory: ContentInventory,
    val installedInventory: ContentInventory,
)

@Serializable
private data class WireContentFixture(
    val catalog: WireCatalog,
    val geometry: WireGeometryBundle,
    val initial_probe: WireInitialProbe,
    val map_tile_view: WireMapTileView,
    val flight_plan: WireFlightPlan,
    val remote_only_inventory: WireContentInventory,
    val installed_inventory: WireContentInventory,
)

object SampleData {
    private const val ASSET_PATH = "fixtures/contentFixture.json"

    private val json = Json {
        encodeDefaults = true
        ignoreUnknownKeys = true
    }

    fun load(context: Context): ContentFixture {
        val payload = context.assets.open(ASSET_PATH).bufferedReader().use { it.readText() }
        val fixtureElement = json.parseToJsonElement(payload).jsonObject
        val fixture = json.decodeFromString<WireContentFixture>(payload)
        return ContentFixture(
            catalog = fixture.catalog.toUiCatalog(),
            catalogJson = fixtureElement.getValue("catalog").toString(),
            geometryJson = fixtureElement.getValue("geometry").toString(),
            initialProbe = fixture.initial_probe.toUi(),
            mapTileView = fixture.map_tile_view.toUi(),
            samplePlan = fixture.flight_plan.toUiFlightPlan(),
            remoteOnlyInventory = fixture.remote_only_inventory.toUiInventory(),
            installedInventory = fixture.installed_inventory.toUiInventory(),
        )
    }
}

private fun WireInitialProbe.toUi() = MapProbe(
    family = when (family) {
        WireChartFamilyId.Sectional -> MapChartFamily.Sectional
        WireChartFamilyId.Tac -> MapChartFamily.Tac
    },
    lat = lat,
    lon = lon,
)

private fun WireMapTileView.toUi() = MapTileView(
    chartFamily = when (chart_family) {
        WireChartFamilyId.Sectional -> MapChartFamily.Sectional
        WireChartFamilyId.Tac -> MapChartFamily.Tac
    },
    chartName = chart_name,
    chartIndex = chart_index,
    tileRoot = tile_root,
    zoom = zoom,
    tileSize = tile_size,
    radius = radius,
    centerX = center_x,
    centerYTms = center_y_tms,
    probeOffsetX = probe_offset_x,
    probeOffsetY = probe_offset_y,
)
