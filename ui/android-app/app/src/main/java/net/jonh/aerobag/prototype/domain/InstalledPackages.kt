package net.jonh.aerobag.prototype.domain

import android.content.Context
import java.io.File
import java.io.InputStream

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

    fun replaceInstalledFile(context: Context, kind: InstalledPackageKind, packageId: String, bytes: ByteArray) {
        val target = internalFile(context, kind, packageId)
        target.parentFile?.mkdirs()
        PackageZipStore.invalidate(target)
        val temp = File(target.parentFile, "${target.name}.tmp")
        temp.outputStream().use { it.write(bytes) }
        if (!temp.renameTo(target)) {
            temp.copyTo(target, overwrite = true)
            temp.delete()
        }
    }

    fun replaceInstalledFileFromStream(
        context: Context,
        kind: InstalledPackageKind,
        packageId: String,
        source: InputStream,
    ) {
        val target = internalFile(context, kind, packageId)
        target.parentFile?.mkdirs()
        PackageZipStore.invalidate(target)
        val temp = File(target.parentFile, "${target.name}.tmp")
        temp.outputStream().use { output ->
            source.copyTo(output)
        }
        if (!temp.renameTo(target)) {
            temp.copyTo(target, overwrite = true)
            temp.delete()
        }
    }

    fun deleteInstalledFile(context: Context, kind: InstalledPackageKind, packageId: String) {
        existingInstalledFile(context, kind, packageId)?.let { file ->
            PackageZipStore.invalidate(file)
            file.delete()
        }
    }

    fun isInstalled(context: Context, kind: InstalledPackageKind, packageId: String): Boolean =
        existingInstalledFile(context, kind, packageId) != null

    fun listInstalledPackageIds(context: Context, kind: InstalledPackageKind): List<String> {
        val directories = sequenceOf(
            File(context.filesDir, kind.directoryName),
            context.getExternalFilesDir(null)?.let { File(it, kind.directoryName) },
        ).filterNotNull()
        return directories
            .filter { it.isDirectory }
            .flatMap { dir -> dir.listFiles()?.asSequence().orEmpty() }
            .filter { it.isFile && it.extension == "zip" }
            .map { it.name.removeSuffix(".zip") }
            .distinct()
            .sorted()
            .toList()
    }

    fun readZipEntryText(context: Context, kind: InstalledPackageKind, packageId: String, entryName: String): String =
        readZipEntryBytes(context, kind, packageId, entryName).decodeToString()

    fun readZipEntryBytes(context: Context, kind: InstalledPackageKind, packageId: String, entryName: String): ByteArray {
        val installed = existingInstalledFile(context, kind, packageId)
            ?: error("missing installed ${kind.directoryName} package $packageId")
        return readZipEntryBytes(installed, entryName)
    }

    fun readZipEntryBytes(zipFile: File, entryName: String): ByteArray {
        return PackageZipStore.readEntryBytes(zipFile, entryName)
            ?: error("missing $entryName in ${zipFile.absolutePath}")
    }
}
