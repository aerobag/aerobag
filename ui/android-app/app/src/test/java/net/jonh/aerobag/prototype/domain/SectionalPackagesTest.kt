package net.jonh.aerobag.prototype.domain

import org.junit.Assert.assertEquals
import org.junit.Test

class SectionalPackagesTest {
    @Test
    fun sectionalTilesUseZipEntryPathsWithoutAssetPrefix() {
        val mapView = MapView(
            chartFamily = MapChartFamily.Sectional,
            chartName = "NW Sectional",
            chartIndex = 0,
            tileRoot = "tiles",
            tileUrlRoot = "/sectional-packages/NW_SEC/tiles",
            tileSize = 256,
            minZoom = 4.2,
            maxZoom = 10.8,
            storageKind = TileStorageKind.SectionalPackage,
            packageName = "NW_SEC",
            initialViewport = MapViewportSeed(
                lat = 47.0,
                lon = -121.0,
                zoom = 7.2,
            ),
            levels = listOf(
                TileLevelAvailability(
                    zoom = 10,
                    xMin = 156,
                    xMax = 219,
                    yTmsMin = 636,
                    yTmsMax = 672,
                ),
            ),
        )
        val tile = RenderTile(
            x = 198,
            yTms = 650,
            leftPx = 0f,
            topPx = 0f,
            sizePx = 256f,
            zoom = 10,
        )

        assertEquals("tiles/0/10/198/650.webp", tileRelativePath(mapView, tile))
        assertEquals("tiles/tiles/0/10/198/650.webp", tileAssetPath(mapView, tile))
    }
}
