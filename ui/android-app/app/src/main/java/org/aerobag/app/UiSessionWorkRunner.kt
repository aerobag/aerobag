// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

@file:OptIn(org.aerobag.app.domain.RawUiSessionWorkApi::class)

package org.aerobag.app

import android.os.Looper
import android.os.SystemClock
import android.util.Log
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import kotlinx.serialization.decodeFromString
import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.Json
import org.aerobag.app.domain.CoreResourceRequest
import org.aerobag.app.domain.LatLonPoint
import org.aerobag.app.domain.MapOverlayQueryOutcome
import org.aerobag.app.domain.MapSelectionForNavRefResult
import org.aerobag.app.domain.MapSelectionQueryResult
import org.aerobag.app.domain.MapViewportState
import org.aerobag.app.domain.NativeBindings
import org.aerobag.app.domain.NativeBridge
import org.aerobag.app.domain.NativeUiSession
import org.aerobag.app.domain.NavRef
import org.aerobag.app.domain.PagedSessionOperationMetrics
import org.aerobag.app.domain.PagedSessionOperationMetricsSnapshot
import org.aerobag.app.domain.TerrainOverlayQueryResult
import org.aerobag.app.domain.TerrainOverlayTileRequest
import org.aerobag.app.domain.UiSessionSnapshot
import org.aerobag.app.generated.NexradOverlayQueryResult
import org.aerobag.app.generated.UiSessionWorkCompletionDecision
import org.aerobag.app.generated.UiSessionWorkKind
import org.aerobag.app.generated.UiSessionWorkRequest
import org.aerobag.app.generated.UiSessionWorkRequestDecision
import org.aerobag.app.generated.UiSessionWorkResultAction

private const val UiSessionWorkLogTag = "AerobagSessionWork"

