package net.jonh.aerobag.prototype.domain

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File
import java.nio.file.Files
import java.util.zip.ZipEntry
import java.util.zip.ZipOutputStream

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

    @Test
    fun zipPackageStoreCachesOpenZipAndEntryIndex() {
        val zipFile = createZip(
            "tiles/0/10/198/650.webp" to "tile-a".encodeToByteArray(),
            "tiles/0/10/198/651.webp" to "tile-b".encodeToByteArray(),
        )
        val store = ZipPackageStore()

        val first = store.loadTileBytes(zipFile, "tiles/0/10/198/650.webp")
        val firstIdentity = store.cachedIdentity(zipFile)

        val second = store.loadTileBytes(zipFile, "tiles/0/10/198/651.webp")
        val secondIdentity = store.cachedIdentity(zipFile)

        assertEquals("tile-a", first?.decodeToString())
        assertEquals("tile-b", second?.decodeToString())
        assertEquals(2, store.cachedEntryCount(zipFile))
        assertEquals(firstIdentity, secondIdentity)
    }

    @Test
    fun zipPackageStoreReloadsCacheWhenZipChanges() {
        val zipFile = createZip("tiles/0/10/198/650.webp" to "old".encodeToByteArray())
        val store = ZipPackageStore()

        assertEquals("old", store.loadTileBytes(zipFile, "tiles/0/10/198/650.webp")?.decodeToString())
        val firstIdentity = store.cachedIdentity(zipFile)

        rewriteZip(zipFile, "tiles/0/10/198/650.webp" to "new".encodeToByteArray())

        assertEquals("new", store.loadTileBytes(zipFile, "tiles/0/10/198/650.webp")?.decodeToString())
        assertNotEquals(firstIdentity, store.cachedIdentity(zipFile))
    }

    @Test
    fun zipPackageStoreUsesEntryIndexToShortCircuitMissingPaths() {
        val zipFile = createZip("tiles/0/10/198/650.webp" to "tile".encodeToByteArray())
        val store = ZipPackageStore()

        assertNull(store.loadTileBytes(zipFile, "tiles/0/10/198/999.webp"))
        assertTrue((store.cachedEntryCount(zipFile) ?: 0) >= 1)
    }

    private fun createZip(vararg entries: Pair<String, ByteArray>): File {
        val directory = Files.createTempDirectory("sectional-packages-test").toFile()
        val zipFile = File(directory, "package.zip")
        writeZip(zipFile, *entries)
        return zipFile
    }

    private fun rewriteZip(zipFile: File, vararg entries: Pair<String, ByteArray>) {
        writeZip(zipFile, *entries)
        zipFile.setLastModified(System.currentTimeMillis() + 1_000)
    }

    private fun writeZip(zipFile: File, vararg entries: Pair<String, ByteArray>) {
        ZipOutputStream(zipFile.outputStream().buffered()).use { zip ->
            for ((name, bytes) in entries) {
                zip.putNextEntry(ZipEntry(name))
                zip.write(bytes)
                zip.closeEntry()
            }
        }
    }
}
