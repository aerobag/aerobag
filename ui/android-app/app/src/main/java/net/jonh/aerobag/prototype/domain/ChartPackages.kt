package net.jonh.aerobag.prototype.domain

import android.content.Context

object ChartPackages {
    fun loadPackageBytes(context: Context, packageId: String, sourceAssetPath: String): ByteArray? {
        val installed = InstalledPackages.existingInstalledFile(context, InstalledPackageKind.Plates, packageId) ?: return null
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
