package org.aerobag.app.domain

import android.util.Log
import java.io.File
import java.util.concurrent.ConcurrentHashMap
import java.util.zip.ZipFile

internal object PackageZipStore {
    internal const val TAG = "AerobagZipStore"
    private const val SLOW_READ_MS = 20L

    private val openPackages = ConcurrentHashMap<String, OpenZipPackage>()

    fun readEntryBytes(file: File, entryName: String): ByteArray? {
        val packageRef = packageFor(file)
        val start = monotonicMs()
        val entry = packageRef.zipFile.getEntry(entryName) ?: return null
        if (entry.isDirectory) {
            return null
        }
        val bytes = packageRef.zipFile.getInputStream(entry).use { it.readBytes() }
        val elapsedMs = monotonicMs() - start
        if (elapsedMs >= SLOW_READ_MS) {
            logInfo("read file=${file.name} entry=$entryName bytes=${bytes.size} elapsedMs=$elapsedMs")
        }
        return bytes
    }

    fun invalidate(file: File) {
        openPackages.remove(file.absolutePath)?.close()
    }

    internal fun debugIdentity(file: File): String? =
        openPackages[file.absolutePath]?.identity

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
    val identity: String = "$path:$length:$lastModified"

    fun matches(file: File): Boolean =
        file.absolutePath == path && file.length() == length && file.lastModified() == lastModified

    fun close() {
        zipFile.close()
    }

    companion object {
        fun open(file: File): OpenZipPackage {
            val start = monotonicMs()
            val zip = ZipFile(file)
            val elapsedMs = monotonicMs() - start
            logInfo("open file=${file.name} size=${file.length()} elapsedMs=$elapsedMs")
            return OpenZipPackage(
                path = file.absolutePath,
                length = file.length(),
                lastModified = file.lastModified(),
                zipFile = zip,
            )
        }
    }
}

private fun monotonicMs(): Long =
    System.nanoTime() / 1_000_000L

private fun logInfo(message: String) {
    runCatching { Log.i(PackageZipStore.TAG, message) }
}
