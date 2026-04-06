package net.jonh.aerobag.prototype.domain

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.Json

class ContentLogicTest {
    private val appCore: AppCoreAdapter = MockAppCoreAdapter()
    private val json = Json {
        encodeDefaults = true
        ignoreUnknownKeys = true
    }

    @Test
    fun streamAllowedTreatsRemoteOnlyContentAsSatisfied() {
        var state = appCore.replaceFlightPlan(AppState(), SampleDataFixture.catalog, SampleDataFixture.samplePlan)
        state = appCore.setContentPolicy(state, ContentPolicy.StreamAllowed)
        state = appCore.refreshContent(state, SampleDataFixture.remoteOnlyInventory)

        assertTrue(state.lastContentReport!!.fullySatisfied)
        assertEquals(
            ContentAvailability.RemoteOnly,
            state.lastContentReport!!.items.first().availability.availability,
        )
    }

    @Test
    fun offlineRequiredNeedsInstalledContent() {
        var state = appCore.replaceFlightPlan(AppState(), SampleDataFixture.catalog, SampleDataFixture.samplePlan)
        state = appCore.setContentPolicy(state, ContentPolicy.OfflineRequired)
        state = appCore.refreshContent(state, SampleDataFixture.remoteOnlyInventory)

        assertEquals(
            ContentAvailability.Unavailable,
            state.lastContentReport!!.items.first().availability.availability,
        )
    }

    @Test
    fun installedContentIsOfflineUsable() {
        var state = appCore.replaceFlightPlan(AppState(), SampleDataFixture.catalog, SampleDataFixture.samplePlan)
        state = appCore.setContentPolicy(state, ContentPolicy.OfflineRequired)
        state = appCore.refreshContent(state, SampleDataFixture.installedInventory)

        assertTrue(state.lastContentReport!!.fullySatisfied)
        assertTrue(state.lastContentReport!!.items.first().availability.offlineUsable)
    }

    @Test(expected = IllegalArgumentException::class)
    fun emptyPlansAreRejected() {
        appCore.replaceFlightPlan(
            AppState(),
            SampleDataFixture.catalog,
            SampleDataFixture.samplePlan.copy(legs = emptyList()),
        )
    }

    @Test
    fun nativeAdapterMatchesMockContractForRemoteOnlyStreaming() {
        val nativeAdapter = NativeAppCoreAdapter(
            catalog = SampleDataFixture.catalog,
            bridge = FakeNativeBridge(json),
            json = json,
        )

        var mockState = appCore.replaceFlightPlan(AppState(), SampleDataFixture.catalog, SampleDataFixture.samplePlan)
        mockState = appCore.setContentPolicy(mockState, ContentPolicy.StreamAllowed)
        mockState = appCore.refreshContent(mockState, SampleDataFixture.remoteOnlyInventory)

        var nativeState = nativeAdapter.replaceFlightPlan(AppState(), SampleDataFixture.catalog, SampleDataFixture.samplePlan)
        nativeState = nativeAdapter.setContentPolicy(nativeState, ContentPolicy.StreamAllowed)
        nativeState = nativeAdapter.refreshContent(nativeState, SampleDataFixture.remoteOnlyInventory)

        assertEquals(mockState, nativeState)
    }

    @Test
    fun nativeAdapterMatchesMockContractForInstalledOffline() {
        val nativeAdapter = NativeAppCoreAdapter(
            catalog = SampleDataFixture.catalog,
            bridge = FakeNativeBridge(json),
            json = json,
        )

        var mockState = appCore.replaceFlightPlan(AppState(), SampleDataFixture.catalog, SampleDataFixture.samplePlan)
        mockState = appCore.setContentPolicy(mockState, ContentPolicy.OfflineRequired)
        mockState = appCore.refreshContent(mockState, SampleDataFixture.installedInventory)

        var nativeState = nativeAdapter.replaceFlightPlan(AppState(), SampleDataFixture.catalog, SampleDataFixture.samplePlan)
        nativeState = nativeAdapter.setContentPolicy(nativeState, ContentPolicy.OfflineRequired)
        nativeState = nativeAdapter.refreshContent(nativeState, SampleDataFixture.installedInventory)

        assertEquals(mockState, nativeState)
    }

    @Test
    fun nativeStateJsonIncludesContentPolicyEvenAtDefault() {
        val encoded = json.encodeToString(AppState().toWireForTesting())

        assertTrue(encoded.contains("\"content_policy\""))
    }

