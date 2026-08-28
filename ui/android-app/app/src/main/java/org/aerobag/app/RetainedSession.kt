// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app

import android.content.Context
import android.os.SystemClock
import android.util.Log
import androidx.core.content.ContextCompat
import androidx.lifecycle.ViewModel
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.CoroutineStart
import kotlinx.coroutines.Deferred
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.async
import kotlinx.coroutines.cancel
import org.aerobag.app.domain.ClientBuildInfo
import org.aerobag.app.domain.AndroidRuntimeContent
import org.aerobag.app.domain.LiveFeedCacheStore
import org.aerobag.app.domain.MapViewportState
import org.aerobag.app.domain.MapOrientationMemory
import org.aerobag.app.domain.NativeAppCoreAdapter
import org.aerobag.app.domain.NativeUiSession
import org.aerobag.app.domain.RuntimeContent
import org.aerobag.app.domain.SituationRingCandidate
import org.aerobag.app.domain.UiSessionSnapshot

internal class AerobagRetainedCoreSession(
    val runtimeContent: RuntimeContent,
    val appCore: NativeAppCoreAdapter,
    val uiSession: NativeUiSession,
    val situationRingCandidates: List<SituationRingCandidate>,
    val decodedTileBitmapCache: DecodedTileBitmapCache,
    val liveFeedRuntime: RetainedLiveFeedRuntime,
    val sessionSnapshotRefreshRunner: SessionSnapshotRefreshRunner<UiSessionSnapshot>,
    val uiSessionWorkRunner: UiSessionWorkRunner,
) {
    private var closed = false

    fun close() {
        if (closed) return
        closed = true
        runCatching { liveFeedRuntime.close() }
            .onFailure { Log.w("AerobagRetainedState", "failed to close live-feed runtime", it) }
        runCatching { sessionSnapshotRefreshRunner.close() }
            .onFailure { Log.w("AerobagRetainedState", "failed to close snapshot refresh runner", it) }
        runCatching { uiSessionWorkRunner.close() }
            .onFailure { Log.w("AerobagRetainedState", "failed to close session work runner", it) }
        runCatching { uiSession.destroy() }
            .onFailure { Log.w("AerobagRetainedState", "failed to destroy retained UI session", it) }
        runCatching { runtimeContent.navKvStore.close() }
            .onFailure { Log.w("AerobagRetainedState", "failed to close retained nav DB", it) }
        decodedTileBitmapCache.clear()
    }
}

internal class AerobagRetainedModel : ViewModel() {
    private val startupLock = Any()
    private val startupScope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
    private var startupGeneration = 0L
    private var startupPreparation: Deferred<Result<RuntimeContent>>? = null

    @Volatile
    var runtimeResult: Result<RuntimeContent>? = null
        private set

    @Volatile
    var coreSession: AerobagRetainedCoreSession? = null
        private set

    var page: AppPage? = null
    var pageHistory: List<AppViewSnapshot> = emptyList()
    var mapViewport: MapViewportState? = null
    val mapOrientationMemory = MapOrientationMemory()

    fun resetRuntime() {
        val (preparation, session, runtime) = synchronized(startupLock) {
            startupGeneration += 1
            val detached = Triple(
                startupPreparation,
                coreSession,
                runtimeResult?.getOrNull(),
            )
            startupPreparation = null
            coreSession = null
            runtimeResult = null
            detached
        }
        preparation?.cancel()
        val sessionRuntime = session?.runtimeContent
        session?.close()
        runtime
            ?.takeIf { it !== sessionRuntime }
            ?.let { runtime ->
                runCatching { runtime.navKvStore.close() }
                    .onFailure { Log.w("AerobagRetainedState", "failed to close retained runtime", it) }
            }
    }

