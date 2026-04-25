package net.jonh.aerobag.prototype.domain

import android.content.Context
import android.util.Log

object SectionalPackages {
    private const val TAG = "AerobagTiles"

    fun isInstalled(context: Context, packageName: String): Boolean =
        InstalledPackages.isInstalled(context, InstalledPackageKind.Charts, packageName)

    fun loadTileBytes(
        context: Context,
        tile: RenderTile,
    ): ByteArray? {
        return when (tile.mapView.storageKind) {
            TileStorageKind.AssetTree ->
                error("asset-tree tile loading is no longer supported on Android")

            TileStorageKind.StaticProduct ->
                error("static-product tile loading is no longer supported on Android")

            TileStorageKind.SectionalPackage -> {
                val candidates = tile.candidateMapViews
                    .distinctBy { "${it.packageName}:${it.tileRoot}:${it.chartIndex}" }
                candidates.forEach { candidateMapView ->
                    val candidateName = candidateMapView.packageName ?: return@forEach
                    val installed = InstalledPackages.existingInstalledFile(
                        context,
                        InstalledPackageKind.Charts,
                        candidateName,
                    ) ?: return@forEach
                    if (!installed.isFile) {
                        return@forEach
                    }
                    val relativePath = tileRelativePath(tile, candidateMapView)
                    val bytes = runCatching {
                        PackageZipStore.readEntryBytes(installed, relativePath)
                    }.onFailure { error ->
                        logError(TAG, "zip read failed file=${installed.name} path=$relativePath", error)
                    }.getOrNull()
                    if (bytes != null) {
                        if (candidateMapView != tile.mapView) {
                            logWarn(
                                TAG,
                                "fallback hit requested=${tile.mapView.packageName} served=$candidateName path=$relativePath",
                            )
                        }
                        return bytes
                    }
                }
                logWarn(
                    TAG,
                    "tile unavailable across family package=${tile.mapView.packageName} zoom=${tile.zoom} x=${tile.x} y=${tile.yTms} candidates=${candidates.joinToString(",") { "${it.packageName}:${tileRelativePath(tile, it)}" }}",
                )
                null
            }
        }
    }
}

private fun logWarn(tag: String, message: String) {
    runCatching { Log.w(tag, message) }
}

private fun logError(tag: String, message: String, error: Throwable) {
    runCatching { Log.e(tag, message, error) }
}
