package net.jonh.aerobag.prototype.domain

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
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
            catalogJson = SampleDataFixture.catalogJson,
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
            catalogJson = SampleDataFixture.catalogJson,
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

    @Test
    fun mockMapLookupFindsBostonTacAtInitialProbe() {
        val adapter = MockMapLookupAdapter(json)

        val chart = adapter.chartForPosition(
            catalogJson = SampleMapFixture.catalogJson,
            geometryJson = SampleMapFixture.geometryJson,
            family = SampleMapFixture.initialProbe.family,
            lat = SampleMapFixture.initialProbe.lat,
            lon = SampleMapFixture.initialProbe.lon,
        )

        assertEquals("Boston TAC", chart?.displayName)
    }

    @Test
    fun mockMapLookupReturnsNullOutsideCoverage() {
        val adapter = MockMapLookupAdapter(json)

        val chart = adapter.chartForPosition(
            catalogJson = SampleMapFixture.catalogJson,
            geometryJson = SampleMapFixture.geometryJson,
            family = SampleMapFixture.initialProbe.family,
            lat = SampleMapFixture.initialProbe.lat + 4.0,
            lon = SampleMapFixture.initialProbe.lon + 4.0,
        )

        assertNull(chart)
    }

    @Test
    fun tileViewportBuildsCenteredTmsGrid() {
        val cells = tileCells(
            MapTileView(
                chartFamily = MapChartFamily.Tac,
                chartName = "Boston TAC",
                chartIndex = 1,
                tileRoot = "charts-tac",
                zoom = 10,
                tileSize = 256,
                radius = 1,
                centerX = 310,
                centerYTms = 644,
                probeOffsetX = 0.18,
                probeOffsetY = 0.20,
            ),
        )

        assertEquals(MapTileCell(309, 645), cells.first())
        assertEquals(MapTileCell(310, 644), cells[4])
        assertEquals(MapTileCell(311, 643), cells.last())
    }

    @Test
    fun tileAssetPathMatchesCopiedPrototypeTileLayout() {
        val path = tileAssetPath(
            MapTileView(
                chartFamily = MapChartFamily.Tac,
                chartName = "Boston TAC",
                chartIndex = 1,
                tileRoot = "charts-tac",
                zoom = 10,
                tileSize = 256,
                radius = 1,
                centerX = 310,
                centerYTms = 644,
                probeOffsetX = 0.18,
                probeOffsetY = 0.20,
            ),
            MapTileCell(310, 644),
        )

        assertEquals("tiles/charts-tac/1/10/310/644.webp", path)
    }
}

private object SampleDataFixture {
    private val json = Json {
        encodeDefaults = true
        ignoreUnknownKeys = true
    }

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

    val catalogJson = json.encodeToString(catalog.toWireForTesting())

