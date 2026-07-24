// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app

import android.content.Context
import java.io.File
import org.aerobag.app.domain.CoreSettingsStore

private const val CoreSettingsFileName = "core-settings-v1.json"

internal class AndroidCoreSettingsStore(context: Context) : CoreSettingsStore {
    private val file = File(context.applicationContext.filesDir, CoreSettingsFileName)

    override fun readSettings(): ByteArray? =
        if (file.exists()) {
            file.readBytes()
        } else {
            null
        }

    override fun writeSettings(bytes: ByteArray) {
        file.parentFile?.mkdirs()
        file.writeBytes(bytes)
    }
}
