package net.jonh.aerobag.prototype.domain

import android.content.Context
import java.io.File
import java.util.Collections
import java.util.concurrent.ConcurrentHashMap
import java.util.zip.ZipFile

object SectionalPackages {
    private const val ASSET_DIR = "sectional-packages"
    private const val INSTALL_DIR = "sectional-packages"
    private val packageStore = ZipPackageStore()

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
        packageStore.invalidate(target)
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
                packageStore.loadTileBytes(installed, tileRelativePath(mapView, tile))
            }
        }
    }
}

internal class ZipPackageStore {
    private val openPackages = ConcurrentHashMap<String, OpenZipPackage>()

    fun loadTileBytes(file: File, relativePath: String): ByteArray? {
        val packageRef = packageFor(file)
        if (!packageRef.entries.contains(relativePath)) {
            return null
        }
        val entry = packageRef.zipFile.getEntry(relativePath) ?: return null
        return packageRef.zipFile.getInputStream(entry).use { it.readBytes() }
    }

    fun invalidate(file: File) {
        openPackages.remove(file.absolutePath)?.close()
    }

    internal fun cachedEntryCount(file: File): Int? =
        openPackages[file.absolutePath]?.takeIf { it.matches(file) }?.entries?.size

    internal fun cachedIdentity(file: File): Int? =
        openPackages[file.absolutePath]?.takeIf { it.matches(file) }?.identityHashCode

    private fun packageFor(file: File): OpenZipPackage {
        val path = file.absolutePath
        val existing = openPackages[path]
        if (existing != null && existing.matches(file)) {
            return existing
        }
        synchronized(this) {
            val current = openPackages[path]
            if (current != null && current.matches(file)) {
                return current
            }
            current?.close()
            val replacement = OpenZipPackage.open(file)
            openPackages[path] = replacement
            return replacement
        }
    }
}

internal class OpenZipPackage private constructor(
    private val path: String,
    private val length: Long,
    private val lastModified: Long,
    val zipFile: ZipFile,
    val entries: Set<String>,
) {
    val identityHashCode: Int = System.identityHashCode(zipFile)

    fun matches(file: File): Boolean =
        file.absolutePath == path && file.length() == length && file.lastModified() == lastModified

    fun close() {
        zipFile.close()
    }

    companion object {
        fun open(file: File): OpenZipPackage {
            val zip = ZipFile(file)
            val entryNames = Collections.unmodifiableSet(zip.entries().asSequence().map { it.name }.toSet())
            return OpenZipPackage(
                path = file.absolutePath,
                length = file.length(),
                lastModified = file.lastModified(),
                zipFile = zip,
                entries = entryNames,
            )
        }
    }
}
