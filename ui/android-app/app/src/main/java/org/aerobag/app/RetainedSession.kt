// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app

import org.aerobag.app.domain.MapOrientationMode

import android.content.Context
import android.os.SystemClock
import android.util.Log
import androidx.core.content.ContextCompat
import androidx.lifecycle.ViewModel
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
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
    val receiverPublisher: ReceiverSessionPublisher,
    private val settingsStore: AndroidLocalDocumentStore,
) {
    private var closed = false

    fun close() {
        if (closed) return
        closed = true
        settingsStore.close()
        receiverPublisher.close()
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
    private val startupScope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
    private val startup = RetainedRuntime<AerobagRetainedCoreSession>(startupScope) { it.close() }

    val runtimeResult: Result<RuntimeContent>?
        get() = startup.result?.map { it.runtimeContent }

    val coreSession: AerobagRetainedCoreSession?
        get() = startup.result?.getOrNull()

    var tourSavedView: Triple<AppViewSnapshot, List<AppViewSnapshot>, MapOrientationMode>? = null
    var page: AppPage? = null
    var pageHistory: List<AppViewSnapshot> = emptyList()
    var mapViewport: MapViewportState? = null
    val mapOrientationMemory = MapOrientationMemory()

    fun resetRuntime() {
        startup.reset()
    }

    suspend fun adoptInstalledPackages(
        adoptionId: String,
        context: Context,
        startupPerfTrace: AndroidStartupPerfTrace? = null,
    ): Result<RuntimeContent> {
        startup.resetFor(adoptionId)
        return awaitStartupPreparation(context, startupPerfTrace)
    }

    fun beginStartupPreparation(
        context: Context,
        startupPerfTrace: AndroidStartupPerfTrace? = null,
    ) {
        val appContext = context.applicationContext
        startup.begin {
            prepareStartup(appContext, startupPerfTrace)
        }
    }

    suspend fun awaitStartupPreparation(
        context: Context,
        startupPerfTrace: AndroidStartupPerfTrace? = null,
    ): Result<RuntimeContent> {
        val appContext = context.applicationContext
        return startup.await {
            prepareStartup(appContext, startupPerfTrace)
        }.map { it.runtimeContent }
    }

    fun preparedCoreSession(runtimeContent: RuntimeContent): AerobagRetainedCoreSession =
        checkNotNull(coreSession?.takeIf { it.runtimeContent === runtimeContent }) {
            "runtime content was published without its prepared core session"
        }

    private fun prepareStartup(
        context: Context,
        startupPerfTrace: AndroidStartupPerfTrace?,
    ): AerobagRetainedCoreSession {
        var runtimeContent: RuntimeContent? = null
        var retainedSession: AerobagRetainedCoreSession? = null
        val settingsStore = AndroidLocalDocumentStore(context)
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
                settingsStore = settingsStore,
            )
            startupPerfTrace?.mark("session_created", sessionCreateStartedAtMs)

            return retainedSession
        } catch (error: Throwable) {
            settingsStore.close()
            retainedSession?.close()
                ?: runtimeContent?.let { runtime ->
                    runCatching { runtime.navKvStore.close() }
                        .onFailure { Log.w("AerobagRetainedState", "failed to close failed startup runtime", it) }
                }
            if (error is CancellationException) {
                throw error
            }
            startupPerfTrace?.mark("runtime_loaded", detail = "outcome=failure")
            throw error
        }
    }

    private fun buildCoreSession(
        context: Context,
        runtimeContent: RuntimeContent,
        recentAirportIds: List<String>,
        selectedAirportId: String?,
        selectedChartId: String?,
        startupPerfTrace: AndroidStartupPerfTrace?,
        settingsStore: AndroidLocalDocumentStore,
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
            settingsStore = settingsStore,
            displayPolicySettingsAvailable = true,
            aerobagCloudBaseUrl = loadAndroidCloudServerBaseUrl(context.applicationContext),
            serviceBulletinUrls = BuildConfig.AEROBAG_SERVICE_BULLETIN_URLS.split(',').filter { it.isNotBlank() },
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
        val workRunner = UiSessionWorkRunner(uiSession)
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
            uiSessionWorkRunner = workRunner,
            receiverPublisher = ReceiverSessionPublisher(AndroidReceiverRuntime.get(context), workRunner),
            settingsStore = settingsStore,
        )
    }

    override fun onCleared() {
        startup.close()
        super.onCleared()
    }
}
