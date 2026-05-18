package org.aerobag.app.domain

import android.content.Context

object ChartPackages {
    fun loadPackageBytes(context: Context, packageId: String, sourceAssetPath: String): ByteArray? {
        val installed = InstalledPackages.existingInstalledFile(context, packageId) ?: return null
        if (!installed.isFile) {
            return null
        }
        return PackageZipStore.readEntryBytes(installed, sourceAssetPath)
    }

    fun loadChartBytes(context: Context, chart: ChartAsset): ByteArray? =
        loadPackageBytes(context, chart.packageId, chart.sourceAssetPath)

    fun loadThumbnailBytes(context: Context, chart: ChartAsset): ByteArray? {
        val thumbnailPath = chart.thumbnailSourceAssetPath ?: return null
        return loadPackageBytes(context, chart.packageId, thumbnailPath)
    }
}
