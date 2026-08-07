// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app.domain

import android.content.Context
import java.io.File
import java.io.InputStream
import kotlinx.serialization.Serializable
import kotlinx.serialization.decodeFromString
import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.Json

private const val InstalledPackagesDirectoryName = "packages"

@Serializable
internal data class InstalledArtifactMetadata(
    val artifactId: String,
    val filename: String,
    val sizeBytes: Long? = null,
    val checksumSha256: String? = null,
    val familyId: String? = null,
    val regionId: String? = null,
    val chartPackageTier: String? = null,
)

private val InstalledArtifactJson = Json {
    ignoreUnknownKeys = true
    encodeDefaults = true
}

data class InstalledPackageArtifact(
    val artifactId: String,
    val filename: String,
    val file: File,
    val sizeBytes: Long? = null,
    val checksumSha256: String? = null,
    val familyId: String? = null,
    val regionId: String? = null,
    val chartPackageTier: String? = null,
)

data class InstalledPackageStorageStats(
    val availableBytes: Long,
    val totalBytes: Long?,
)

object InstalledPackages {
    fun internalPackageFile(context: Context, filename: String): File =
        File(File(context.filesDir, InstalledPackagesDirectoryName), filename)

    fun packageStorageStats(context: Context): InstalledPackageStorageStats {
        val directory = File(context.filesDir, InstalledPackagesDirectoryName)
        directory.mkdirs()
        return InstalledPackageStorageStats(
            availableBytes = directory.usableSpace.coerceAtLeast(0L),
            totalBytes = directory.totalSpace.takeIf { it > 0L },
        )
    }

    private fun externalFile(context: Context, filename: String): File? =
        context.getExternalFilesDir(null)?.let { File(File(it, InstalledPackagesDirectoryName), filename) }

    private fun metadataFile(zipFile: File): File =
        File(zipFile.parentFile, "${zipFile.name}.metadata.json")

    private fun writeMetadata(zipFile: File, metadata: InstalledArtifactMetadata) {
        val target = metadataFile(zipFile)
        target.parentFile?.mkdirs()
        val temp = File(target.parentFile, "${target.name}.tmp")
        temp.writeText(encodeInstalledArtifactMetadata(metadata))
        if (!temp.renameTo(target)) {
            temp.copyTo(target, overwrite = true)
            temp.delete()
        }
    }

    private fun readMetadata(zipFile: File): InstalledArtifactMetadata? =
        metadataFile(zipFile)
            .takeIf { it.isFile }
            ?.let { file ->
                runCatching { decodeInstalledArtifactMetadata(file.readText()) }.getOrNull()
            }

    fun listInstalledArtifacts(context: Context): List<InstalledPackageArtifact> {
        val directories = listOfNotNull(
            context.getExternalFilesDir(null)?.let { File(it, InstalledPackagesDirectoryName) },
            File(context.filesDir, InstalledPackagesDirectoryName),
        )
        val artifactsByFilename = linkedMapOf<String, InstalledPackageArtifact>()
        directories
            .filter { it.isDirectory }
            .flatMap { dir -> dir.listFiles()?.asList().orEmpty() }
            .filter { it.isFile && it.extension == "zip" }
            .sortedBy { it.name }
            .forEach { zipFile ->
                val metadata = readMetadata(zipFile) ?: return@forEach
                val artifact = InstalledPackageArtifact(
                    artifactId = metadata.artifactId,
                    filename = metadata.filename,
                    file = zipFile,
                    sizeBytes = metadata.sizeBytes ?: zipFile.length(),
                    checksumSha256 = metadata.checksumSha256,
                    familyId = metadata.familyId,
                    regionId = metadata.regionId,
                    chartPackageTier = metadata.chartPackageTier,
                )
                artifactsByFilename.putIfAbsent(artifact.filename, artifact)
            }
        return artifactsByFilename.values.sortedBy { it.filename }
    }

    fun existingInstalledArtifacts(context: Context, artifactId: String): List<InstalledPackageArtifact> =
        listInstalledArtifacts(context)
            .filter { it.artifactId == artifactId }
            .sortedWith(compareByDescending<InstalledPackageArtifact> { it.file.lastModified() }.thenByDescending { it.filename })

    fun existingInstalledFile(context: Context, artifactId: String): File? =
        existingInstalledArtifacts(context, artifactId)
            .firstOrNull()
            ?.file

