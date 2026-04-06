package net.jonh.aerobag.prototype.domain

import android.content.Context
import java.io.File
import java.util.zip.ZipFile

object SectionalPackages {
    private const val ASSET_DIR = "sectional-packages"
    private const val INSTALL_DIR = "sectional-packages"

    fun installedFile(context: Context, packageName: String): File =
        File(File(context.filesDir, INSTALL_DIR), "$packageName.zip")

    fun isInstalled(context: Context, packageName: String): Boolean =
        installedFile(context, packageName).isFile

    fun install(context: Context, packageName: String): File {
        val target = installedFile(context, packageName)
        if (target.isFile) {
            return target
        }
        target.parentFile?.mkdirs()
        context.assets.open("$ASSET_DIR/$packageName.zip").use { input ->
            target.outputStream().use { output -> input.copyTo(output) }
        }
        return target
    }

    fun loadTileBytes(context: Context, mapView: MapView, tile: RenderTile): ByteArray? {
        return when (mapView.storageKind) {
            TileStorageKind.AssetTree ->
                runCatching {
                    context.assets.open(tileAssetPath(mapView, tile)).use { it.readBytes() }
                }.getOrNull()

            TileStorageKind.SectionalPackage -> {
                val packageName = mapView.packageName ?: return null
                val installed = installedFile(context, packageName)
                if (!installed.isFile) {
                    return null
                }
                ZipFile(installed).use { zip ->
                    val entry = zip.getEntry(tileRelativePath(mapView, tile)) ?: return null
                    zip.getInputStream(entry).use { stream -> stream.readBytes() }
                }
            }
        }
    }
}
