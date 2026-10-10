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

internal class AndroidLocalDocumentStore(context: Context) : LocalDocumentStore {
    private val root = context.applicationContext.filesDir
    private fun document(key: String): AtomicFile {
        require(key.isNotEmpty() && key != "." && key != ".." && !key.contains('/') && !key.contains('\\'))
        return AtomicFile(File(root, key))
    }

    @Synchronized
    override fun readDocument(key: String): ByteArray? {
        val file = document(key)
        return try {
            file.readFully()
        } catch (error: FileNotFoundException) {
            if (file.baseFile.exists() || File(root, "${key}.bak").exists() || !root.canRead()) throw error
            null
        }
    }

    @Synchronized
    override fun writeDocument(key: String, bytes: ByteArray?) {
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
            Files.move(temporary.toPath(), target.toPath(), StandardCopyOption.ATOMIC_MOVE, StandardCopyOption.REPLACE_EXISTING)
            syncDirectory()
        } finally {
            temporary.delete()
        }
    }

    private fun syncDirectory() {
        FileChannel.open(root.toPath(), StandardOpenOption.READ).use { it.force(true) }
    }

    @Synchronized
    fun clearSettings() {
        document(CoreSettingsFileName).delete()
        document("tour-introduction-v1.json").delete()
    }
}
