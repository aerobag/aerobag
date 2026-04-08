package net.jonh.aerobag.prototype.domain

import android.content.Context
import java.io.File

object ChartPackages {
    private const val INSTALL_DIR = "chart-packages"
    private val packageStore = ZipPackageStore()

    private fun internalInstalledFile(context: Context, packageId: String): File =
        File(File(context.filesDir, INSTALL_DIR), "$packageId.zip")

    private fun externalInstalledFile(context: Context, packageId: String): File? =
        context.getExternalFilesDir(null)?.let { File(File(it, INSTALL_DIR), "$packageId.zip") }

    private fun existingInstalledFile(context: Context, packageId: String): File? {
        val external = externalInstalledFile(context, packageId)
        if (external?.isFile == true) {
            return external
        }
        val internal = internalInstalledFile(context, packageId)
        if (internal.isFile) {
            return internal
        }
        return null
    }

    fun loadChartBytes(context: Context, chart: ChartAsset): ByteArray? {
        val installed = existingInstalledFile(context, chart.packageId) ?: return null
        if (!installed.isFile) {
            return null
        }
        return packageStore.loadTileBytes(installed, chart.sourceAssetPath)
    }
}
