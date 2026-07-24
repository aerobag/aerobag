// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

@file:OptIn(org.aerobag.app.domain.RawUiSessionWorkApi::class)

package org.aerobag.app

import android.os.SystemClock
import android.util.Log
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
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

private const val UiSessionWorkLogTag = "AerobagSessionWork"

class UiSessionWorkRunner(
    private val uiSession: NativeUiSession,
    private val bridge: NativeBridge = NativeBindings,
) {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)
    private val schedulerHandle = bridge.createUiSessionWorkScheduler()
    private val payloads = mutableMapOf<Long, WorkPayload>()
    private var nextRequestId = 1L
    private var closed = false

    fun close() {
        if (closed) return
        closed = true
        payloads.values.forEach { it.dropped("runner_closed") }
        payloads.clear()
        scope.cancel()
        bridge.destroyUiSessionWorkScheduler(schedulerHandle)
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
    ): MapSelectionQueryResult {
        val deferred = CompletableDeferred<MapSelectionQueryResult>()
        withContext(Dispatchers.Main.immediate) {
            submitMapSelection(
                viewport = viewport,
                widthPx = widthPx,
                heightPx = heightPx,
                click = click,
                pointDisplayScale = pointDisplayScale,
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
                onResult = onResult,
                onError = onError,
                onDropped = onDropped,
            ),
        )
    }

    private fun request(payload: WorkPayload) {
        if (closed) {
            payload.dropped("runner_closed")
            return
        }
        val request = WorkRequestWire(
            id = nextRequestId++,
            kind = payload.kind,
            coalesceKey = payload.coalesceKey,
            requestedAtMs = SystemClock.elapsedRealtime(),
        )
        payloads[request.id] = payload
        val decision = decodeRequestDecision(
            bridge.uiSessionWorkSchedulerRequestJson(
                schedulerHandle,
                json.encodeToString(request),
            ),
        )
        when (decision) {
            is RequestDecision.Start -> start(decision.request)
            is RequestDecision.Queued -> {
                decision.replacedRequestId?.let { replacedRequestId ->
                    payloads.remove(replacedRequestId)?.dropped("replaced_by_newer_pending")
                }
            }
        }
    }

    private fun start(request: WorkRequestWire) {
        if (closed) {
            return
        }
        val payload = payloads[request.id]
        if (payload == null) {
            complete(request.id)
            return
        }
        scope.launch {
            val outcome = runCatching {
                withContext(Dispatchers.IO) {
                    payload.run(uiSession)
                }
            }
            if (closed) {
                payloads.remove(request.id)
                return@launch
            }
            val completion = complete(request.id)
            payloads.remove(request.id)
            if (closed) {
                return@launch
            }
            when (completion.resultAction) {
                is ResultAction.Land -> {
                    outcome
                        .onSuccess { payload.land(it) }
                        .onFailure { payload.failed(it) }
                }
                is ResultAction.Drop -> {
                    payload.dropped(completion.resultAction.reason)
                }
            }
            completion.next?.let { next ->
                if (!closed) {
                    start(next)
                }
            }
        }
    }

    private fun complete(requestId: Long): CompletionDecision {
        if (closed) {
            return droppedCompletion("runner_closed")
        }
        return decodeCompletionDecision(
            bridge.uiSessionWorkSchedulerCompleteJson(
                schedulerHandle,
                requestId,
            ),
        )
    }
}

private fun droppedCompletion(reason: String): CompletionDecision =
    CompletionDecision(
        resultAction = ResultAction.Drop(reason),
        next = null,
    )

