package net.jonh.aerobag.prototype.domain

import java.io.File
import java.util.Collections
import java.util.concurrent.ConcurrentHashMap
import java.util.zip.ZipFile

internal object PackageZipStore {
    private val openPackages = ConcurrentHashMap<String, OpenZipPackage>()

    fun readEntryBytes(file: File, entryName: String): ByteArray? {
        val packageRef = packageFor(file)
        val entry = packageRef.zipFile.getEntry(entryName) ?: return null
        if (entry.isDirectory) {
            return null
        }
        return packageRef.zipFile.getInputStream(entry).use { it.readBytes() }
    }

    fun hasEntry(file: File, entryName: String): Boolean =
        entryNames(file).contains(entryName)

    fun entryNames(file: File): Set<String> =
        packageFor(file).entries

    fun invalidate(file: File) {
        openPackages.remove(file.absolutePath)?.close()
    }

    internal fun debugIdentity(file: File): String? =
        openPackages[file.absolutePath]?.identity

    internal fun debugEntryCount(file: File): Int? =
        openPackages[file.absolutePath]?.entries?.size

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
            return OpenZipPackage.open(file).also { openPackages[path] = it }
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
    val identity: String = "$path:$length:$lastModified"

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
