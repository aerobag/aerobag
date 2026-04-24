package net.jonh.aerobag.prototype.domain

import android.content.Context
import java.io.BufferedInputStream
import java.io.File
import java.util.zip.ZipInputStream

enum class InstalledPackageKind(val directoryName: String) {
    Charts("chart-packages"),
    Plates("plate-packages"),
    Data("data-packages"),
}

object InstalledPackages {
    private fun internalFile(context: Context, kind: InstalledPackageKind, packageId: String): File =
        File(File(context.filesDir, kind.directoryName), "$packageId.zip")

    private fun externalFile(context: Context, kind: InstalledPackageKind, packageId: String): File? =
        context.getExternalFilesDir(null)?.let { File(File(it, kind.directoryName), "$packageId.zip") }

    fun existingInstalledFile(context: Context, kind: InstalledPackageKind, packageId: String): File? {
        val external = externalFile(context, kind, packageId)
        if (external?.isFile == true) {
            return external
        }
        val internal = internalFile(context, kind, packageId)
        if (internal.isFile) {
            return internal
        }
        return null
    }

    fun installedFile(context: Context, kind: InstalledPackageKind, packageId: String): File =
        existingInstalledFile(context, kind, packageId)
            ?: internalFile(context, kind, packageId)

    fun isInstalled(context: Context, kind: InstalledPackageKind, packageId: String): Boolean =
        existingInstalledFile(context, kind, packageId) != null

    fun readZipEntryText(context: Context, kind: InstalledPackageKind, packageId: String, entryName: String): String =
        readZipEntryBytes(context, kind, packageId, entryName).decodeToString()

    fun readZipEntryBytes(context: Context, kind: InstalledPackageKind, packageId: String, entryName: String): ByteArray {
        val installed = existingInstalledFile(context, kind, packageId)
            ?: error("missing installed ${kind.directoryName} package $packageId")
        return readZipEntryBytes(installed, entryName)
    }

    fun readZipEntryBytes(zipFile: File, entryName: String): ByteArray {
        ZipInputStream(BufferedInputStream(zipFile.inputStream())).use { zipStream ->
            while (true) {
                val entry = zipStream.nextEntry ?: break
                if (!entry.isDirectory && entry.name == entryName) {
                    return zipStream.readBytes()
                }
            }
        }
        error("missing $entryName in ${zipFile.absolutePath}")
    }
}