    fun beginStartupPreparation(
        context: Context,
        startupPerfTrace: AndroidStartupPerfTrace? = null,
    ) {
        val appContext = context.applicationContext
        val preparation = synchronized(startupLock) {
            if (runtimeResult != null || startupPreparation != null) {
                return
            }
            val generation = startupGeneration
            startupScope.async(start = CoroutineStart.LAZY) {
                prepareStartup(appContext, generation, startupPerfTrace)
            }.also { startupPreparation = it }
        }
        preparation.start()
    }

    suspend fun awaitStartupPreparation(
        context: Context,
        startupPerfTrace: AndroidStartupPerfTrace? = null,
    ): Result<RuntimeContent> {
        beginStartupPreparation(context, startupPerfTrace)
        val (result, preparation) = synchronized(startupLock) {
            runtimeResult to startupPreparation
        }
        return result ?: checkNotNull(preparation) {
            "startup preparation has neither a result nor an active job"
        }.await()
    }

    fun preparedCoreSession(runtimeContent: RuntimeContent): AerobagRetainedCoreSession =
        checkNotNull(coreSession?.takeIf { it.runtimeContent === runtimeContent }) {
            "runtime content was published without its prepared core session"
        }

    private fun prepareStartup(
        context: Context,
        generation: Long,
        startupPerfTrace: AndroidStartupPerfTrace?,
    ): Result<RuntimeContent> {
        var runtimeContent: RuntimeContent? = null
        var retainedSession: AerobagRetainedCoreSession? = null
        try {
            val prefs = context.getSharedPreferences(UiPrefsName, Context.MODE_PRIVATE)
            val runtimeLoadStartedAtMs = SystemClock.elapsedRealtime()
            startupPerfTrace?.mark("runtime_load_started")
            val loadedRuntime = AndroidRuntimeContent.loadInstalledRuntime(
                context,
                readOfflinePackagesLibraryCacheJson(prefs),
                onStartupStage = { stage, durationMs ->
                    startupPerfTrace?.mark(
                        stage,
                        SystemClock.elapsedRealtime() - durationMs,
                    )
                },
            )
            runtimeContent = loadedRuntime
            startupPerfTrace?.mark(
                "runtime_loaded",
                runtimeLoadStartedAtMs,
                "outcome=success",
            )

            val sessionCreateStartedAtMs = SystemClock.elapsedRealtime()
            startupPerfTrace?.mark("session_create_started")
            retainedSession = buildCoreSession(
                context = context,
                runtimeContent = loadedRuntime,
                recentAirportIds = readRecentAirportIds(context),
                selectedAirportId = prefs.getString(UiPrefsSelectedAirportKey, null),
                selectedChartId = prefs.getString(UiPrefsSelectedChartKey, null),
                startupPerfTrace = startupPerfTrace,
            )
            startupPerfTrace?.mark("session_created", sessionCreateStartedAtMs)

            val result = Result.success(loadedRuntime)
            val published = synchronized(startupLock) {
                if (generation != startupGeneration) {
                    false
                } else {
                    coreSession = retainedSession
                    runtimeResult = result
                    startupPreparation = null
                    true
                }
            }
            if (!published) {
                retainedSession.close()
            }
            return result
        } catch (error: Throwable) {
            retainedSession?.close()
                ?: runtimeContent?.let { runtime ->
                    runCatching { runtime.navKvStore.close() }
                        .onFailure { Log.w("AerobagRetainedState", "failed to close failed startup runtime", it) }
                }
            if (error is CancellationException) {
                throw error
            }
            startupPerfTrace?.mark("runtime_loaded", detail = "outcome=failure")
            val result = Result.failure<RuntimeContent>(error)
            synchronized(startupLock) {
                if (generation == startupGeneration) {
                    runtimeResult = result
                    startupPreparation = null
                }
            }
            return result
        }
    }

