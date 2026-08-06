// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app.domain

import android.content.Context
import android.os.SystemClock
import org.aerobag.app.diagnosticLogInfo

class RuntimeContent(
    val navKvStore: NavKvStore,
    installedPackageIds: List<String>,
) {
    var installedPackageIds: List<String> = installedPackageIds
        private set

    fun replaceInstalledArtifacts(
        context: Context,
        libraryCacheJson: String,
        uiSession: NativeUiSession,
        plannedGcFilenames: Set<String>,
    ): NavDbAdvanceUiResult {
        val installedArtifacts = InstalledPackages.listInstalledArtifacts(context.applicationContext)
        val result = uiSession.advanceInstalledArtifacts(
            installedArtifacts,
            libraryCacheJson,
            plannedGcFilenames,
        )
        installedArtifacts
            .filterNot { it.filename in plannedGcFilenames && it.filename !in result.retainedArtifactFilenames }
            .map { it.artifactId }
            .distinct()
            .sorted()
            .also { installedPackageIds = it }
        return result
    }
}

object AndroidRuntimeContent {
    private const val TAG = "AndroidRuntimeContent"

    fun loadInstalledRuntime(
        context: Context,
        libraryCacheJson: String,
    ): RuntimeContent {
        val navKvOpenStartMs = SystemClock.elapsedRealtime()
        val installedArtifacts = InstalledPackages.listInstalledArtifacts(context.applicationContext)
        val navKvStore = NavKvStore.openInstalledArtifacts(
            installedArtifacts,
            libraryCacheJson = libraryCacheJson,
        )
        val navKvOpenMs = SystemClock.elapsedRealtime() - navKvOpenStartMs
        val installedPackageIds = installedArtifacts
            .map { it.artifactId }
            .distinct()
            .sorted()
        return loadInstalledRuntime(
            navKvStore = navKvStore,
            navKvOpenMs = navKvOpenMs,
            installedPackageIds = installedPackageIds,
        )
    }

    fun loadInstalledRuntime(
        navKvStore: NavKvStore,
        navKvOpenMs: Long,
        installedPackageIds: List<String> = emptyList(),
    ): RuntimeContent {
        val startMs = SystemClock.elapsedRealtime()
        return RuntimeContent(
            navKvStore = navKvStore,
            installedPackageIds = installedPackageIds,
        ).also {
            diagnosticLogInfo(TAG) {
                "loadInstalledRuntime completed in ${SystemClock.elapsedRealtime() - startMs}ms " +
                    "(navKvOpen=${navKvOpenMs}ms)"
            }
        }
    }

}
