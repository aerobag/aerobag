// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app

import android.content.Context
import android.util.Log
import java.util.concurrent.Executor
import java.util.concurrent.Executors
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExecutorCoroutineDispatcher
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.asCoroutineDispatcher
import kotlinx.coroutines.cancel
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import org.aerobag.app.domain.AndroidLiveFeedClient
import org.aerobag.app.domain.LiveFeedCache
import org.aerobag.app.domain.LiveFeedCacheStore
import org.aerobag.app.domain.LiveFeedConnectionEvent
import org.aerobag.app.domain.LiveFeedInstalledSummary
import org.aerobag.app.domain.NativeSessionCommandRejectedException
import org.aerobag.app.domain.NativeUiSession

internal fun interface InitialLiveFeedPromotionGate {
    suspend fun awaitPromotion(installed: List<LiveFeedInstalledSummary>)
}

/** Owns the one live-feed restore and connection pipeline for a retained core session. */
internal class RetainedLiveFeedRuntime(
    context: Context,
    private val uiSession: NativeUiSession,
    private val cache: LiveFeedCache,
    sourceRootUrl: String,
    private val resultExecutor: Executor,
    private val initialPromotionGate: InitialLiveFeedPromotionGate,
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
    )
    private var started = false
    private var closed = false
    private var generation = 0
    private var generationListener: ((Int) -> Unit)? = null

    fun start() {
        synchronized(lock) {
            check(!closed) { "cannot start a closed live-feed runtime" }
            if (started) return
            started = true
        }
        scope.launch {
            try {
                val installed = withContext(Dispatchers.IO) {
                    LiveFeedCacheStore.restore(appContext, cache)
                    LiveFeedCacheStore.listInstalledSummaries(appContext)
                }
                initialPromotionGate.awaitPromotion(installed)
                installed.forEach { summary -> promote(summary) }
                client.bootstrapAndRun(
                    promote = { summary ->
                        check(promote(summary)) {
                            "failed to promote ${summary.product}/${summary.version}"
                        }
                    },
                    onChanged = {
                        runSessionCommand("syncLiveFeedCacheCatalog") {
                            uiSession.syncLiveFeedCacheCatalog(cache)
                        }
                    },
                    onConnectionEvent = { event -> reportConnection(event) },
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

    private suspend fun reportConnection(event: LiveFeedConnectionEvent) {
        runSessionCommand("reportLiveFeedConnectionEvent") {
            uiSession.reportLiveFeedConnectionEvent(event)
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
