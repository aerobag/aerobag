// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app

import android.content.Context
import android.util.AtomicFile
import java.io.File
import java.io.FileNotFoundException
import java.io.FileOutputStream
import java.nio.file.Files
import java.nio.file.StandardCopyOption
import java.nio.file.StandardOpenOption
import java.nio.channels.FileChannel
import org.aerobag.app.domain.LocalDocumentStore

private const val CoreSettingsFileName = "core-settings-v1.json"

internal class AndroidLocalDocumentStore(
    context: Context,
    private val replaceFile: (File, File) -> Unit = { source, target ->
        Files.move(source.toPath(), target.toPath(), StandardCopyOption.ATOMIC_MOVE, StandardCopyOption.REPLACE_EXISTING)
    },
) : LocalDocumentStore, AutoCloseable {
    private val root = context.applicationContext.filesDir
    private val diskLock = synchronized(rootLocks) { rootLocks.getOrPut(root.canonicalPath) { Any() } }
    private var closed = false
    private fun document(key: String): AtomicFile {
        require(key.isNotEmpty() && key != "." && key != ".." && !key.contains('/') && !key.contains('\\'))
        return AtomicFile(File(root, key))
    }

    override fun readDocument(key: String): ByteArray? = synchronized(diskLock) {
        check(!closed) { "Local document store is closed" }
        val file = document(key)
        try {
            file.readFully()
        } catch (error: FileNotFoundException) {
            if (file.baseFile.exists() || File(root, "${key}.bak").exists() || !root.canRead()) throw error
            null
        }
    }

    override fun writeDocument(key: String, bytes: ByteArray?): Unit = synchronized(diskLock) {
        check(!closed) { "Local document store is closed" }
        val file = document(key)
        if (bytes == null) {
            file.delete()
            check(!file.baseFile.exists() && !File(root, "${key}.bak").exists()) { "Local document deletion failed" }
            syncDirectory()
        } else writeDocument(file, bytes)
    }

    private fun writeDocument(atomicFile: AtomicFile, bytes: ByteArray) {
        val target = atomicFile.baseFile
        check(!target.exists() || target.isFile) { "Local document is not a file" }
        val temporary = File(root, ".${target.name}.pending")
        try {
            FileOutputStream(temporary).use { output ->
                output.write(bytes)
                output.fd.sync()
            }
            // AtomicFile.finishWrite logs some rename failures instead of throwing.
            // The native host needs an actual completion/failure, not a log line.
            replaceFile(temporary, target)
            syncDirectory()
        } finally {
            temporary.delete()
        }
    }

    private fun syncDirectory() {
        FileChannel.open(root.toPath(), StandardOpenOption.READ).use { it.force(true) }
    }

    fun clearSettings(): Unit = synchronized(diskLock) {
        check(!closed) { "Local document store is closed" }
        document(CoreSettingsFileName).delete()
        document("tour-introduction-v1.json").delete()
    }

    override fun close() = synchronized(diskLock) { closed = true }

    private companion object {
        // JNI keeps store references while queued completions outlive a session.
        // All readers/writers of a directory must serialize, not just one wrapper.
        val rootLocks = mutableMapOf<String, Any>()
    }
}
