// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app

import android.content.Context
import android.util.Log
import androidx.lifecycle.ViewModel
import org.aerobag.app.domain.MapViewportState
import org.aerobag.app.domain.ClientBuildInfo
import org.aerobag.app.domain.NativeAppCoreAdapter
import org.aerobag.app.domain.NativeUiSession
import org.aerobag.app.domain.RuntimeContent
import org.aerobag.app.domain.SituationRingCandidate

internal class AerobagRetainedCoreSession(
    val runtimeContent: RuntimeContent,
    val appCore: NativeAppCoreAdapter,
    val uiSession: NativeUiSession,
    val situationRingCandidates: List<SituationRingCandidate>,
    val decodedTileBitmapCache: DecodedTileBitmapCache,
) {
    private var closed = false

    fun close() {
        if (closed) return
        closed = true
        runCatching { uiSession.destroy() }
            .onFailure { Log.w("AerobagRetainedState", "failed to destroy retained UI session", it) }
        runCatching { runtimeContent.navKvStore.close() }
            .onFailure { Log.w("AerobagRetainedState", "failed to close retained nav DB", it) }
        decodedTileBitmapCache.clear()
    }
}

internal class AerobagRetainedModel : ViewModel() {
    var runtimeResult: Result<RuntimeContent>? = null
    var coreSession: AerobagRetainedCoreSession? = null
    var page: AppPage? = null
    var pageHistory: List<AppViewSnapshot> = emptyList()
    var mapViewport: MapViewportState? = null

    fun resetRuntime() {
        val sessionRuntime = coreSession?.runtimeContent
        coreSession?.close()
        coreSession = null
        runtimeResult?.getOrNull()
            ?.takeIf { it !== sessionRuntime }
            ?.let { runtime ->
                runCatching { runtime.navKvStore.close() }
                    .onFailure { Log.w("AerobagRetainedState", "failed to close retained runtime", it) }
            }
        runtimeResult = null
    }

    fun getOrCreateCoreSession(
        context: Context,
        runtimeContent: RuntimeContent,
        recentAirportIds: List<String>,
        selectedAirportId: String?,
        selectedChartId: String?,
    ): AerobagRetainedCoreSession {
        coreSession
            ?.takeIf { it.runtimeContent === runtimeContent }
            ?.let { return it }

        coreSession?.close()
        val appCore = NativeAppCoreAdapter(navKvStore = runtimeContent.navKvStore)
        val uiSession = appCore.createUiSession(
            recentAirportIds,
            selectedAirportId,
            selectedChartId,
            runtimeContent.installedPackageIds,
            settingsStore = AndroidCoreSettingsStore(context.applicationContext),
            displayPolicySettingsAvailable = true,
            clientBuildInfo = ClientBuildInfo(
                platform = "Android",
                version = BuildConfig.VERSION_NAME,
                builtAtUtc = BuildConfig.AEROBAG_BUILT_AT_UTC,
                commit = BuildConfig.AEROBAG_GIT_COMMIT,
                dirty = BuildConfig.AEROBAG_BUILD_DIRTY,
            ),
        )
        val prefs = context.applicationContext.getSharedPreferences(UiPrefsName, Context.MODE_PRIVATE)
        uiSession.loadOfflinePackageLibraryCache(readOfflinePackagesLibraryCacheJson(prefs))
        return AerobagRetainedCoreSession(
            runtimeContent = runtimeContent,
            appCore = appCore,
            uiSession = uiSession,
            situationRingCandidates = appCore.situationRingCandidates(),
            decodedTileBitmapCache = DecodedTileBitmapCache(DecodedTileCacheMaxBytes),
        ).also { coreSession = it }
    }

    override fun onCleared() {
        resetRuntime()
        super.onCleared()
    }
}