private sealed class WorkPayload(
    val kind: String,
    val coalesceKey: String?,
) {
    abstract fun run(uiSession: NativeUiSession): WorkResult
    abstract fun land(result: WorkResult)
    abstract fun failed(error: Throwable)
    open fun dropped(reason: String) {
        Log.d(UiSessionWorkLogTag, "dropped kind=$kind reason=$reason")
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
) : WorkPayload("map_overlay", "map_overlay") {
    override fun run(uiSession: NativeUiSession): WorkResult {
        return WorkResult.Overlay(
            uiSession.queryMapOverlay(
                viewport = viewport,
                widthPx = widthPx,
                heightPx = heightPx,
                pointDisplayScale = pointDisplayScale,
                fetchResource = fetchResource,
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
    private val onResult: (MapSelectionQueryResult) -> Unit,
    private val onError: (Throwable) -> Unit,
    private val onDropped: (String) -> Unit,
) : WorkPayload("map_selection", "map_selection") {
    override fun run(uiSession: NativeUiSession): WorkResult {
        return WorkResult.MapSelection(
            uiSession.queryMapSelection(
                viewport = viewport,
                widthPx = widthPx,
                heightPx = heightPx,
                click = click,
                pointDisplayScale = pointDisplayScale,
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
    private val onResult: (MapSelectionForNavRefResult) -> Unit,
    private val onError: (Throwable) -> Unit,
    private val onDropped: (String) -> Unit,
) : WorkPayload("map_selection_for_nav_ref", "map_selection_for_nav_ref") {
    override fun run(uiSession: NativeUiSession): WorkResult {
        return WorkResult.MapSelectionForNavRef(
            uiSession.queryMapSelectionForNavRef(
                viewport = viewport,
                widthPx = widthPx,
                heightPx = heightPx,
                navRef = navRef,
                pointDisplayScale = pointDisplayScale,
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

private sealed class WorkResult {
    data class Overlay(val outcome: MapOverlayQueryOutcome) : WorkResult()
    data class MapSelection(val result: MapSelectionQueryResult) : WorkResult()
    data class MapSelectionForNavRef(val result: MapSelectionForNavRefResult) : WorkResult()
}

@Serializable
private data class WorkRequestWire(
    val id: Long,
    val kind: String,
    @SerialName("coalesce_key")
    val coalesceKey: String?,
    @SerialName("requested_at_ms")
    val requestedAtMs: Long,
)

private sealed class RequestDecision {
    data class Start(val request: WorkRequestWire) : RequestDecision()
    data class Queued(val replacedRequestId: Long?) : RequestDecision()
}

private data class CompletionDecision(
    val resultAction: ResultAction,
    val next: WorkRequestWire?,
)

private sealed class ResultAction {
    data object Land : ResultAction()
    data class Drop(val reason: String) : ResultAction()
}

private val json = Json {
    ignoreUnknownKeys = true
}

private fun decodeRequestDecision(payload: String): RequestDecision {
    val obj = json.parseToJsonElement(payload).jsonObject
    return when (obj.requiredString("kind")) {
        "start" -> RequestDecision.Start(obj.requiredObject("request").decodeWorkRequest())
        "queued" -> RequestDecision.Queued(obj.optionalLong("replaced_request_id"))
        else -> error("unknown ui session work request decision: $payload")
    }
}

private fun decodeCompletionDecision(payload: String): CompletionDecision {
    val obj = json.parseToJsonElement(payload).jsonObject
    val actionObj = obj.requiredObject("result_action")
    val action = when (actionObj.requiredString("kind")) {
        "land" -> ResultAction.Land
        "drop" -> ResultAction.Drop(actionObj.requiredString("reason"))
        else -> error("unknown ui session work result action: $payload")
    }
    return CompletionDecision(
        resultAction = action,
        next = obj["next"]?.takeIf { it !is kotlinx.serialization.json.JsonNull }?.jsonObject?.decodeWorkRequest(),
    )
}

private fun JsonObject.decodeWorkRequest(): WorkRequestWire {
    return WorkRequestWire(
        id = requiredLong("id"),
        kind = requiredString("kind"),
        coalesceKey = optionalString("coalesce_key"),
        requestedAtMs = requiredLong("requested_at_ms"),
    )
}

private fun JsonObject.requiredObject(name: String): JsonObject {
    return this[name]?.jsonObject ?: error("missing object field $name")
}

private fun JsonObject.requiredString(name: String): String {
    return this[name]?.jsonPrimitive?.content ?: error("missing string field $name")
}

private fun JsonObject.optionalString(name: String): String? {
    return this[name]?.jsonPrimitive?.content
}

private fun JsonObject.requiredLong(name: String): Long {
    return requiredString(name).toLong()
}

private fun JsonObject.optionalLong(name: String): Long? {
    return this[name]?.jsonPrimitive?.content?.toLongOrNull()
}