class UiSessionWorkRunner(
    private val uiSession: NativeUiSession,
    private val bridge: NativeBridge = NativeBindings,
) {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)
    private val schedulerHandle = bridge.createUiSessionWorkScheduler()
    private val mutationQueue = Channel<SessionMutation>(Channel.UNLIMITED)
    private val payloads = mutableMapOf<Long, RetainedWork>()
    private val activeRequestIds = mutableSetOf<Long>()
    private var nextRequestId = 1L
    private var closed = false
    private var perfMetricsEnabled = false

    init {
        scope.launch {
            for (mutation in mutationQueue) {
                val startedAtMs = SystemClock.elapsedRealtime()
                val outcome = runCatching {
                    withContext(Dispatchers.IO) {
                        mutation.operation(uiSession)
                    }
                }
                if (closed) return@launch
                outcome
                    .onSuccess(mutation.onResult)
                    .onFailure(mutation.onError)
                if (perfMetricsEnabled) {
                    Log.i(
                        UiSessionWorkLogTag,
                        "event=mutation_finished command=${mutation.commandName} " +
                            "outcome=${if (outcome.isSuccess) "success" else "error"} " +
                            "total_ms=${SystemClock.elapsedRealtime() - startedAtMs} main_thread=false",
                    )
                }
            }
        }
    }

    fun setPerfMetricsEnabled(enabled: Boolean) {
        if (perfMetricsEnabled == enabled) return
        perfMetricsEnabled = enabled
        if (enabled) {
            Log.i(UiSessionWorkLogTag, "event=instrumentation_enabled")
        }
    }

    fun close() {
        if (closed) return
        closed = true
        mutationQueue.close()
        payloads.values.forEach { retained ->
            retained.payload.dropped("runner_closed")
            logFinished(retained, "runner_closed", outcome = "cancelled")
        }
        payloads.clear()
        activeRequestIds.clear()
        scope.cancel()
        bridge.destroyUiSessionWorkScheduler(schedulerHandle)
    }

    fun submitSettingsAction(
        actionId: String,
        valueId: String,
        onResult: (UiSessionSnapshot) -> Unit,
        onError: (Throwable) -> Unit,
    ) {
        submitMutation(
            commandName = "performSettingsAction",
            operation = { it.performSettingsAction(actionId, valueId) },
            onResult = onResult,
            onError = onError,
        )
    }

    fun submitAircraftLibraryAction(
        actionId: String,
        sourceJson: String,
        onResult: (UiSessionSnapshot) -> Unit,
        onError: (Throwable) -> Unit,
    ) {
        submitMutation(
            commandName = "performAircraftLibraryAction",
            operation = { it.performAircraftLibraryAction(actionId, sourceJson) },
            onResult = onResult,
            onError = onError,
        )
    }

    private fun submitMutation(
        commandName: String,
        operation: (NativeUiSession) -> UiSessionSnapshot,
        onResult: (UiSessionSnapshot) -> Unit,
        onError: (Throwable) -> Unit,
    ) {
        val mutation = SessionMutation(commandName, operation, onResult, onError)
        if (closed || mutationQueue.trySend(mutation).isFailure) {
            onError(CancellationException("session work runner is closed"))
        }
    }

    fun submitOverlay(
        viewport: MapViewportState,
        widthPx: Double,
        heightPx: Double,
        pointDisplayScale: Double,
        fetchResource: (CoreResourceRequest) -> ByteArray,
        onResult: (MapOverlayQueryOutcome) -> Unit,
        onError: (Throwable) -> Unit,
        onDropped: (String) -> Unit = {},
    ) {
        request(
            OverlayPayload(
                viewport = viewport,
                widthPx = widthPx,
                heightPx = heightPx,
                pointDisplayScale = pointDisplayScale,
                fetchResource = fetchResource,
                onResult = onResult,
                onError = onError,
                onDropped = onDropped,
            ),
        )
    }

    suspend fun queryOverlay(
        viewport: MapViewportState,
        widthPx: Double,
        heightPx: Double,
        pointDisplayScale: Double,
        fetchResource: (CoreResourceRequest) -> ByteArray,
    ): MapOverlayQueryOutcome {
        val deferred = CompletableDeferred<MapOverlayQueryOutcome>()
        withContext(Dispatchers.Main.immediate) {
            submitOverlay(
                viewport = viewport,
                widthPx = widthPx,
                heightPx = heightPx,
                pointDisplayScale = pointDisplayScale,
                fetchResource = fetchResource,
                onResult = { deferred.complete(it) },
                onError = { deferred.completeExceptionally(it) },
                onDropped = { reason ->
                    deferred.completeExceptionally(CancellationException(reason))
                },
            )
        }
        return deferred.await()
    }

    fun submitMapSelection(
        viewport: MapViewportState,
        widthPx: Double,
        heightPx: Double,
        click: LatLonPoint,
        pointDisplayScale: Double,
        fetchResource: (CoreResourceRequest) -> ByteArray,
        onResult: (MapSelectionQueryResult) -> Unit,
        onError: (Throwable) -> Unit,
        onDropped: (String) -> Unit = {},
    ) {
        request(
            MapSelectionPayload(
                viewport = viewport,
                widthPx = widthPx,
                heightPx = heightPx,
                click = click,
                pointDisplayScale = pointDisplayScale,
                fetchResource = fetchResource,
                onResult = onResult,
                onError = onError,
                onDropped = onDropped,
            ),
        )
    }

    suspend fun queryMapSelection(
        viewport: MapViewportState,
        widthPx: Double,
        heightPx: Double,
        click: LatLonPoint,
        pointDisplayScale: Double,
        fetchResource: (CoreResourceRequest) -> ByteArray,
    ): MapSelectionQueryResult {
        val deferred = CompletableDeferred<MapSelectionQueryResult>()
        withContext(Dispatchers.Main.immediate) {
            submitMapSelection(
                viewport = viewport,
                widthPx = widthPx,
                heightPx = heightPx,
                click = click,
                pointDisplayScale = pointDisplayScale,
                fetchResource = fetchResource,
                onResult = { deferred.complete(it) },
                onError = { deferred.completeExceptionally(it) },
                onDropped = { reason ->
                    deferred.completeExceptionally(CancellationException(reason))
                },
            )
        }
        return deferred.await()
    }

    fun submitMapSelectionForNavRef(
        viewport: MapViewportState,
        widthPx: Double,
        heightPx: Double,
        navRef: NavRef,
        pointDisplayScale: Double,
        fetchResource: (CoreResourceRequest) -> ByteArray,
        onResult: (MapSelectionForNavRefResult) -> Unit,
        onError: (Throwable) -> Unit,
        onDropped: (String) -> Unit = {},
    ) {
        request(
            MapSelectionForNavRefPayload(
                viewport = viewport,
                widthPx = widthPx,
                heightPx = heightPx,
                navRef = navRef,
                pointDisplayScale = pointDisplayScale,
                fetchResource = fetchResource,
                onResult = onResult,
                onError = onError,
                onDropped = onDropped,
            ),
        )
    }

    suspend fun queryTerrainOverlay(
        viewport: MapViewportState,
        widthPx: Double,
        heightPx: Double,
        decodedCacheKeys: Collection<String>,
        inFlightCacheKeys: Collection<String>,
        fetchResource: (CoreResourceRequest) -> ByteArray,
    ): TerrainOverlayQueryResult = awaitPayload { onResult, onError, onDropped ->
        TerrainOverlayPayload(
            viewport = viewport,
            widthPx = widthPx,
            heightPx = heightPx,
            decodedCacheKeys = decodedCacheKeys.toList(),
            inFlightCacheKeys = inFlightCacheKeys.toList(),
            fetchResource = fetchResource,
            onResult = onResult,
            onError = onError,
            onDropped = onDropped,
        )
    }

    suspend fun renderTerrainOverlayTile(
        request: TerrainOverlayTileRequest,
        aircraftAltitudeFt: Double,
        fetchResource: (CoreResourceRequest) -> ByteArray,
    ): ByteArray = awaitPayload { onResult, onError, onDropped ->
        TerrainTilePayload(
            request = request,
            aircraftAltitudeFt = aircraftAltitudeFt,
            fetchResource = fetchResource,
            onResult = onResult,
            onError = onError,
            onDropped = onDropped,
        )
    }

    suspend fun queryNexradOverlay(
        viewport: MapViewportState,
        widthPx: Double,
        heightPx: Double,
        fetchResource: (CoreResourceRequest) -> ByteArray,
    ): NexradOverlayQueryResult = awaitPayload { onResult, onError, onDropped ->
        NexradOverlayPayload(
            viewport = viewport,
            widthPx = widthPx,
            heightPx = heightPx,
            fetchResource = fetchResource,
            onResult = onResult,
            onError = onError,
            onDropped = onDropped,
        )
    }

    suspend fun nexradTileBytes(
        src: String,
        fetchResource: (CoreResourceRequest) -> ByteArray,
    ): ByteArray = awaitPayload { onResult, onError, onDropped ->
        NexradTilePayload(
            src = src,
            fetchResource = fetchResource,
            onResult = onResult,
            onError = onError,
            onDropped = onDropped,
        )
    }

    suspend fun chartAssetBytes(
        chartId: String,
        assetKind: String,
        fetchResource: (CoreResourceRequest) -> ByteArray,
    ): ByteArray = awaitPayload { onResult, onError, onDropped ->
        ChartAssetPayload(
            chartId = chartId,
            assetKind = assetKind,
            fetchResource = fetchResource,
            onResult = onResult,
            onError = onError,
            onDropped = onDropped,
        )
    }

    private suspend fun <T> awaitPayload(
        create: (
            onResult: (T) -> Unit,
            onError: (Throwable) -> Unit,
            onDropped: (String) -> Unit,
        ) -> WorkPayload,
    ): T {
        val deferred = CompletableDeferred<T>()
        val requestId = withContext(Dispatchers.Main.immediate) {
            request(
                create(
                    { deferred.complete(it) },
                    { deferred.completeExceptionally(it) },
                    { reason -> deferred.completeExceptionally(CancellationException(reason)) },
                ),
            )
        }
        return try {
            deferred.await()
        } catch (error: CancellationException) {
            if (requestId != null) {
                scope.launch {
                    payloads.remove(requestId)?.let { retained ->
                        retained.payload.dropped("caller_cancelled")
                        logFinished(retained, "caller_cancelled", outcome = "cancelled")
                    }
                }
            }
            throw error
        }
    }

    private fun request(payload: WorkPayload): Long? {
        if (closed) {
            payload.dropped("runner_closed")
            return null
        }
        val request = UiSessionWorkRequest(
            id = nextRequestId++,
            kind = payload.kind,
            coalesceKey = payload.coalesceKey,
            requestedAtMs = SystemClock.elapsedRealtime(),
        )
        val retained = RetainedWork(request = request, payload = payload)
        payloads[request.id] = retained
        logEvent(
            "requested",
            retained,
            "retained_count" to payloads.size,
        )
        val decision = json.decodeFromString<UiSessionWorkRequestDecision>(
            bridge.uiSessionWorkSchedulerRequestJson(
                schedulerHandle,
                json.encodeToString(request),
            ),
        )
        when (decision) {
            is UiSessionWorkRequestDecision.Start -> start(decision.request)
            is UiSessionWorkRequestDecision.Queued -> {
                logEvent(
                    "queued",
                    retained,
                    "replaced_request_id" to (decision.replacedRequestId ?: 0L),
                    "retained_count" to payloads.size,
                )
                decision.replacedRequestId?.let { replacedRequestId ->
                    payloads.remove(replacedRequestId)?.let { replaced ->
                        replaced.payload.dropped("replaced_by_newer_pending")
                        logFinished(
                            replaced,
                            action = "replaced_by_newer_pending",
                            outcome = "cancelled",
                        )
                    }
                }
            }
        }
        return request.id
    }

    private fun start(request: UiSessionWorkRequest) {
        if (closed) {
            return
        }
        val retained = payloads[request.id]
        if (retained == null) {
            val completion = complete(request.id)
            completion.next?.let { next ->
                if (!closed) {
                    start(next)
                }
            }
            return
        }
        retained.startedAtMs = SystemClock.elapsedRealtime()
        activeRequestIds += request.id
        logEvent(
            "started",
            retained,
            "queue_ms" to (retained.startedAtMs - request.requestedAtMs).coerceAtLeast(0L),
            "active_count" to activeRequestIds.size,
            "retained_count" to payloads.size,
        )
        scope.launch {
            val operationMetrics = if (perfMetricsEnabled) PagedSessionOperationMetrics() else null
            val dispatchedAtMs = SystemClock.elapsedRealtime()
            var workerStartedAtMs = dispatchedAtMs
            var workerFinishedAtMs = dispatchedAtMs
            var executionThread = "unknown"
            var executionOnMainThread = false
            val outcome = runCatching {
                withContext(Dispatchers.IO) {
                    workerStartedAtMs = SystemClock.elapsedRealtime()
                    executionThread = Thread.currentThread().name.replace(' ', '_')
                    executionOnMainThread = Looper.myLooper() == Looper.getMainLooper()
                    try {
                        retained.payload.run(uiSession, operationMetrics)
                    } finally {
                        workerFinishedAtMs = SystemClock.elapsedRealtime()
                    }
                }
            }
            if (closed) {
                payloads.remove(request.id)
                activeRequestIds -= request.id
                return@launch
            }
            val workerReturnedAtMs = SystemClock.elapsedRealtime()
            val schedulerStartedAtMs = workerReturnedAtMs
            val completion = complete(request.id)
            val schedulerMs = SystemClock.elapsedRealtime() - schedulerStartedAtMs
            payloads.remove(request.id)
            activeRequestIds -= request.id
            if (closed) {
                return@launch
            }
            val landingStartedAtMs = SystemClock.elapsedRealtime()
            val action: String
            when (completion.resultAction) {
                is UiSessionWorkResultAction.Land -> {
                    action = if (outcome.isSuccess) "land" else "failed"
                    outcome
                        .onSuccess { retained.payload.land(it) }
                        .onFailure { retained.payload.failed(it) }
                }
                is UiSessionWorkResultAction.Drop -> {
                    action = completion.resultAction.reason
                    retained.payload.dropped(completion.resultAction.reason)
                }
            }
            val finishedAtMs = SystemClock.elapsedRealtime()
            val operationMetricsSnapshot = operationMetrics?.snapshot()
            operationMetricsSnapshot?.resourceRounds?.forEach { round ->
                logEvent(
                    "resource_frontier",
                    retained,
                    "round" to round.index,
                    "width" to round.resourceIds.size,
                    "source_kinds" to round.sourceKinds.entries.joinToString("+") { (kind, count) ->
                        "$kind:$count"
                    },
                    "resource_ids" to round.resourceIds.joinToString("+"),
                    "fetch_wall_us" to round.fetchWallUs,
                    "fetch_work_us" to round.fetchWorkUs,
                    "max_concurrency" to round.maxConcurrency,
                    "ingest_us" to round.ingestUs,
                )
            }
            logFinished(
                retained = retained,
                action = action,
                outcome = if (outcome.isSuccess) "success" else "error",
                extra = arrayOf<Pair<String, Any?>>(
                    "queue_ms" to (retained.startedAtMs - request.requestedAtMs).coerceAtLeast(0L),
                    "dispatcher_wait_ms" to (workerStartedAtMs - dispatchedAtMs).coerceAtLeast(0L),
                    "work_ms" to (workerFinishedAtMs - workerStartedAtMs).coerceAtLeast(0L),
                    "delivery_ms" to (workerReturnedAtMs - workerFinishedAtMs).coerceAtLeast(0L),
                    "scheduler_ms" to schedulerMs.coerceAtLeast(0L),
                    "landing_ms" to (finishedAtMs - landingStartedAtMs).coerceAtLeast(0L),
                    "total_ms" to (finishedAtMs - request.requestedAtMs).coerceAtLeast(0L),
                    "execution_thread" to executionThread,
                    "main_thread" to executionOnMainThread,
                    "active_count" to activeRequestIds.size,
                    "retained_count" to payloads.size,
                ) + (
                    operationMetricsSnapshot
                        ?.let(::operationMetricFields)
                        ?: emptyArray()
                    ),
            )
            completion.next?.let { next ->
                if (!closed) {
                    start(next)
                }
            }
        }
    }

    private fun complete(requestId: Long): UiSessionWorkCompletionDecision {
        if (closed) {
            return droppedCompletion("runner_closed")
        }
        return json.decodeFromString<UiSessionWorkCompletionDecision>(
            bridge.uiSessionWorkSchedulerCompleteJson(
                schedulerHandle,
                requestId,
            ),
        )
    }

    private fun logFinished(
        retained: RetainedWork,
        action: String,
        outcome: String,
        extra: Array<Pair<String, Any?>> = emptyArray(),
    ) {
        logEvent("finished", retained, "action" to action, "outcome" to outcome, *extra)
    }

    private fun logEvent(
        event: String,
        retained: RetainedWork,
        vararg fields: Pair<String, Any?>,
    ) {
        if (!perfMetricsEnabled) return
        val request = retained.request
        val base = listOf(
            "event=$event",
            "request_id=${request.id}",
            "kind=${json.encodeToString(request.kind).removeSurrounding("\"")}",
            "coalesce_key=${request.coalesceKey ?: "-"}",
        )
        val encodedFields = fields.map { (name, value) -> "$name=${value ?: "-"}" }
        Log.i(UiSessionWorkLogTag, (base + encodedFields).joinToString(" "))
    }
}

