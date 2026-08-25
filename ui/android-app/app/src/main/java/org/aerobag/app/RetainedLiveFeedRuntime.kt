// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app

import android.content.Context
import android.os.SystemClock
import android.util.Log
import java.io.File
import java.util.concurrent.Executor
import java.util.concurrent.Executors
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExecutorCoroutineDispatcher
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.asCoroutineDispatcher
import kotlinx.coroutines.cancel
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import org.aerobag.app.domain.AndroidLiveFeedClient
import org.aerobag.app.domain.LiveFeedCache
import org.aerobag.app.domain.LiveFeedCacheStore
import org.aerobag.app.domain.LiveFeedConnectionEvent
import org.aerobag.app.domain.LiveFeedInstalledSummary
import org.aerobag.app.domain.NativeSessionCommandRejectedException
import org.aerobag.app.domain.NativeUiSession
import org.aerobag.app.generated.UiSessionUpdateGroup

internal fun interface InitialLiveFeedPromotionGate {
    suspend fun awaitPromotion(installed: List<LiveFeedInstalledSummary>)
}

internal const val E2eLiveFeedPromotionPauseFile = "e2e-live-feed-promotion.pause"
internal const val E2eLiveFeedPromotionPauseMarker = "E2E live-feed promotion paused"

internal fun createFileControlledInitialLiveFeedPromotionGate(
    context: Context,
    enabled: Boolean,
): InitialLiveFeedPromotionGate {
    if (!enabled) return InitialLiveFeedPromotionGate { }
    val sentinel = File(context.filesDir, E2eLiveFeedPromotionPauseFile)
    return InitialLiveFeedPromotionGate { installed ->
        if (sentinel.exists() && installed.isNotEmpty()) {
            Log.i("AndroidLiveFeeds", E2eLiveFeedPromotionPauseMarker)
            while (sentinel.exists()) delay(50)
        }
    }
}

internal suspend fun runLiveFeedStartup(
    listInstalled: suspend () -> List<LiveFeedInstalledSummary>,
    awaitInitialPromotion: suspend (List<LiveFeedInstalledSummary>) -> Unit,
    restoreInstalled: suspend (
        List<LiveFeedInstalledSummary>,
        suspend (LiveFeedInstalledSummary) -> Unit,
    ) -> List<LiveFeedInstalledSummary>,
    promote: suspend (LiveFeedInstalledSummary) -> Boolean,
    onInitialPromotionComplete: (List<LiveFeedInstalledSummary>) -> Unit,
    enablePolicyPumps: () -> Unit,
    startNetwork: suspend () -> Unit,
    reportFailure: (String, Throwable) -> Unit,
) {
    val installed = try {
        listInstalled()
    } catch (error: CancellationException) {
        throw error
    } catch (error: Throwable) {
        reportFailure("persisted cache restore", error)
        emptyList()
    }

    try {
        awaitInitialPromotion(installed)
    } catch (error: CancellationException) {
        throw error
    } catch (error: Throwable) {
        reportFailure("initial promotion gate", error)
    }

    suspend fun promoteRestored(summary: LiveFeedInstalledSummary) {
        try {
            check(promote(summary)) {
                "failed to promote ${summary.product}/${summary.version}"
            }
        } catch (error: CancellationException) {
            throw error
        } catch (error: Throwable) {
            reportFailure("cached ${summary.product}/${summary.version} promotion", error)
        }
    }
    val restored = try {
        restoreInstalled(installed, ::promoteRestored)
    } catch (error: CancellationException) {
        throw error
    } catch (error: Throwable) {
        reportFailure("persisted cache restore", error)
        emptyList()
    }
    onInitialPromotionComplete(restored)
    enablePolicyPumps()
    startNetwork()
}

