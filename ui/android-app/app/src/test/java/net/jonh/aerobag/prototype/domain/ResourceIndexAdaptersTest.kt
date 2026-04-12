package net.jonh.aerobag.prototype.domain

import org.junit.Assert.assertEquals
import org.junit.Test

class ResourceIndexAdaptersTest {
    private val resourceIndex = WireResourceIndex(
        schema_version = 1,
        cycle = "2604",
        generated_at_utc = "2026-04-07T00:00:00Z",
        families = listOf(
            WireResourceFamily("sec", "Sectional", "tiled_raster"),
            WireResourceFamily("tac", "TAC", "tiled_raster"),
            WireResourceFamily("tpp", "TPP", "flat_image"),
            WireResourceFamily("csup", "CSUP", "flat_image"),
        ),
        regions = listOf(
            WireCatalogRegion(WireRegionId.Nw, "Northwest", 0),
            WireCatalogRegion(WireRegionId.Ne, "Northeast", 1),
        ),
        packages = emptyList(),
        chart_collections = listOf(
            WireChartCollection(
                id = "sec:nw",
                family_id = WireChartFamilyId.Sec,
                region_id = WireRegionId.Nw,
                package_id = "NW_SEC",
                chart_index = 0,
                tile_path_template = "tiles/0/{z}/{x}/{y}.webp",
                levels = listOf(WireChartCollectionLevel(10, 1, 2, 3, 4)),
                coverage_bounds = WireCoverageBounds(1.0, 2.0, 3.0, 4.0),
                default_view = WireDefaultView(45.0, -122.0, 8.0),
            ),
            WireChartCollection(
                id = "tac:nw",
                family_id = WireChartFamilyId.Tac,
                region_id = WireRegionId.Nw,
                package_id = "NW_TAC",
                chart_index = 1,
                tile_path_template = "tiles/1/{z}/{x}/{y}.webp",
                levels = listOf(WireChartCollectionLevel(11, 5, 6, 7, 8)),
                coverage_bounds = WireCoverageBounds(1.0, 2.0, 3.0, 4.0),
                default_view = WireDefaultView(46.0, -123.0, 9.0),
            ),
        ),
        airports = emptyList(),
        airport_resources = listOf(
            WireAirportResources(
                airport_id = "BOS",
                plate_ids = listOf("plate:BOS:IAP-MA-ILS OR LOC RWY 04R.png"),
                csup_ids = listOf("csup:BOS:CSUP-NE_0-0.png"),
                package_ids = listOf("NE_CSUP", "NE_TPP"),
            ),
        ),
        plates = listOf(
            WireResourcePlate(
                id = "plate:BOS:IAP-MA-ILS OR LOC RWY 04R.png",
                airport_id = "BOS",
                region_id = WireRegionId.Ne,
                package_id = "NE_TPP",
                asset_path = "plates/BOS/IAP-MA-ILS OR LOC RWY 04R.png",
                label = "IAP-MA-ILS OR LOC RWY 04R",
                asset_kind = "png",
                document_type = "approach",
            ),
        ),
        csups = listOf(
            WireResourceCsup(
                id = "csup:BOS:CSUP-NE_0-0.png",
                airport_id = "BOS",
                region_id = WireRegionId.Ne,
                package_id = "NE_CSUP",
                asset_path = "afd/BOS/CSUP-NE_0-0.png",
                label = "CSUP-NE_0-0",
                asset_kind = "png",
                document_type = "csup",
            ),
        ),
    )

    private val samplePlan = FlightPlan(
        id = "plan-1",
        name = "test",
        legs = listOf(FlightPlanLeg(NavRef.Airport("BOS"), NavRef.Airport("BOS"), null)),
        departure = "BOS",
        destination = "BOS",
        alternate = null,
        cruiseAltitudeFt = null,
        notes = null,
        updatedAtEpochMs = 0,
        version = 1,
    )

    @Test
    fun deriveMapViewsUsesPreferredCollectionIds() {
        val mapViews = deriveMapViews(resourceIndex, listOf("sec:nw", "tac:nw"))
        assertEquals(listOf("sec:nw", "tac:nw"), mapViews.map { it.id })
        assertEquals("NW_SEC", mapViews[0].mapView.packageName)
        assertEquals(MapChartFamily.Tac, mapViews[1].mapView.chartFamily)
        assertEquals(512, mapViews[0].mapView.tileSize)
        assertEquals(512, mapViews[1].mapView.tileSize)
    }

    @Test
    fun deriveChartPageBuildsAssetsFromPlatesAndCsups() {
        val chartPage = deriveChartPage(
            resourceIndex = resourceIndex,
            samplePlan = samplePlan,
        )
        assertEquals(1, chartPage.airports.size)
        assertEquals("BOS", chartPage.airports[0].id)
        assertEquals(listOf("csup", "plate"), chartPage.airports[0].charts.map { it.kind })
        assertEquals(listOf("csup", "approach"), chartPage.airports[0].charts.map { it.folderCategory })
    }
}