private data class RetainedWork(
    val request: UiSessionWorkRequest,
    val payload: WorkPayload,
    var startedAtMs: Long = request.requestedAtMs,
)

private data class SessionMutation(
    val commandName: String,
    val operation: (NativeUiSession) -> UiSessionSnapshot,
    val onResult: (UiSessionSnapshot) -> Unit,
    val onError: (Throwable) -> Unit,
)

private fun operationMetricFields(
    metrics: PagedSessionOperationMetricsSnapshot,
): Array<Pair<String, Any?>> = arrayOf(
    "core_calls" to metrics.coreCallCount,
    "core_us" to metrics.coreCallUs,
    "resource_rounds" to metrics.resourceRoundCount,
    "resource_requests" to metrics.resourceRequestCount,
    "resource_loads" to metrics.resourceLoadCount,
    "resource_cache_hits" to metrics.resourceCacheHitCount,
    "resource_bytes" to metrics.resourceBytes,
    "resource_fetch_us" to metrics.resourceFetchUs,
    "resource_fetch_wall_us" to metrics.resourceRounds.sumOf { it.fetchWallUs },
    "resource_max_concurrency" to (metrics.resourceRounds.maxOfOrNull { it.maxConcurrency } ?: 0),
    "resource_ingest_us" to metrics.resourceIngestUs,
)