    private fun buildCoreSession(
        context: Context,
        runtimeContent: RuntimeContent,
        recentAirportIds: List<String>,
        selectedAirportId: String?,
        selectedChartId: String?,
        startupPerfTrace: AndroidStartupPerfTrace?,
    ): AerobagRetainedCoreSession {
        val appCore = NativeAppCoreAdapter(
            navKvStore = runtimeContent.navKvStore,
            sessionResourceFetcher = { resource ->
                fetchCoreResource(context.applicationContext, resource, "")
            },
        )
        val prefs = context.applicationContext.getSharedPreferences(UiPrefsName, Context.MODE_PRIVATE)
        val cycleDataBaseUrl = runCatching {
            resolvePublicationRootUrl(readPackageSourceBaseUrl(context.applicationContext, prefs))
        }.getOrElse {
            readPackageSourceBaseUrl(context.applicationContext, prefs).trim().trimEnd('/')
        }
        val liveFeedSourceRootUrl = configuredLiveFeedSourceRootUrl(
            context.applicationContext,
            prefs,
            loadAndroidDevServerBaseUrl(context.applicationContext),
        )
        val uiSession = appCore.createUiSession(
            recentAirportIds,
            selectedAirportId,
            selectedChartId,
            runtimeContent.installedPackageIds,
            settingsStore = AndroidCoreSettingsStore(context.applicationContext),
            displayPolicySettingsAvailable = true,
            aerobagCloudBaseUrl = loadAndroidCloudServerBaseUrl(context.applicationContext),
            clientBuildInfo = ClientBuildInfo(
                platform = "Android",
                version = BuildConfig.VERSION_NAME,
                builtAtUtc = BuildConfig.AEROBAG_BUILT_AT_UTC,
                commit = BuildConfig.AEROBAG_GIT_COMMIT,
                dirty = BuildConfig.AEROBAG_BUILD_DIRTY,
            ),
            cycleDataBaseUrl = cycleDataBaseUrl,
            liveFeedsBaseUrl = "${liveFeedSourceRootUrl.trimEnd('/')}/live-feeds",
            onStartupStage = { stage, durationMs ->
                startupPerfTrace?.mark(
                    stage,
                    SystemClock.elapsedRealtime() - durationMs,
                )
            },
        )
        val situationCandidatesStartedAtMs = SystemClock.elapsedRealtime()
        val situationRingCandidates = appCore.situationRingCandidates()
        startupPerfTrace?.mark("session_situation_candidates_loaded", situationCandidatesStartedAtMs)
        val packageCacheStartedAtMs = SystemClock.elapsedRealtime()
        uiSession.loadOfflinePackageLibraryCache(readOfflinePackagesLibraryCacheJson(prefs))
        startupPerfTrace?.mark("session_package_cache_loaded", packageCacheStartedAtMs)
        val liveFeedCache = LiveFeedCacheStore.create(liveFeedSourceRootUrl)
        val resultExecutor = ContextCompat.getMainExecutor(context.applicationContext)
        return AerobagRetainedCoreSession(
            runtimeContent = runtimeContent,
            appCore = appCore,
            uiSession = uiSession,
            situationRingCandidates = situationRingCandidates,
            decodedTileBitmapCache = DecodedTileBitmapCache(DecodedTileCacheMaxBytes),
            liveFeedRuntime = RetainedLiveFeedRuntime(
                context = context.applicationContext,
                uiSession = uiSession,
                cache = liveFeedCache,
                sourceRootUrl = liveFeedSourceRootUrl,
                resultExecutor = resultExecutor,
                initialPromotionGate = createInitialLiveFeedPromotionGate(context.applicationContext),
                startupPerfTrace = startupPerfTrace,
            ),
            sessionSnapshotRefreshRunner = SessionSnapshotRefreshRunner(
                refresh = uiSession::refreshSnapshot,
                resultExecutor = resultExecutor,
            ),
            uiSessionWorkRunner = UiSessionWorkRunner(uiSession),
        )
    }

    override fun onCleared() {
        resetRuntime()
        startupScope.cancel()
        super.onCleared()
    }
}
