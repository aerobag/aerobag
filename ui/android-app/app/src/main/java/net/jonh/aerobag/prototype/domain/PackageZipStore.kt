package net.jonh.aerobag.prototype.domain

import android.os.SystemClock
import android.util.Log
import java.io.File
import java.util.Collections
import java.util.concurrent.ConcurrentHashMap
import java.util.zip.ZipFile

internal object PackageZipStore {
    internal const val TAG = "AerobagZipStore"
    private const val SLOW_READ_MS = 20L

    private val openPackages = ConcurrentHashMap<String, OpenZipPackage>()

    fun readEntryBytes(file: File, entryName: String): ByteArray? {
        val packageRef = packageFor(file)
        val start = SystemClock.elapsedRealtime()
        val entry = packageRef.zipFile.getEntry(entryName) ?: return null
        if (entry.isDirectory) {
            return null
        }
        val bytes = packageRef.zipFile.getInputStream(entry).use { it.readBytes() }
        val elapsedMs = SystemClock.elapsedRealtime() - start
        if (elapsedMs >= SLOW_READ_MS) {
            Log.i(
                TAG,
                "read file=${file.name} entry=$entryName bytes=${bytes.size} elapsedMs=$elapsedMs",
            )
        }
        return bytes
    }

    fun hasEntry(file: File, entryName: String): Boolean =
        entryNames(file).contains(entryName)

    fun entryNames(file: File): Set<String> =
        packageFor(file).entryNames()

    fun invalidate(file: File) {
        openPackages.remove(file.absolutePath)?.close()
    }

    internal fun debugIdentity(file: File): String? =
        openPackages[file.absolutePath]?.identity

    internal fun debugEntryCount(file: File): Int? =
        openPackages[file.absolutePath]?.debugEntryCount()

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
) {
    private var entries: Set<String>? = null

    val identity: String = "$path:$length:$lastModified"

    fun matches(file: File): Boolean =
        file.absolutePath == path && file.length() == length && file.lastModified() == lastModified

    fun close() {
        zipFile.close()
    }

    fun entryNames(): Set<String> {
        entries?.let { return it }
        synchronized(this) {
            entries?.let { return it }
            val start = SystemClock.elapsedRealtime()
            val indexedEntries = Collections.unmodifiableSet(zipFile.entries().asSequence().map { it.name }.toSet())
            entries = indexedEntries
            Log.i(
                PackageZipStore.TAG,
                "index file=${File(path).name} entries=${indexedEntries.size} elapsedMs=${SystemClock.elapsedRealtime() - start}",
            )
            return indexedEntries
        }
    }

    fun debugEntryCount(): Int? =
        entries?.size

    companion object {
        fun open(file: File): OpenZipPackage {
            val start = SystemClock.elapsedRealtime()
            val zip = ZipFile(file)
            val elapsedMs = SystemClock.elapsedRealtime() - start
            Log.i(
                PackageZipStore.TAG,
                "open file=${file.name} size=${file.length()} elapsedMs=$elapsedMs",
            )
            return OpenZipPackage(
                path = file.absolutePath,
                length = file.length(),
                lastModified = file.lastModified(),
                zipFile = zip,
            )
        }
    }
}
