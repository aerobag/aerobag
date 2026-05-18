package org.aerobag.app.domain

import android.content.Context
import android.util.Log

object SectionalPackages {
    private const val TAG = "AerobagTiles"

    fun isInstalled(context: Context, packageName: String): Boolean =
        InstalledPackages.existingInstalledFile(context, packageName) != null

    fun loadTileBytes(
        context: Context,
        tile: RenderTile,
    ): ByteArray? {
        tile.sources
            .distinctBy { "${it.packageName}:${it.storageKind}:${it.path}" }
            .forEach { candidate ->
                when (candidate.storageKind) {
                    TileStorageKind.AssetTree ->
                        error("asset-tree tile loading is no longer supported on Android")

                    TileStorageKind.StaticProduct,
                    TileStorageKind.SectionalPackage -> {
                        val candidateName = candidate.packageName ?: return@forEach
                        val installed = InstalledPackages.existingInstalledFile(
                            context,
                            candidateName,
                        ) ?: return@forEach
                        if (!installed.isFile) {
                            return@forEach
                        }
                        val bytes = runCatching {
                            PackageZipStore.readEntryBytes(installed, candidate.path)
                        }.onFailure { error ->
                            logError(TAG, "zip read failed file=${installed.name} path=${candidate.path}", error)
                        }.getOrNull()
                        if (bytes != null) {
                            return bytes
                        }
                    }
                }
            }
        logWarn(
            TAG,
            "tile unavailable across core sources zoom=${tile.zoom} x=${tile.x} y=${tile.yTms} candidates=${tile.sources.joinToString(",") { "${it.packageName}:${it.path}" }}",
        )
        return null
    }
}

private fun logWarn(tag: String, message: String) {
    runCatching { Log.w(tag, message) }
}

private fun logError(tag: String, message: String, error: Throwable) {
    runCatching { Log.e(tag, message, error) }
}
