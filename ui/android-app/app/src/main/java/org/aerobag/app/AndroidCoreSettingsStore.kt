// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app

import android.content.Context
import android.util.AtomicFile
import java.io.File
import org.aerobag.app.domain.CoreSettingsStore

private const val CoreSettingsFileName = "core-settings-v1.json"

internal class AndroidCoreSettingsStore(context: Context) : CoreSettingsStore {
    private val file = File(context.applicationContext.filesDir, CoreSettingsFileName)
    private val atomicFile = AtomicFile(file)

    @Synchronized
    override fun readSettings(): ByteArray? =
        if (file.exists()) {
            atomicFile.readFully()
        } else {
            null
        }

    @Synchronized
    override fun writeSettings(bytes: ByteArray) {
        file.parentFile?.mkdirs()
        val output = atomicFile.startWrite()
        try {
            output.write(bytes)
            atomicFile.finishWrite(output)
        } catch (error: Throwable) {
            atomicFile.failWrite(output)
            throw error
        }
    }
}