    fun installedFile(context: Context, artifactId: String): File =
        existingInstalledFile(context, artifactId)
            ?: error("missing installed package $artifactId")

    fun replaceInstalledFile(
        context: Context,
        artifactId: String,
        filename: String,
        bytes: ByteArray,
        sizeBytes: Long? = null,
        checksumSha256: String? = null,
        familyId: String? = null,
        regionId: String? = null,
        chartPackageTier: String? = null,
    ) {
        val target = internalPackageFile(context, filename)
        target.parentFile?.mkdirs()
        PackageZipStore.invalidate(target)
        val temp = File(target.parentFile, "${target.name}.tmp")
        temp.outputStream().use { it.write(bytes) }
        if (!temp.renameTo(target)) {
            temp.copyTo(target, overwrite = true)
            temp.delete()
        }
        writeMetadata(
            zipFile = target,
            metadata = InstalledArtifactMetadata(
                artifactId = artifactId,
                filename = filename,
                sizeBytes = sizeBytes ?: target.length(),
                checksumSha256 = checksumSha256,
                familyId = familyId,
                regionId = regionId,
                chartPackageTier = chartPackageTier,
            ),
        )
    }

    fun replaceInstalledFileFromStream(
        context: Context,
        artifactId: String,
        filename: String,
        source: InputStream,
        sizeBytes: Long? = null,
        checksumSha256: String? = null,
        familyId: String? = null,
        regionId: String? = null,
        chartPackageTier: String? = null,
    ) {
        val target = internalPackageFile(context, filename)
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
        writeMetadata(
            zipFile = target,
            metadata = InstalledArtifactMetadata(
                artifactId = artifactId,
                filename = filename,
                sizeBytes = sizeBytes ?: target.length(),
                checksumSha256 = checksumSha256,
                familyId = familyId,
                regionId = regionId,
                chartPackageTier = chartPackageTier,
            ),
        )
    }

    fun updateInstalledArtifactGrouping(
        context: Context,
        artifactId: String,
        filename: String,
        familyId: String,
        regionId: String?,
        chartPackageTier: String?,
    ): Boolean {
        val artifact = listInstalledArtifacts(context)
            .firstOrNull { it.artifactId == artifactId && it.filename == filename }
            ?: return false
        val metadata = readMetadata(artifact.file) ?: return false
        if (
            metadata.familyId == familyId &&
            metadata.regionId == regionId &&
            metadata.chartPackageTier == chartPackageTier
        ) {
            return false
        }
        writeMetadata(
            artifact.file,
            metadata.copy(
                familyId = familyId,
                regionId = regionId,
                chartPackageTier = chartPackageTier,
            ),
        )
        return true
    }

    fun deleteInstalledArtifact(
        context: Context,
        artifactId: String,
        filename: String,
        keepFilename: String? = null,
    ) {
        existingInstalledArtifacts(context, artifactId)
            .filter { it.filename == filename }
            .filterNot { it.filename == keepFilename }
            .forEach { artifact ->
                PackageZipStore.invalidate(artifact.file)
                artifact.file.delete()
                metadataFile(artifact.file).delete()
            }
    }

    fun listInstalledPackageIds(context: Context): List<String> {
        return listInstalledArtifacts(context)
            .map { it.artifactId }
            .distinct()
            .sorted()
            .toList()
    }

    fun readZipEntryText(context: Context, packageId: String, entryName: String): String =
        readZipEntryBytes(context, packageId, entryName).decodeToString()

    fun readZipEntryBytes(context: Context, packageId: String, entryName: String): ByteArray {
        val installed = existingInstalledFile(context, packageId)
            ?: error("missing installed package $packageId")
        return readZipEntryBytes(installed, entryName)
    }

    fun readZipEntryBytes(zipFile: File, entryName: String): ByteArray {
        return PackageZipStore.readEntryBytes(zipFile, entryName)
            ?: error("missing $entryName in ${zipFile.absolutePath}")
    }
}

internal fun encodeInstalledArtifactMetadata(metadata: InstalledArtifactMetadata): String =
    InstalledArtifactJson.encodeToString(metadata)

internal fun decodeInstalledArtifactMetadata(value: String): InstalledArtifactMetadata =
    InstalledArtifactJson.decodeFromString(value)
