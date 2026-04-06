package net.jonh.aerobag.prototype.domain

import android.content.Context
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.Json

data class ContentFixture(
    val catalog: Catalog,
    val samplePlan: FlightPlan,
    val remoteOnlyInventory: ContentInventory,
    val installedInventory: ContentInventory,
)

@Serializable
private data class WireContentFixture(
    val catalog: WireCatalog,
    val flight_plan: WireFlightPlan,
    val remote_only_inventory: WireContentInventory,
    val installed_inventory: WireContentInventory,
)

object SampleData {
    private const val ASSET_PATH = "fixtures/contentFixture.json"

    private val json = Json {
        ignoreUnknownKeys = true
    }

    fun load(context: Context): ContentFixture {
        val payload = context.assets.open(ASSET_PATH).bufferedReader().use { it.readText() }
        val fixture = json.decodeFromString<WireContentFixture>(payload)
        return ContentFixture(
            catalog = fixture.catalog.toUiCatalog(),
            samplePlan = fixture.flight_plan.toUiFlightPlan(),
            remoteOnlyInventory = fixture.remote_only_inventory.toUiInventory(),
            installedInventory = fixture.installed_inventory.toUiInventory(),
        )
    }
}
