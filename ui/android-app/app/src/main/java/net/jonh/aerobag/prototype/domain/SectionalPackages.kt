package net.jonh.aerobag.prototype.domain

import android.content.Context
import android.util.Log
import java.io.File
import java.util.Collections
import java.util.concurrent.ConcurrentHashMap
import java.util.zip.ZipFile

object SectionalPackages {
    private const val TAG = "AerobagTiles"
    private val packageStore = ZipPackageStore()

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
                    val bytes = packageStore.loadTileBytes(installed, relativePath)
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

internal class ZipPackageStore {
    companion object {
        private const val TAG = "AerobagTiles"
    }

    private val openPackages = ConcurrentHashMap<String, OpenZipPackage>()

    fun loadTileBytes(file: File, relativePath: String): ByteArray? {
        val packageRef = packageFor(file)
        if (!packageRef.entries.contains(relativePath)) {
            logWarn(TAG, "zip missing entry file=${file.name} path=$relativePath entries=${packageRef.entries.size}")
            return null
        }
        val entry = packageRef.zipFile.getEntry(relativePath) ?: return null
        return runCatching {
            packageRef.zipFile.getInputStream(entry).use { it.readBytes() }
        }.onFailure { error ->
            logError(TAG, "zip read failed file=${file.name} path=$relativePath", error)
        }.getOrNull()
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

private fun logWarn(tag: String, message: String) {
    runCatching { Log.w(tag, message) }
}

private fun logError(tag: String, message: String, error: Throwable) {
    runCatching { Log.e(tag, message, error) }
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
