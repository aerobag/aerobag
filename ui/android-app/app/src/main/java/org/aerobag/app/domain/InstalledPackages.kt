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
private data class InstalledArtifactMetadata(
    val artifactId: String,
    val filename: String,
    val sizeBytes: Long? = null,
    val checksumSha256: String? = null,
)

data class InstalledPackageArtifact(
    val artifactId: String,
    val filename: String,
    val file: File,
    val sizeBytes: Long? = null,
    val checksumSha256: String? = null,
)

object InstalledPackages {
    private val json = Json {
        ignoreUnknownKeys = true
        encodeDefaults = true
    }

    fun internalPackageFile(context: Context, filename: String): File =
        File(File(context.filesDir, InstalledPackagesDirectoryName), filename)

    private fun externalFile(context: Context, filename: String): File? =
        context.getExternalFilesDir(null)?.let { File(File(it, InstalledPackagesDirectoryName), filename) }

    private fun metadataFile(zipFile: File): File =
        File(zipFile.parentFile, "${zipFile.name}.metadata.json")

    private fun writeMetadata(zipFile: File, metadata: InstalledArtifactMetadata) {
        val target = metadataFile(zipFile)
        target.parentFile?.mkdirs()
        val temp = File(target.parentFile, "${target.name}.tmp")
        temp.writeText(json.encodeToString(metadata))
        if (!temp.renameTo(target)) {
            temp.copyTo(target, overwrite = true)
            temp.delete()
        }
    }

    private fun readMetadata(zipFile: File): InstalledArtifactMetadata? =
        metadataFile(zipFile)
            .takeIf { it.isFile }
            ?.let { file ->
                runCatching { json.decodeFromString<InstalledArtifactMetadata>(file.readText()) }.getOrNull()
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
            ),
        )
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