    val samplePlan = FlightPlan(
        id = "plan-1",
        name = "BOS local",
        legs = listOf(
            FlightPlanLeg(
                from = NavRef.Airport("BOS"),
                to = NavRef.Airport("BOS"),
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

private object SampleMapFixture {
    val catalogJson =
        """
        {
          "schema_version": 1,
          "cycle": "2026-04-16",
          "catalog_revision": "2026-04-06T00:00:00Z",
          "families": [
            {
              "id": "sectional",
              "display_name": "VFR Sectional Charts",
              "kind": "tiled_raster",
              "max_zoom": 10,
              "tile_size": 512
            },
            {
              "id": "tac",
              "display_name": "Terminal Area Charts",
              "kind": "tiled_raster",
              "max_zoom": 11,
              "tile_size": 512
            }
          ],
          "regions": [
            {
              "id": "ne",
              "display_name": "Northeast",
              "sort_order": 0
            }
          ],
          "packages": [],
          "charts": [
            {
              "id": {
                "family": "tac",
                "name": "Boston TAC",
                "cycle": "2026-04-16"
              },
              "family_id": "tac",
              "name": "Boston TAC",
              "display_name": "Boston TAC",
              "cycle": "2026-04-16",
              "region_ids": ["ne"],
              "max_zoom": 11,
              "tile_path_template": "tiles/charts-tac/boston/{z}/{x}/{y}",
              "coverage": {
                "kind": "polygon_ref",
                "value": {
                  "polygon_id": "tac:boston"
                }
              }
            }
          ],
          "plates": [],
          "supplements": []
        }
        """.trimIndent()

    val geometryJson =
        """
        {
          "schema_version": 1,
          "polygons": [
            {
              "id": "tac:boston",
              "points": [
                [-72.0, 41.0],
                [-70.0, 41.0],
                [-70.0, 43.0],
                [-72.0, 43.0],
                [-72.0, 41.0]
              ]
            }
          ]
        }
        """.trimIndent()

    val initialProbe = MapProbe(
        family = MapChartFamily.Tac,
        lat = 42.0,
        lon = -71.0,
    )
}

private class FakeNativeBridge(
    private val json: Json,
) : NativeBridge {
    private val mock = MockAppCoreAdapter()

    override fun createUiSessionJson(
        catalogJson: String,
        resourceIndexJson: String,
        planJson: String,
        recentAirportIdsJson: String,
        selectedAirportIdJson: String,
        selectedChartIdJson: String,
    ): String {
        return """{"handle":1,"chart_catalog":{"airports":[]},"snapshot":{"app_state":{"active_plan":null,"content_policy":"PreferLocal","last_content_requirements":[],"last_content_report":null},"chart_page_state":{"ordered_airport_ids":[],"recent_airport_ids":[],"selected_airport_id":"","selected_chart_id":""}}}"""
    }

    override fun removeLegInSessionJson(handle: Long, index: Int): String = getSessionSnapshotJson(handle)

    override fun moveWaypointInSessionJson(handle: Long, waypointIndex: Int, delta: Int): String =
        getSessionSnapshotJson(handle)

    override fun setSituationInSessionJson(handle: Long, situationJson: String): String =
        getSessionSnapshotJson(handle)

    override fun selectAirportInSessionJson(handle: Long, airportIdJson: String): String = getSessionSnapshotJson(handle)

    override fun selectChartInSessionJson(handle: Long, chartIdJson: String): String = getSessionSnapshotJson(handle)

    override fun getSessionSnapshotJson(handle: Long): String {
        return """{"app_state":{"active_plan":null,"content_policy":"PreferLocal","last_content_requirements":[],"last_content_report":null},"chart_page_state":{"ordered_airport_ids":[],"recent_airport_ids":[],"selected_airport_id":"","selected_chart_id":""}}"""
    }

    override fun restoreChartPageStateInSessionJson(
        handle: Long,
        recentAirportIdsJson: String,
        selectedAirportIdJson: String,
        selectedChartIdJson: String,
    ): String = getSessionSnapshotJson(handle)

    override fun destroySession(handle: Long) {}

    override fun deriveChartPageJson(
        resourceIndexJson: String,
        planJson: String,
    ): String {
        return """{"airports":[]}"""
    }

    override fun deriveChartPageStateJson(
        resourceIndexJson: String,
        planJson: String,
        recentAirportIdsJson: String,
        selectedAirportIdJson: String,
        selectedChartIdJson: String,
    ): String {
        return """{"airports":[],"recent_airport_ids":[],"selected_airport_id":"","selected_chart_id":""}"""
    }

    override fun removeFlightPlanLegJson(
        planJson: String,
        index: Int,
    ): String {
        val plan = json.decodeFromString<WireFlightPlan>(planJson).toUiForTesting()
        require(index in plan.legs.indices) { "Flight plan leg index out of range: $index" }
        val legs = plan.legs.filterIndexed { legIndex, _ -> legIndex != index }
        require(legs.isNotEmpty()) { "Flight plan must contain at least one leg" }
        return json.encodeToString(
            plan.copy(
                legs = legs,
                departure = (legs.firstOrNull()?.from as? NavRef.Airport)?.code,
                destination = (legs.lastOrNull()?.to as? NavRef.Airport)?.code,
                updatedAtEpochMs = plan.updatedAtEpochMs + 1,
                version = plan.version + 1,
            ).toWireForTesting(),
        )
    }

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

    override fun chartForPositionJson(
        catalogJson: String,
        geometryJson: String,
        familyJson: String,
        lat: Double,
        lon: Double,
    ): String {
        val family = when (json.decodeFromString<WireChartFamilyId>(familyJson)) {
            WireChartFamilyId.Sectional -> MapChartFamily.Sectional
            WireChartFamilyId.Tac -> MapChartFamily.Tac
            WireChartFamilyId.IfrLow -> MapChartFamily.IfrLow
            WireChartFamilyId.IfrHigh -> MapChartFamily.IfrHigh
        }
        val chart = MockMapLookupAdapter(json).chartForPosition(
            catalogJson = catalogJson,
            geometryJson = geometryJson,
            family = family,
            lat = lat,
            lon = lon,
        )
        return chart?.let {
            json.encodeToString(
                WireChartRecord(
                    id = WireChartId(
                        family = family.toWireFamilyForTesting(),
                        name = it.name,
                        cycle = "2026-04-16",
                    ),
                    family_id = family.toWireFamilyForTesting(),
                    name = it.name,
                    display_name = it.displayName,
                    cycle = "2026-04-16",
                    region_ids = listOf(WireRegionId.Ne),
                    max_zoom = 11,
                    tile_path_template = "tiles/charts-tac/boston/{z}/{x}/{y}",
                ),
            )
        } ?: "null"
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

private fun MapChartFamily.toWireFamilyForTesting() = when (this) {
    MapChartFamily.Sectional -> WireChartFamilyId.Sectional
    MapChartFamily.Tac -> WireChartFamilyId.Tac
    MapChartFamily.IfrLow -> WireChartFamilyId.IfrLow
    MapChartFamily.IfrHigh -> WireChartFamilyId.IfrHigh
}
