package org.aerobag.app

import java.io.File
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class ChartAssetLoadKeyTest {
    @Test
    fun chartAssetLoadKeyChangesWhenDataRevisionChanges() {
        val first = chartAssetLoadKey(
            chartId = "Plate:KPLU:IAP-RNAV-35.png",
            dataRevision = 0,
        )
        val afterDataChange = chartAssetLoadKey(
            chartId = "Plate:KPLU:IAP-RNAV-35.png",
            dataRevision = 1,
        )

        assertNotEquals(
            "The plate bitmap producer must reload when chart data/resources change even if the selected chart id is unchanged.",
            first,
            afterDataChange,
        )
    }

    @Test
    fun chartAssetLoadKeyDoesNotChangeForIdenticalInputs() {
        assertEquals(
            chartAssetLoadKey(
                chartId = "Plate:KPLU:IAP-RNAV-35.png",
                dataRevision = 7,
            ),
            chartAssetLoadKey(
                chartId = "Plate:KPLU:IAP-RNAV-35.png",
                dataRevision = 7,
            ),
        )
    }

    @Test
    fun chartsPageBitmapProducerIsKeyedByDataRevision() {
        val source = sourceFile("src/main/java/org/aerobag/app/ChartsPage.kt").readText()

        assertTrue(
            "ChartsPage must include the data revision in the plate bitmap producer key.",
            source.contains("val bitmapLoadKey = chartAssetLoadKey(selectedChart?.id, chartAssetDataRevision)") &&
                source.contains("bitmapLoadKey,"),
        )
        assertFalse(
            "ChartsPage must not poll for plate bitmap availability after a failed load.",
            source.contains("nextChartAssetRetryRevisionAfterFailure") ||
                source.contains("chartAssetRetryDelayMs") ||
                source.contains("delay(chartAsset"),
        )
    }

    @Test
    fun chartViewportInitializerRerunsWhenViewportIsClearedAfterBitmapLoads() {
        val source = sourceFile("src/main/java/org/aerobag/app/ChartsPage.kt").readText()

        assertTrue(
            "Android plate rendering must mirror web: viewport initialization has to rerun when " +
                "the parent clears chartViewport after a plate asset and surface are already ready.",
            source.contains("LaunchedEffect(bitmap, surfaceSize, viewport)"),
        )
    }

    @Test
    fun offlinePackageSyncCompletionBumpsChartAssetDataRevision() {
        val mainActivity = sourceFile("src/main/java/org/aerobag/app/MainActivity.kt").readText()
        val homePage = sourceFile("src/main/java/org/aerobag/app/HomePage.kt").readText()

        assertTrue(
            "MainActivity must pass package data changes into the chart asset load key.",
            mainActivity.contains("var chartAssetDataRevision by remember") &&
                mainActivity.contains("chartAssetDataRevision = chartAssetDataRevision + 1") &&
                mainActivity.contains("chartAssetDataRevision = chartAssetDataRevision,"),
        )
        assertTrue(
            "Offline package sync completion is the deterministic signal that package-backed chart assets may have changed.",
            homePage.contains("onOfflinePackageArtifactsChanged") &&
                homePage.contains("event is OfflinePackagesControllerEventWire.SyncFinished") &&
                homePage.contains("event.summary.fetchedCount > 0 || event.summary.gcCount > 0"),
        )
    }

    @Test
    fun failedChartAssetLoadWaitsForDataRevisionForSameChart() {
        val first = chartAssetLoadKey(
            chartId = "Plate:KPLU:IAP-RNAV-35.png",
            dataRevision = 0,
        )
        val sameData = chartAssetLoadKey(
            chartId = first.chartId,
            dataRevision = first.dataRevision,
        )
        val afterDataReady = chartAssetLoadKey(
            chartId = first.chartId,
            dataRevision = first.dataRevision + 1,
        )

        assertEquals(first, sameData)
        assertEquals("Plate:KPLU:IAP-RNAV-35.png", afterDataReady.chartId)
        assertEquals(1, afterDataReady.dataRevision)
        assertNotEquals(
            "A transient blank plate load must reload when package-backed data changes without requiring the user to select a different plate first.",
            first,
            afterDataReady,
        )
    }

    private fun sourceFile(path: String): File {
        val start = File(".").canonicalFile
        return generateSequence(start) { it.parentFile }
            .map { File(it, path) }
            .firstOrNull { it.isFile }
            ?: error("could not locate source file $path from $start")
    }
}