private fun droppedCompletion(reason: String): UiSessionWorkCompletionDecision =
    UiSessionWorkCompletionDecision(
        resultAction = UiSessionWorkResultAction.Drop(reason),
        next = null,
    )

private sealed class WorkPayload(
    val kind: UiSessionWorkKind,
    val coalesceKey: String?,
) {
    abstract fun run(
        uiSession: NativeUiSession,
        metrics: PagedSessionOperationMetrics?,
    ): WorkResult
    abstract fun land(result: WorkResult)
    abstract fun failed(error: Throwable)
    open fun dropped(reason: String) {
    }
}

private class OverlayPayload(
    private val viewport: MapViewportState,
    private val widthPx: Double,
    private val heightPx: Double,
    private val pointDisplayScale: Double,
    private val fetchResource: (CoreResourceRequest) -> ByteArray,
    private val onResult: (MapOverlayQueryOutcome) -> Unit,
    private val onError: (Throwable) -> Unit,
    private val onDropped: (String) -> Unit,
) : WorkPayload(UiSessionWorkKind.MapOverlay, "map_overlay") {
    override fun run(
        uiSession: NativeUiSession,
        metrics: PagedSessionOperationMetrics?,
    ): WorkResult {
        return WorkResult.Overlay(
            uiSession.queryMapOverlay(
                viewport = viewport,
                widthPx = widthPx,
                heightPx = heightPx,
                pointDisplayScale = pointDisplayScale,
                fetchResource = fetchResource,
                metrics = metrics,
            ),
        )
    }

    override fun land(result: WorkResult) {
        onResult((result as WorkResult.Overlay).outcome)
    }

    override fun failed(error: Throwable) {
        onError(error)
    }

    override fun dropped(reason: String) {
        super.dropped(reason)
        onDropped(reason)
    }
}