    @Test
    fun navRefWireShapeMatchesRustEnumContract() {
        val encoded = json.encodeToString(WireNavRefSerializer, WireNavRef.Airport("KBOS"))

        assertEquals("""{"Airport":"KBOS"}""", encoded)
    }

    @Test
    fun contentPolicyWireShapeMatchesRustEnumContract() {
        val encoded = json.encodeToString(WireContentPolicy.PreferLocal)

        assertEquals(""""PreferLocal"""", encoded)
    }

    @Test
    fun contentAvailabilityWireShapeMatchesRustEnumContract() {
        val encoded = json.encodeToString(WireContentAvailability.RemoteOnly)

        assertEquals(""""RemoteOnly"""", encoded)
    }
}

private object SampleDataFixture {
    val catalog = Catalog(
        cycle = "2026-04-16",
        packages = listOf(
            CatalogPackage(
                id = PackageId(
                    region = "ne",
                    family = "sectional",
                    cycle = "2026-04-16",
                ),
                packageName = "NE_SEC",
                regionId = "ne",
            ),
        ),
        plates = listOf(
            PlateRecord(
                airportId = "BOS",
                regionId = "ne",
            ),
        ),
    )

    val samplePlan = FlightPlan(
        id = "plan-1",
        name = "BOS local",
        legs = listOf(
            FlightPlanLeg(
                fromAirport = "BOS",
                toAirport = "BOS",
            ),
        ),
        departure = "BOS",
        destination = "BOS",
        alternate = null,
        cruiseAltitudeFt = 3000,
        notes = "Generated from preprocessor outputs",
        updatedAtEpochMs = 0,
        version = 1,
    )

    val remoteOnlyInventory = ContentInventory(
        installedPackages = emptyList(),
    )

    val installedInventory = ContentInventory(
        installedPackages = listOf(
            InstalledPackage(
                packageId = PackageId(
                    region = "ne",
                    family = "sectional",
                    cycle = "2026-04-16",
                ),
                integrityOk = true,
            ),
        ),
    )
}

private class FakeNativeBridge(
    private val json: Json,
) : NativeBridge {
    private val mock = MockAppCoreAdapter()

    override fun replaceFlightPlanStateJson(
        stateJson: String,
        catalogJson: String,
        planJson: String,
    ): String {
        val state = json.decodeFromString<WireAppState>(stateJson).toUiForTesting()
        val catalog = json.decodeFromString<WireCatalog>(catalogJson).toUiForTesting()
        val plan = json.decodeFromString<WireFlightPlan>(planJson).toUiForTesting()
        return json.encodeToString(mock.replaceFlightPlan(state, catalog, plan).toWireForTesting())
    }

    override fun setContentPolicyStateJson(
        stateJson: String,
        catalogJson: String,
        policyJson: String,
    ): String {
        val state = json.decodeFromString<WireAppState>(stateJson).toUiForTesting()
        val policy = json.decodeFromString<WireContentPolicy>(policyJson).toUi()
        return json.encodeToString(mock.setContentPolicy(state, policy).toWireForTesting())
    }

    override fun refreshContentStateJson(
        stateJson: String,
        catalogJson: String,
        inventoryJson: String,
    ): String {
        val state = json.decodeFromString<WireAppState>(stateJson).toUiForTesting()
        val inventory = json.decodeFromString<WireContentInventory>(inventoryJson).toUiForTesting()
        return json.encodeToString(mock.refreshContent(state, inventory).toWireForTesting())
    }
}

private fun WireContentPolicy.toUi() = when (this) {
    WireContentPolicy.OfflineRequired -> ContentPolicy.OfflineRequired
    WireContentPolicy.PreferLocal -> ContentPolicy.PreferLocal
    WireContentPolicy.StreamAllowed -> ContentPolicy.StreamAllowed
}

private fun WireContentInventory.toUiForTesting() = ContentInventory(
    installedPackages = installed_packages.map {
        InstalledPackage(
            packageId = it.package_id.toUiForTesting(),
            integrityOk = it.integrity_ok,
        )
    },
)

private fun WirePackageId.toUiForTesting() = PackageId(
    region = when (region) {
        WireRegionId.Ne -> "ne"
        WireRegionId.Nc -> "nc"
        WireRegionId.Nw -> "nw"
        WireRegionId.Se -> "se"
        WireRegionId.Sc -> "sc"
        WireRegionId.Sw -> "sw"
        WireRegionId.Ec -> "ec"
        WireRegionId.Ak -> "ak"
        WireRegionId.Pac -> "pac"
    },
    family = "sectional",
    cycle = cycle,
)