/** Owns the one live-feed restore and connection pipeline for a retained core session. */
internal class RetainedLiveFeedRuntime(
    context: Context,
    private val uiSession: NativeUiSession,
    private val cache: LiveFeedCache,
    sourceRootUrl: String,
    private val resultExecutor: Executor,
    private val initialPromotionGate: InitialLiveFeedPromotionGate,
    private val startupPerfTrace: AndroidStartupPerfTrace? = null,
) : AutoCloseable {
    private val lock = Any()
    private val workerDispatcher: ExecutorCoroutineDispatcher =
        Executors.newSingleThreadExecutor { runnable ->
            Thread(runnable, "aerobag-live-feeds").apply { isDaemon = true }
        }.asCoroutineDispatcher()
    private val scope = CoroutineScope(SupervisorJob() + workerDispatcher)
    private val appContext = context.applicationContext
    private val client = AndroidLiveFeedClient(
        context = appContext,
        cache = cache,
        sourceRootUrl = sourceRootUrl,
        beforePump = { uiSession.syncLiveFeedCacheAcquisitionPolicy(cache) },
        reportAcquisitionPhase = { product, phase ->
            uiSession.reportLiveFeedAcquisitionPhase(product, phase)
        },
    )
    private var started = false
    private var policyPumpsEnabled = false
    private var closed = false
    private var generation = 0
    private var generationListener: ((Int) -> Unit)? = null
    private val policySubscription = uiSession.subscribeSnapshotGroups(
        setOf(
            UiSessionUpdateGroup.Settings,
            UiSessionUpdateGroup.Map,
            UiSessionUpdateGroup.Ownship,
            UiSessionUpdateGroup.FlightPlan,
        ),
    ) {
        val shouldPump = synchronized(lock) { started && policyPumpsEnabled && !closed }
        if (shouldPump) {
            scope.launch {
                client.pumpUntilSettled(
                    promote = { summary ->
                        check(promote(summary)) {
                            "failed to promote ${summary.product}/${summary.version}"
                        }
                    },
                    onChanged = ::syncCatalog,
                )
            }
        }
    }
    fun start() {
        synchronized(lock) {
            check(!closed) { "cannot start a closed live-feed runtime" }
            if (started) return
            started = true
        }
        startupPerfTrace?.mark("live_feed_restore_started")
        scope.launch {
            try {
                var promotionStartedAtMs = 0L
                runLiveFeedStartup(
                    listInstalled = ::listPersistedProducts,
                    awaitInitialPromotion = { installed ->
                        promotionStartedAtMs = SystemClock.elapsedRealtime()
                        initialPromotionGate.awaitPromotion(installed)
                    },
                    restoreInstalled = { installed, onRestored ->
                        val restoreStartedAtMs = SystemClock.elapsedRealtime()
                        val restored = restoreInstalledProducts(installed, onRestored)
                        startupPerfTrace?.mark(
                            "live_feed_cache_restored",
                            restoreStartedAtMs,
                            "products=${restored.size}",
                        )
                        restored
                    },
                    promote = { summary ->
                        if (summary.product != "winds-aloft") {
                            promote(summary)
                        } else {
                            try {
                                promote(summary)
                            } finally {
                                reportWindsAcquisitionPhase("idle")
                            }
                        }
                    },
                    onInitialPromotionComplete = { installed ->
                        startupPerfTrace?.mark(
                            "live_feed_products_promoted",
                            promotionStartedAtMs,
                            "products=${installed.size}",
                        )
                    },
                    enablePolicyPumps = {
                        synchronized(lock) {
                            if (!closed) policyPumpsEnabled = true
                        }
                    },
                    startNetwork = {
                        startupPerfTrace?.mark("live_feed_connection_started")
                        client.bootstrapAndRun(
                            promote = { summary ->
                                check(promote(summary)) {
                                    "failed to promote ${summary.product}/${summary.version}"
                                }
                            },
                            onChanged = ::syncCatalog,
                            onConnectionEvent = { event -> reportConnection(event) },
                        )
                    },
                    reportFailure = { phase, error ->
                        Log.e(LogTag, "live-feed startup $phase failed", error)
                    },
                )
            } catch (error: CancellationException) {
                throw error
            } catch (error: Throwable) {
                Log.e(LogTag, "retained live-feed runtime stopped", error)
            }
        }
    }

    fun subscribeGeneration(listener: (Int) -> Unit): AutoCloseable {
        val currentGeneration = synchronized(lock) {
            check(!closed) { "cannot subscribe to a closed live-feed runtime" }
            generationListener = listener
            generation
        }
        resultExecutor.execute { listener(currentGeneration) }
        return AutoCloseable {
            synchronized(lock) {
                if (generationListener === listener) generationListener = null
            }
        }
    }

    override fun close() {
        synchronized(lock) {
            if (closed) return
            closed = true
            generationListener = null
        }
        scope.cancel()
        policySubscription.close()
        workerDispatcher.close()
        cache.close()
    }

    private suspend fun promote(summary: LiveFeedInstalledSummary): Boolean {
        val preparedBytes = withContext(Dispatchers.IO) {
            cache.preparedInstallCandidate(summary.product, summary.version)
        }
        val promoted = runSessionCommand("installLiveFeedCacheProduct") {
            if (preparedBytes != null) {
                uiSession.installPreparedLiveFeedCacheProduct(
                    cache,
                    summary.product,
                    summary.version,
                    preparedBytes,
                )
            } else {
                uiSession.installLiveFeedCacheProduct(cache, summary.product, summary.version)
            }
        }
        if (promoted) publishNextGeneration(summary)
        return promoted
    }

    private suspend fun listPersistedProducts(): List<LiveFeedInstalledSummary> {
        val installed = withContext(Dispatchers.IO) {
            LiveFeedCacheStore.listInstalledSummaries(appContext)
        }
        if (installed.any { it.product == "winds-aloft" }) {
            reportWindsAcquisitionPhase("installing")
        }
        return installed
    }

    private suspend fun restoreInstalledProducts(
        installed: List<LiveFeedInstalledSummary>,
        onRestored: suspend (LiveFeedInstalledSummary) -> Unit,
    ): List<LiveFeedInstalledSummary> {
        val restoringWinds = installed.any { it.product == "winds-aloft" }
        return try {
            val restored = LiveFeedCacheStore.restore(
                context = appContext,
                cache = cache,
                installed = installed,
                onRestored = onRestored,
            )
            if (restoringWinds && restored.none { it.product == "winds-aloft" }) {
                reportWindsAcquisitionPhase("idle")
            }
            restored
        } catch (error: CancellationException) {
            throw error
        } catch (error: Throwable) {
            if (restoringWinds) reportWindsAcquisitionPhase("idle")
            throw error
        }
    }

    private suspend fun reportWindsAcquisitionPhase(phase: String) {
        runSessionCommand("reportRestoredWindsAcquisitionPhase") {
            uiSession.reportLiveFeedAcquisitionPhase("winds-aloft", phase)
        }
    }

    private suspend fun reportConnection(event: LiveFeedConnectionEvent) {
        runSessionCommand("reportLiveFeedConnectionEvent") {
            uiSession.reportLiveFeedConnectionEvent(event)
        }
    }

    private suspend fun syncCatalog() {
        runSessionCommand("syncLiveFeedCacheCatalog") {
            uiSession.syncLiveFeedCacheCatalog(cache)
        }
    }

    private suspend fun runSessionCommand(
        commandName: String,
        operation: () -> Unit,
    ): Boolean = withContext(workerDispatcher) {
        try {
            operation()
            true
        } catch (error: NativeSessionCommandRejectedException) {
            Log.w(LogTag, "core rejected background command=$commandName", error)
            false
        } catch (error: CancellationException) {
            throw error
        } catch (error: Throwable) {
            Log.w(LogTag, "background command failed command=$commandName", error)
            false
        }
    }

    private fun publishNextGeneration(summary: LiveFeedInstalledSummary) {
        val update = synchronized(lock) {
            if (closed) return
            generation += 1
            generation to generationListener
        }
        diagnosticLogInfo(LogTag) {
            "promoted product=${summary.product} version=${summary.version} generation=${update.first}"
        }
        update.second?.let { listener -> resultExecutor.execute { listener(update.first) } }
    }

    private companion object {
        const val LogTag = "AndroidLiveFeeds"
    }
}