private class MapSelectionPayload(
    private val viewport: MapViewportState,
    private val widthPx: Double,
    private val heightPx: Double,
    private val click: LatLonPoint,
    private val pointDisplayScale: Double,
    private val fetchResource: (CoreResourceRequest) -> ByteArray,
    private val onResult: (MapSelectionQueryResult) -> Unit,
    private val onError: (Throwable) -> Unit,
    private val onDropped: (String) -> Unit,
) : WorkPayload(UiSessionWorkKind.MapSelection, "map_selection") {
    override fun run(
        uiSession: NativeUiSession,
        metrics: PagedSessionOperationMetrics?,
    ): WorkResult {
        return WorkResult.MapSelection(
            uiSession.queryMapSelection(
                viewport = viewport,
                widthPx = widthPx,
                heightPx = heightPx,
                click = click,
                pointDisplayScale = pointDisplayScale,
                fetchResource = fetchResource,
                metrics = metrics,
            ),
        )
    }

    override fun land(result: WorkResult) {
        onResult((result as WorkResult.MapSelection).result)
    }

    override fun failed(error: Throwable) {
        onError(error)
    }

    override fun dropped(reason: String) {
        super.dropped(reason)
        onDropped(reason)
    }
}

private class MapSelectionForNavRefPayload(
    private val viewport: MapViewportState,
    private val widthPx: Double,
    private val heightPx: Double,
    private val navRef: NavRef,
    private val pointDisplayScale: Double,
    private val fetchResource: (CoreResourceRequest) -> ByteArray,
    private val onResult: (MapSelectionForNavRefResult) -> Unit,
    private val onError: (Throwable) -> Unit,
    private val onDropped: (String) -> Unit,
) : WorkPayload(UiSessionWorkKind.MapSelectionForNavRef, "map_selection_for_nav_ref") {
    override fun run(
        uiSession: NativeUiSession,
        metrics: PagedSessionOperationMetrics?,
    ): WorkResult {
        return WorkResult.MapSelectionForNavRef(
            uiSession.queryMapSelectionForNavRef(
                viewport = viewport,
                widthPx = widthPx,
                heightPx = heightPx,
                navRef = navRef,
                pointDisplayScale = pointDisplayScale,
                fetchResource = fetchResource,
                metrics = metrics,
            ),
        )
    }

    override fun land(result: WorkResult) {
        onResult((result as WorkResult.MapSelectionForNavRef).result)
    }

    override fun failed(error: Throwable) {
        onError(error)
    }

    override fun dropped(reason: String) {
        super.dropped(reason)
        onDropped(reason)
    }
}

private class TerrainOverlayPayload(
    private val viewport: MapViewportState,
    private val widthPx: Double,
    private val heightPx: Double,
    private val decodedCacheKeys: List<String>,
    private val inFlightCacheKeys: List<String>,
    private val fetchResource: (CoreResourceRequest) -> ByteArray,
    private val onResult: (TerrainOverlayQueryResult) -> Unit,
    private val onError: (Throwable) -> Unit,
    private val onDropped: (String) -> Unit,
) : WorkPayload(UiSessionWorkKind.TerrainOverlay, "terrain_overlay") {
    override fun run(
        uiSession: NativeUiSession,
        metrics: PagedSessionOperationMetrics?,
    ): WorkResult = WorkResult.TerrainOverlay(
        uiSession.queryTerrainOverlay(
            viewport = viewport,
            widthPx = widthPx,
            heightPx = heightPx,
            decodedCacheKeys = decodedCacheKeys,
            inFlightCacheKeys = inFlightCacheKeys,
            fetchResource = fetchResource,
            metrics = metrics,
        ),
    )

    override fun land(result: WorkResult) {
        onResult((result as WorkResult.TerrainOverlay).result)
    }

    override fun failed(error: Throwable) {
        onError(error)
    }

    override fun dropped(reason: String) {
        super.dropped(reason)
        onDropped(reason)
    }
}

private class TerrainTilePayload(
    private val request: TerrainOverlayTileRequest,
    private val aircraftAltitudeFt: Double,
    private val fetchResource: (CoreResourceRequest) -> ByteArray,
    private val onResult: (ByteArray) -> Unit,
    private val onError: (Throwable) -> Unit,
    private val onDropped: (String) -> Unit,
) : WorkPayload(UiSessionWorkKind.TerrainTile, "terrain_tile:${request.cacheKey}") {
    override fun run(
        uiSession: NativeUiSession,
        metrics: PagedSessionOperationMetrics?,
    ): WorkResult = WorkResult.TerrainTile(
        uiSession.renderTerrainOverlayTile(request, aircraftAltitudeFt, fetchResource, metrics),
    )

    override fun land(result: WorkResult) {
        onResult((result as WorkResult.TerrainTile).bytes)
    }

    override fun failed(error: Throwable) {
        onError(error)
    }

    override fun dropped(reason: String) {
        super.dropped(reason)
        onDropped(reason)
    }
}

private class NexradOverlayPayload(
    private val viewport: MapViewportState,
    private val widthPx: Double,
    private val heightPx: Double,
    private val fetchResource: (CoreResourceRequest) -> ByteArray,
    private val onResult: (NexradOverlayQueryResult) -> Unit,
    private val onError: (Throwable) -> Unit,
    private val onDropped: (String) -> Unit,
) : WorkPayload(UiSessionWorkKind.NexradOverlay, "nexrad_overlay") {
    override fun run(
        uiSession: NativeUiSession,
        metrics: PagedSessionOperationMetrics?,
    ): WorkResult = WorkResult.NexradOverlay(
        uiSession.queryNexradOverlay(
            viewport = viewport,
            widthPx = widthPx,
            heightPx = heightPx,
            fetchResource = fetchResource,
            metrics = metrics,
        ),
    )

    override fun land(result: WorkResult) {
        onResult((result as WorkResult.NexradOverlay).result)
    }

    override fun failed(error: Throwable) {
        onError(error)
    }

    override fun dropped(reason: String) {
        super.dropped(reason)
        onDropped(reason)
    }
}

private class NexradTilePayload(
    private val src: String,
    private val fetchResource: (CoreResourceRequest) -> ByteArray,
    private val onResult: (ByteArray) -> Unit,
    private val onError: (Throwable) -> Unit,
    private val onDropped: (String) -> Unit,
) : WorkPayload(UiSessionWorkKind.NexradTile, "nexrad_tile:$src") {
    override fun run(
        uiSession: NativeUiSession,
        metrics: PagedSessionOperationMetrics?,
    ): WorkResult = WorkResult.NexradTile(
        uiSession.nexradTileBytes(src, fetchResource, metrics),
    )

    override fun land(result: WorkResult) {
        onResult((result as WorkResult.NexradTile).bytes)
    }

    override fun failed(error: Throwable) {
        onError(error)
    }

    override fun dropped(reason: String) {
        super.dropped(reason)
        onDropped(reason)
    }
}

private class ChartAssetPayload(
    private val chartId: String,
    private val assetKind: String,
    private val fetchResource: (CoreResourceRequest) -> ByteArray,
    private val onResult: (ByteArray) -> Unit,
    private val onError: (Throwable) -> Unit,
    private val onDropped: (String) -> Unit,
) : WorkPayload(UiSessionWorkKind.ChartAsset, "chart_asset:$assetKind:$chartId") {
    override fun run(
        uiSession: NativeUiSession,
        metrics: PagedSessionOperationMetrics?,
    ): WorkResult = WorkResult.ChartAsset(
        uiSession.chartAssetBytes(chartId, assetKind, fetchResource, metrics),
    )

    override fun land(result: WorkResult) {
        onResult((result as WorkResult.ChartAsset).bytes)
    }

    override fun failed(error: Throwable) {
        onError(error)
    }

    override fun dropped(reason: String) {
        super.dropped(reason)
        onDropped(reason)
    }
}

private sealed class WorkResult {
    data class ChartAsset(val bytes: ByteArray) : WorkResult()
    data class Overlay(val outcome: MapOverlayQueryOutcome) : WorkResult()
    data class MapSelection(val result: MapSelectionQueryResult) : WorkResult()
    data class MapSelectionForNavRef(val result: MapSelectionForNavRefResult) : WorkResult()
    data class NexradOverlay(val result: NexradOverlayQueryResult) : WorkResult()
    data class NexradTile(val bytes: ByteArray) : WorkResult()
    data class TerrainOverlay(val result: TerrainOverlayQueryResult) : WorkResult()
    data class TerrainTile(val bytes: ByteArray) : WorkResult()
}

private val json = Json
