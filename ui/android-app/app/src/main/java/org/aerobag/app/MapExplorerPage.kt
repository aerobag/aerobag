// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app

import android.Manifest
import android.content.ActivityNotFoundException
import android.content.Context
import android.content.Intent
import android.content.SharedPreferences
import android.content.pm.PackageManager
import android.graphics.BitmapFactory
import android.graphics.Matrix
import android.graphics.Paint
import android.graphics.Path as AndroidPath
import android.graphics.RectF
import android.graphics.Typeface
import android.net.Uri
import android.os.Build
import android.os.Bundle
import android.os.SystemClock
import android.util.Log
import android.view.KeyEvent as AndroidKeyEvent
import android.view.MotionEvent
import android.widget.Toast
import java.util.LinkedHashMap
import java.net.HttpURLConnection
import androidx.annotation.DrawableRes
import androidx.activity.result.contract.ActivityResultContracts
import androidx.activity.ComponentActivity
import androidx.activity.compose.BackHandler
import androidx.activity.compose.setContent
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.animateColorAsState
import androidx.compose.animation.core.animateDpAsState
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.slideInVertically
import androidx.compose.animation.slideOutVertically
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.clickable
import androidx.compose.foundation.focusable
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.gestures.detectTransformGestures
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.verticalScroll
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.ime
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.layout.wrapContentSize
import androidx.compose.foundation.layout.wrapContentWidth
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyListState
import androidx.compose.foundation.lazy.items as lazyColumnItems
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.lazy.grid.GridCells
import androidx.compose.foundation.lazy.grid.LazyVerticalGrid
import androidx.compose.foundation.lazy.grid.items as lazyGridItems
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.Checkbox
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.SideEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableLongStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.produceState
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.foundation.rememberScrollState
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.runtime.withFrameNanos
import androidx.compose.ui.Alignment
import androidx.compose.ui.ExperimentalComposeUiApi
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Shadow
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.PathEffect
import androidx.compose.ui.graphics.asAndroidBitmap
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.StrokeJoin
import androidx.compose.ui.graphics.toArgb
import androidx.compose.ui.graphics.drawscope.clipRect
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.graphics.drawscope.rotate
import androidx.compose.ui.graphics.drawscope.scale
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.drawscope.translate
import androidx.compose.ui.graphics.nativeCanvas
import androidx.compose.ui.graphics.lerp
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.draw.drawWithContent
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.input.key.onPreviewKeyEvent
import androidx.compose.ui.input.pointer.PointerId
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.input.pointer.pointerInteropFilter
import androidx.compose.ui.input.pointer.positionChanged
import androidx.compose.ui.layout.boundsInWindow
import androidx.compose.ui.layout.layout
import androidx.compose.ui.layout.onGloballyPositioned
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalUriHandler
import androidx.compose.ui.platform.LocalFocusManager
import androidx.compose.ui.platform.LocalSoftwareKeyboardController
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.selected
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.semantics.testTagsAsResourceId
import androidx.compose.ui.semantics.disabled
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.withStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextDecoration
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.OffsetMapping
import androidx.compose.ui.text.input.PlatformImeOptions
import androidx.compose.ui.text.input.TransformedText
import androidx.compose.ui.text.input.VisualTransformation
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.IntSize
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.zIndex
import androidx.compose.ui.window.Popup
import androidx.compose.ui.window.PopupProperties
import androidx.core.content.ContextCompat
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.asCoroutineDispatcher
import kotlinx.coroutines.awaitAll
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.delay
import kotlinx.coroutines.ensureActive
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext
import kotlinx.coroutines.withTimeoutOrNull
import kotlinx.coroutines.yield
import org.aerobag.app.domain.ChartAirport
import org.aerobag.app.domain.ChartAsset
import org.aerobag.app.domain.AirwayPresentationPlan
import org.aerobag.app.domain.AirwaySuggestion
import org.aerobag.app.domain.WaypointIdentifierSuggestion
import org.aerobag.app.domain.toNavRef
import org.aerobag.app.domain.DerivedChartPageState
import org.aerobag.app.domain.FlightPlanEntryPreview
import org.aerobag.app.domain.FlightPlanDisplayRowKind
import org.aerobag.app.domain.FlightPlanDisplayRowUiView
import org.aerobag.app.domain.FlightPlanRowActionUiView
import org.aerobag.app.domain.FlightPlanRouteDistanceAnnotation
import org.aerobag.app.domain.FlightPlanRouteSegment
import org.aerobag.app.domain.FlightPlanRouteProjection
import org.aerobag.app.domain.FlightPlanUiState
import org.aerobag.app.domain.CoreResourceRequest
import org.aerobag.app.domain.AirspaceDisplayDecoration
import org.aerobag.app.domain.AirspaceDisplayLabel
import org.aerobag.app.domain.AirspaceDisplayPath
import org.aerobag.app.domain.AirspaceDisplaySubpath
import org.aerobag.app.domain.AirspaceLimitGlyph
import org.aerobag.app.domain.AirspaceScreenPoint
import org.aerobag.app.domain.LatLonPoint
import org.aerobag.app.domain.MapDisplayFrame
import org.aerobag.app.domain.MapFollowTargetGate
import org.aerobag.app.domain.MapLayerId
import org.aerobag.app.domain.MapOrientationMemory
import org.aerobag.app.domain.MapOrientationMode
import org.aerobag.app.domain.MapOverlayQueryResult
import org.aerobag.app.domain.MapSelectionAction
import org.aerobag.app.domain.MapSelectionDetailStatus
import org.aerobag.app.domain.MapSelectionHighlight
import org.aerobag.app.domain.MapSelectionItem
import org.aerobag.app.domain.MapSelectionActionEffect
import org.aerobag.app.domain.MapSelectionQueryResult
import org.aerobag.app.domain.MapFamilyOption
import org.aerobag.app.domain.MapViewportState
import org.aerobag.app.domain.NativeAppCoreAdapter
import org.aerobag.app.domain.NativeBindings
import org.aerobag.app.domain.NativeUiSession
import org.aerobag.app.domain.NavKvStore
import org.aerobag.app.domain.NavRef
import org.aerobag.app.domain.NavElementUiView
import org.aerobag.app.domain.OwnshipMode
import org.aerobag.app.domain.OwnshipSelection
import org.aerobag.app.domain.OwnshipSourceKind
import org.aerobag.app.domain.OwnshipSourceRegistration
import org.aerobag.app.domain.OwnshipSourceStatusUpdate
import org.aerobag.app.domain.PackageZipStore
import org.aerobag.app.domain.PlaybackStatus
import org.aerobag.app.domain.PlaybackUiState
import org.aerobag.app.domain.ProcedureKind
import org.aerobag.app.domain.ProcedureLoadOption
import org.aerobag.app.domain.ProcedureOptions
import org.aerobag.app.domain.ProcedureSummary
import org.aerobag.app.domain.RenderTile
import org.aerobag.app.domain.RenderTileSource
import org.aerobag.app.domain.RouteSegmentStatus
import org.aerobag.app.domain.RouteComponentViewKind
import org.aerobag.app.domain.RasterMapUiState
import org.aerobag.app.domain.ScreenPoint
import org.aerobag.app.domain.SequencingMode
import org.aerobag.app.domain.SituationControlInput
import org.aerobag.app.domain.SituationRingCandidate
import org.aerobag.app.domain.SituationSample
import org.aerobag.app.domain.SourceConnectionState
import org.aerobag.app.domain.TileStorageKind
import org.aerobag.app.domain.TerrainOverlayQueryResult
import org.aerobag.app.domain.TerrainOverlayTileRequest
import org.aerobag.app.domain.UiDebugState
import org.aerobag.app.domain.UiMapLayerToggleState
import org.aerobag.app.domain.UiDataStatusState
import org.aerobag.app.domain.VisibleAdsbTraffic
import org.aerobag.app.domain.UiTheme
import org.aerobag.app.domain.UiThemeLoader
import org.aerobag.app.domain.UiSessionSnapshot
import org.aerobag.app.domain.UiSurfaceStatusControlId
import org.aerobag.app.domain.VisibleMapFeature
import org.aerobag.app.domain.VisibleMetarFeature
import org.aerobag.app.domain.VisiblePirepFeature
import org.aerobag.app.domain.WeatherDetailUiView
import org.aerobag.app.domain.WeatherDetailSectionKind
import org.aerobag.app.domain.AirportNotamUiView
import org.aerobag.app.domain.PlateProcedureNotamDetail
import org.aerobag.app.domain.AirportInfoUiView
import org.aerobag.app.domain.AirportRunwayUiView
import org.aerobag.app.domain.applyPinchGesture
import org.aerobag.app.domain.clampZoom
import org.aerobag.app.domain.compassNeedleRotationDegrees
import org.aerobag.app.domain.createInitialImageViewport
import org.aerobag.app.domain.createPinchSnapshot
import org.aerobag.app.domain.distinctRenderTileCount
import org.aerobag.app.domain.dragImageViewport
import org.aerobag.app.domain.dragViewport
import org.aerobag.app.domain.imageDisplaySize
import org.aerobag.app.domain.kindForLog
import org.aerobag.app.domain.latLonToWorld
import org.aerobag.app.domain.mapFollowSyncViewportForCompletedGesture
import org.aerobag.app.domain.physicalDisplayMaxZoom
import org.aerobag.app.domain.preserveViewportForMap
import org.aerobag.app.domain.renderTileKey
import org.aerobag.app.domain.sameMapViewport
import org.aerobag.app.domain.scaleForZoom
import org.aerobag.app.domain.screenToWorld
import org.aerobag.app.domain.viewportCenterLatLon
import org.aerobag.app.domain.worldToLatLon
import org.aerobag.app.domain.zoomAroundPoint
import org.aerobag.app.domain.zoomImageAroundPoint
import org.aerobag.app.generated.NexradOverlayScreenPoint
import org.aerobag.app.generated.NexradOverlayCacheResource
import org.aerobag.app.generated.NexradOverlayTile
import org.aerobag.app.generated.UiStatusPlatformEffect
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.jsonPrimitive
import org.aerobag.app.generated.airportCircleMarkerPath
import org.aerobag.app.generated.airportFuelMarkerPath
import org.aerobag.app.generated.airportOpenMarkerSymbol
import org.aerobag.app.generated.hasActionSymbol
import org.aerobag.app.generated.fixTrianglePath
import org.aerobag.app.generated.heliportHPath
import org.aerobag.app.generated.mapSelectionSpotSymbol
import org.aerobag.app.generated.metarBknSymbol
import org.aerobag.app.generated.metarClearSymbol
import org.aerobag.app.generated.metarFewSymbol
import org.aerobag.app.generated.metarMissingSymbol
import org.aerobag.app.generated.metarOvcSymbol
import org.aerobag.app.generated.metarSctSymbol
import org.aerobag.app.generated.NavSymbolLayer
import org.aerobag.app.generated.obstacleDotRadius
import org.aerobag.app.generated.obstacleShortDotY
import org.aerobag.app.generated.obstacleShortPath
import org.aerobag.app.generated.obstacleTallDotY
import org.aerobag.app.generated.obstacleTallPath
import org.aerobag.app.generated.pirepGenericSymbol
import org.aerobag.app.generated.pirepLightIcingSymbol
import org.aerobag.app.generated.pirepLightTurbulenceSymbol
import org.aerobag.app.generated.pirepModerateIcingSymbol
import org.aerobag.app.generated.pirepModerateTurbulenceSymbol
import org.aerobag.app.generated.pirepSevereIcingSymbol
import org.aerobag.app.generated.pirepSevereTurbulenceSymbol
import org.aerobag.app.generated.seaplaneAnchorPath
import org.aerobag.app.generated.vorBandPath
import org.aerobag.app.generated.vorOuterHexPath
import org.aerobag.app.generated.weatherCameraSymbol
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.decodeFromString
import kotlinx.serialization.encodeToString
import java.io.BufferedOutputStream
import java.io.File
import java.nio.ByteBuffer
import java.security.MessageDigest
import java.util.concurrent.Executors
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.atomic.AtomicInteger
import java.util.concurrent.atomic.AtomicLong
import kotlin.math.abs
import kotlin.math.ceil
import kotlin.math.roundToInt
import kotlin.math.atan2
import kotlin.math.cos
import kotlin.math.hypot
import kotlin.math.max
import kotlin.math.min
import kotlin.math.pow
import kotlin.math.sin

internal data class NexradOverlayImage(
    val tile: NexradOverlayTile,
    val bitmap: androidx.compose.ui.graphics.ImageBitmap,
)

internal data class NexradOverlayFrame(
    val images: List<NexradOverlayImage>,
    val viewport: MapViewportState,
    val surfaceWidthPx: Float,
    val surfaceHeightPx: Float,
    val decodedImageCount: Int,
    val decodedBytes: Long,
    val selectedFrameIndex: Int?,
    val frameCount: Int,
)

private data class RasterPlanFrame(
    val tiles: List<RenderTile> = emptyList(),
    val chartReferenceAction: WireChartReferenceAction? = null,
    val planId: String = "none",
)

internal class RasterTileLoadGeneration {
    val bitmapCache =
        mutableStateMapOf<org.aerobag.app.domain.RenderTileKey, androidx.compose.ui.graphics.ImageBitmap?>()
    val requests = Channel<RasterTileLoadRequest>(Channel.CONFLATED)
    private val nextRequestId = AtomicLong(1L)
    private val latestRequestId = AtomicLong(0L)

    fun beginRequest(): Long = nextRequestId.getAndIncrement().also(latestRequestId::set)

    fun isCurrentRequest(requestId: Long): Boolean = latestRequestId.get() == requestId

    fun close() {
        requests.close()
    }
}

private fun rasterSemanticToken(value: String): String =
    value.replace("%", "%25").replace(":", "%3A").replace(",", "%2C")

private fun rasterPlanId(mapId: String, viewport: MapViewportState): String =
    buildString {
        append(rasterSemanticToken(mapId))
        append('_')
        append(java.lang.Long.toHexString(viewport.centerWorldX.toBits()))
        append('_')
        append(java.lang.Long.toHexString(viewport.centerWorldY.toBits()))
        append('_')
        append(java.lang.Long.toHexString(viewport.zoom.toBits()))
        append('_')
        append(java.lang.Long.toHexString(viewport.rotationDeg.toBits()))
    }

private const val TerrainTileBitmapCacheMaxEntries = 256
private const val NexradViewportRefreshThrottleMs = 1_000L
private const val NexradRenderFailureRetryMs = 1_000L
private const val PerfScenarioKorsOwnshipSourceId = "perf:kors-terrain-ownship"
private const val PerfScenarioKorsStressCenterLat = 48.6760
private const val PerfScenarioKorsStressCenterLon = -122.8600
private const val PerfScenarioKorsStressZoom = 10.8
private const val PerfScenarioKorsStressAltitudeMslFt = 1_000.0

internal class NexradRenderDeadlineState(
    private val failureRetryMs: Long = NexradRenderFailureRetryMs,
) {
    var deadlineElapsedRealtimeMs: Long? = null
        private set

    fun consumeWake() {
        deadlineElapsedRealtimeMs = null
    }

    fun renderCompleted(nowElapsedRealtimeMs: Long, coreDelayMs: Long?) {
        deadlineElapsedRealtimeMs = coreDelayMs?.coerceAtLeast(0L)?.let(nowElapsedRealtimeMs::plus)
    }

    fun renderFailed(nowElapsedRealtimeMs: Long) {
        deadlineElapsedRealtimeMs = nowElapsedRealtimeMs + failureRetryMs
    }
}

internal fun buildMapFollowProbeTag(
    following: Boolean,
    ownshipPosition: LatLonPoint,
    viewport: MapViewportState,
    surfaceWidthPx: Float,
    surfaceHeightPx: Float,
): String {
    val point = latLonToScreen(
        ownshipPosition.lat,
        ownshipPosition.lon,
        viewport,
        surfaceWidthPx,
        surfaceHeightPx,
    )
    return "parity:map-follow-state:" +
        "following:${if (following) 1 else 0}:" +
        "ownship-x:${point.x.roundToInt()}:" +
        "ownship-y:${point.y.roundToInt()}:" +
        "center-x:${(surfaceWidthPx / 2f).roundToInt()}:" +
        "center-y:${(surfaceHeightPx / 2f).roundToInt()}:" +
        "zoom-centi:${(viewport.zoom * 100.0).roundToInt()}"
}

internal fun buildMapSelectionCenterProbeTag(
    targetLabel: String,
    targetPosition: LatLonPoint,
    viewport: MapViewportState,
    surfaceWidthPx: Float,
    surfaceHeightPx: Float,
): String {
    val point = latLonToScreen(
        targetPosition.lat,
        targetPosition.lon,
        viewport,
        surfaceWidthPx,
        surfaceHeightPx,
    )
    val offsetPx = hypot(
        point.x - surfaceWidthPx / 2f,
        point.y - surfaceHeightPx / 2f,
    ).roundToInt()
    val tagLabel = targetLabel.filter { character ->
        character.isLetterOrDigit() || character == '-' || character == '_' || character == '.'
    }
    return "parity:map-selection-center:$tagLabel:offset-px:$offsetPx"
}

internal fun buildViewportProjectionState(
    viewport: MapViewportState,
    mapUpDeg: Double,
): String =
    "center-x-milli:${(viewport.centerWorldX * 1_000.0).roundToInt()}:" +
        "center-y-milli:${(viewport.centerWorldY * 1_000.0).roundToInt()}:" +
        "zoom:${(viewport.zoom * 1_000.0).roundToInt()}:" +
        "up:${mapUpDeg.roundToInt()}"

internal fun buildMapSelectionProjectionState(
    selectedLabel: String?,
    selectedCategoryId: String?,
    selectedText: String?,
    centerProbeTag: String?,
    detailId: String? = null,
): String {
    val centerState = centerProbeTag
        ?.removePrefix("parity:map-selection-center:")
        ?.takeIf { it != centerProbeTag }
        ?: "none:offset-px:none"
    return "selected:${selectedLabel?.let(::rasterSemanticToken) ?: "none"}:" +
        "category:${selectedCategoryId?.let(::rasterSemanticToken) ?: "none"}:" +
        "text:${selectedText?.let(::rasterSemanticToken).orEmpty()}:" +
        "centered:$centerState:" +
        "detail:${detailId?.let(::rasterSemanticToken) ?: "none"}"
}

internal fun mapSelectionDetailProjectionId(detail: MapSelectionDetailModalState?): String? =
    when {
        detail?.airportInfo != null -> "airport-info-modal:${detail.airportInfo.airportId}"
        detail?.weatherDetail != null -> "weather-detail-modal"
        detail != null -> "map-selection-detail-modal:${detail.title}"
        else -> null
    }

internal fun mapSelectionHeaderDetailText(selectedItem: MapSelectionItem?): String =
    selectedItem?.let { item ->
        listOfNotNull(
            item.description?.takeIf { it.isNotBlank() },
            item.distance?.takeIf { it.isNotBlank() },
        ).joinToString(" · ")
    }.orEmpty()

internal fun mapSelectionHeaderPrimaryText(selectedItem: MapSelectionItem?): String =
    selectedItem?.let { item ->
        listOf(item.label, mapSelectionHeaderDetailText(item))
            .filter { it.isNotEmpty() }
            .joinToString(" · ")
    } ?: " "

internal fun mapSelectionHeaderText(selectedItem: MapSelectionItem?): String =
    listOf(
        mapSelectionHeaderPrimaryText(selectedItem).trim(),
        selectedItem?.secondaryDescription?.trim().orEmpty(),
    ).filter { it.isNotEmpty() }.joinToString(" ")

internal fun viewportOwnedByCenteredInspection(
    requestedViewport: MapViewportState,
    centeredInspectionViewport: MapViewportState?,
): MapViewportState = centeredInspectionViewport ?: requestedViewport

private fun fetchMapOverlayCoreResource(
    context: Context,
    resource: CoreResourceRequest,
    devServerBaseUrl: String,
): ByteArray = fetchCoreResource(context, resource, devServerBaseUrl)

private fun fetchNexradCoreResource(
    context: Context,
    resource: CoreResourceRequest,
    devServerBaseUrl: String,
): ByteArray = fetchCoreResource(context, resource, devServerBaseUrl)

private fun fetchTerrainCoreResource(
    context: Context,
    resource: CoreResourceRequest,
    devServerBaseUrl: String,
): ByteArray = fetchCoreResource(context, resource, devServerBaseUrl)

private fun estimatedImageBitmapBytes(bitmap: androidx.compose.ui.graphics.ImageBitmap): Long =
    bitmap.width.toLong() * bitmap.height.toLong() * 4L

private fun terrainBitmapCacheStats(
    cache: LinkedHashMap<String, androidx.compose.ui.graphics.ImageBitmap>,
): Pair<Int, Long> = cache.size to cache.values.sumOf(::estimatedImageBitmapBytes)

private fun nexradFrameStats(frame: NexradOverlayFrame?): Pair<Int, Long> {
    return (frame?.decodedImageCount ?: 0) to (frame?.decodedBytes ?: 0L)
}

internal suspend fun prefetchNexradCacheResourcesBestEffort(
    resources: List<NexradOverlayCacheResource>,
    fetch: suspend (NexradOverlayCacheResource) -> Unit,
    reportFailure: (NexradOverlayCacheResource, Throwable) -> Unit,
) {
    resources.distinctBy { it.src }.forEach { resource ->
        try {
            fetch(resource)
        } catch (error: CancellationException) {
            throw error
        } catch (error: Throwable) {
            reportFailure(resource, error)
        }
    }
}

private const val TerrainWarningStatusId = "terrain:warning_unavailable"
private const val TerrainNoPositionWarningDetail = "ownship position is unavailable"

private data class TerrainOverlayDiagnostics(
    val updatedAtMs: Long = 0L,
    val status: String = "not-queried",
    val frameKey: String? = null,
    val requestCount: Int = 0,
    val cachedCount: Int = 0,
    val inFlightCount: Int = 0,
    val missingCount: Int = 0,
    val workBatchCount: Int = 0,
    val altitudeBucketFt: Double? = null,
    val viewportZoom: Double? = null,
    val viewportCenterWorldX: Double? = null,
    val viewportCenterWorldY: Double? = null,
    val surfaceWidthPx: Int = 0,
    val surfaceHeightPx: Int = 0,
    val error: String? = null,
)

private fun TerrainOverlayQueryResult.toDiagnostics(
    updatedAtMs: Long,
    viewport: MapViewportState,
    surfaceSize: IntSize,
    error: String? = null,
) = TerrainOverlayDiagnostics(
    updatedAtMs = updatedAtMs,
    status = status.toString(),
    frameKey = frameKey,
    requestCount = tileRequests.size,
    cachedCount = schedule.cachedCount,
    inFlightCount = schedule.inFlightCount,
    missingCount = schedule.missingCount,
    workBatchCount = schedule.workBatch.size,
    altitudeBucketFt = altitudeBucketFt,
    viewportZoom = viewport.zoom,
    viewportCenterWorldX = viewport.centerWorldX,
    viewportCenterWorldY = viewport.centerWorldY,
    surfaceWidthPx = surfaceSize.width,
    surfaceHeightPx = surfaceSize.height,
    error = error,
)

private fun rasterLocalBitmapCacheStats(
    cache: Map<org.aerobag.app.domain.RenderTileKey, androidx.compose.ui.graphics.ImageBitmap?>,
): Pair<Int, Long> {
    val bitmaps = cache.values.filterNotNull()
    return bitmaps.size to bitmaps.sumOf(::estimatedImageBitmapBytes)
}

private fun terrainOverlayImageForRequest(
    request: TerrainOverlayTileRequest,
    bitmap: androidx.compose.ui.graphics.ImageBitmap,
) = TerrainOverlayImage(
    key = request.key,
    z = request.z,
    x = request.x,
    yTms = request.yTms,
    left = request.left,
    top = request.top,
    size = request.size,
    bitmap = bitmap,
)

private fun terrainImagesForCompleteQuery(
    cache: LinkedHashMap<String, androidx.compose.ui.graphics.ImageBitmap>,
    query: TerrainOverlayQueryResult,
): List<TerrainOverlayImage>? {
    val images = ArrayList<TerrainOverlayImage>(query.tileRequests.size)
    query.tileRequests.forEach { request ->
        val bitmap = cache[request.cacheKey] ?: return null
        images += terrainOverlayImageForRequest(request, bitmap)
    }
    return images
}

private fun cacheTerrainBitmap(
    cache: LinkedHashMap<String, androidx.compose.ui.graphics.ImageBitmap>,
    request: TerrainOverlayTileRequest,
    bitmap: androidx.compose.ui.graphics.ImageBitmap,
) {
    cache[request.cacheKey] = bitmap
    while (cache.size > TerrainTileBitmapCacheMaxEntries) {
        val firstKey = cache.entries.iterator().next().key
        cache.remove(firstKey)
    }
}

private fun WireRasterTileSource.toRenderTileSource(): RenderTileSource? {
    // TASK-25 raster exception: installed raster tiles keep a tile-specific
    // package/member path so Android can decode directly from the package zip.
    // Do not use this as a pattern for new resources; those should use core
    // CoreResourceRequest fetching through fetchCoreResource.
    val storageKind = when (storage_kind) {
        "sectional_package" -> TileStorageKind.SectionalPackage
        "static_product" -> TileStorageKind.StaticProduct
        "asset_tree" -> TileStorageKind.AssetTree
        else -> return null
    }
    val path = when (storageKind) {
        TileStorageKind.SectionalPackage,
        TileStorageKind.StaticProduct -> {
            if (resource.kind != "installed_package") return null
            if (resource.package_name != package_name) return null
            resource.member_path?.takeIf { it.isNotBlank() } ?: return null
        }
        TileStorageKind.AssetTree -> return null
    }
    return RenderTileSource(
        mapViewId = map_view_id,
        packageName = package_name,
        storageKind = storageKind,
        path = path,
    )
}

internal data class RasterTileLoadRequest(
    val id: Long,
    val mapId: String,
    val zoom: Double,
    val centerLat: Double,
    val centerLon: Double,
    val visibleTiles: List<org.aerobag.app.domain.RenderTile>,
    val missingTiles: List<org.aerobag.app.domain.RenderTile>,
    val pageTilePaintTiming: PageTilePaintTiming?,
)

@Composable
private fun rememberRasterTileBitmapCache(
    context: Context,
    selectedMapId: String,
    fastTiles: Boolean,
    navDataEpoch: Long,
    tiles: List<RenderTile>,
    currentViewport: MapViewportState,
    decodedTileBitmapCache: DecodedTileBitmapCache,
    startupPerfTrace: AndroidStartupPerfTrace?,
    pageTilePaintTiming: PageTilePaintTiming?,
    onRasterContentReady: () -> Unit,
    onPageTilePaintTimingComplete: (Long) -> Unit,
): Map<org.aerobag.app.domain.RenderTileKey, androidx.compose.ui.graphics.ImageBitmap?> {
    val loadGeneration = remember(selectedMapId, fastTiles, navDataEpoch) {
        RasterTileLoadGeneration()
    }
    val tileBitmapCache = loadGeneration.bitmapCache
    val visibleTileKeys = remember(tiles) {
        tiles.mapTo(LinkedHashSet()) { tile -> renderTileKey(tile) }
    }
    val latestVisibleTileKeys = rememberUpdatedState(visibleTileKeys)
    val latestNavDataEpoch = rememberUpdatedState(navDataEpoch)
    val latestOnRasterContentReady = rememberUpdatedState(onRasterContentReady)
    val latestOnPageTilePaintTimingComplete = rememberUpdatedState(onPageTilePaintTimingComplete)
    val loaderScope = rememberCoroutineScope()
    val loader = remember(context.applicationContext, loaderScope, navDataEpoch) {
        RasterTileBitmapLoader(context.applicationContext, loaderScope)
    }
    val loadRequests = loadGeneration.requests
    DisposableEffect(loader) {
        onDispose(loader::close)
    }
    DisposableEffect(loadGeneration) {
        onDispose(loadGeneration::close)
    }
    LaunchedEffect(tiles, selectedMapId, fastTiles, navDataEpoch, loadRequests) {
        val staleLocalKeys = tileBitmapCache.keys.filter { key -> key !in visibleTileKeys }
        staleLocalKeys.forEach { key -> tileBitmapCache.remove(key) }
        var decodedCacheHits = 0
        tiles.forEach { tile ->
            val renderKey = renderTileKey(tile)
            if (!tileBitmapCache.containsKey(renderKey)) {
                val bitmap = decodedTileBitmapCache.get(decodedTileCacheKey(tile, navDataEpoch))
                if (bitmap != null) {
                    tileBitmapCache[renderKey] = bitmap
                    decodedCacheHits += 1
                }
            }
        }
        if (tiles.isNotEmpty() && tileBitmapCache.values.any { bitmap -> bitmap != null }) {
            latestOnRasterContentReady.value()
        }
        val missingTiles = tiles.filter { tile -> !tileBitmapCache.containsKey(renderTileKey(tile)) }
        perfLogInfo(TileBudgetLogTag) {
            val decodedCacheStats = decodedTileBitmapCache.stats()
            val (localBitmapEntries, localBitmapBytes) = rasterLocalBitmapCacheStats(tileBitmapCache)
            "visible map=$selectedMapId total=${tiles.size} missing=${missingTiles.size} localCache=${tileBitmapCache.size}/${localBitmapBytes}B localBitmaps=$localBitmapEntries pruned=${staleLocalKeys.size} decodedLru=${decodedCacheStats.entries}/${decodedCacheStats.bytes}B lruHits=$decodedCacheHits fastTiles=$fastTiles groups=[${formatTileBudgetSummary(tiles)}]"
        }
        if (missingTiles.isEmpty()) {
            if (tiles.isNotEmpty()) {
                startupPerfTrace?.mark(
                    "raster_cache_ready",
                    detail = "tiles=${tiles.size}",
                )
            }
            pageTilePaintTiming?.takeIf { tiles.isNotEmpty() }?.let { timing ->
                withFrameNanos { }
                perfLogInfo(TileBudgetLogTag) {
                    "tile-paint-frame id=${timing.id} trigger=${timing.trigger} from=${timing.fromPage} elapsedMs=${SystemClock.elapsedRealtime() - timing.startedMs} cacheOnly=true"
                }
                latestOnPageTilePaintTimingComplete.value(timing.id)
            }
            return@LaunchedEffect
        }
        val (viewportLat, viewportLon) = viewportCenterLatLon(currentViewport)
        val requestId = loadGeneration.beginRequest()
        val request = RasterTileLoadRequest(
            id = requestId,
            mapId = selectedMapId,
            zoom = currentViewport.zoom,
            centerLat = viewportLat,
            centerLon = viewportLon,
            visibleTiles = tiles,
            missingTiles = missingTiles,
            pageTilePaintTiming = pageTilePaintTiming,
        )
        perfLogInfo(TileBudgetLogTag) {
            "tile-load-request request=$requestId map=$selectedMapId zoom=${"%.2f".format(currentViewport.zoom)} center=${"%.3f".format(viewportLat)},${"%.3f".format(viewportLon)} total=${tiles.size} missing=${missingTiles.size} cache=${tileBitmapCache.size}"
        }
        if (loadRequests.trySend(request).isFailure) {
            Log.w(TileBudgetLogTag, "tile-load-request-drop request=$requestId map=$selectedMapId")
        }
    }
    LaunchedEffect(loader, loadRequests, tileBitmapCache, navDataEpoch) {
        val loadEpoch = navDataEpoch
        for (initialRequest in loadRequests) {
            var request = initialRequest
            while (true) {
                val loadStartMs = SystemClock.elapsedRealtime()
                startupPerfTrace?.mark(
                    "raster_load_started",
                    detail = "tiles=${request.missingTiles.size}",
                )
                val generationId = TileLoadGenerationIds.incrementAndGet()
                perfLogInfo(TileBudgetLogTag) {
                    "generation-start gen=$generationId request=${request.id} map=${request.mapId} zoom=${"%.2f".format(request.zoom)} center=${"%.3f".format(request.centerLat)},${"%.3f".format(request.centerLon)} total=${request.visibleTiles.size} missing=${request.missingTiles.size} cache=${tileBitmapCache.size}"
                }
                var loadedThisPassCount = 0
                var ignoredEpochCount = 0
                var ignoredRequestCount = 0
                var ignoredVisibilityCount = 0
                val loadedTiles = try {
                    loader.loadVisibleTileBitmaps(
                        request.mapId,
                        generationId,
                        request.missingTiles,
                    ) { loaded ->
                        if (loadEpoch != latestNavDataEpoch.value) {
                            ignoredEpochCount += 1
                            return@loadVisibleTileBitmaps
                        }
                        if (!loadGeneration.isCurrentRequest(request.id)) {
                            ignoredRequestCount += 1
                            return@loadVisibleTileBitmaps
                        }
                        if (loaded.result.key !in latestVisibleTileKeys.value) {
                            ignoredVisibilityCount += 1
                            return@loadVisibleTileBitmaps
                        }
                        tileBitmapCache[loaded.result.key] = loaded.result.bitmap
                        val bitmap = loaded.result.bitmap
                        if (bitmap != null) {
                            loadedThisPassCount += 1
                            decodedTileBitmapCache.put(decodedTileCacheKey(loaded.tile, loadEpoch), bitmap, loaded.result.decodedBytes)
                            startupPerfTrace?.mark(
                                "raster_first_tile_loaded",
                                loadStartMs,
                                "readMs=${loaded.result.readMs} decodeMs=${loaded.result.decodeMs}",
                            )
                            latestOnRasterContentReady.value()
                        } else {
                            Log.w(
                                TileBudgetLogTag,
                                "generation-empty gen=$generationId request=${request.id} key=${loaded.result.key} ${formatTileRef(loaded.tile)}",
                            )
                        }
                    }
                } catch (error: CancellationException) {
                    perfLogInfo(TileBudgetLogTag) {
                        "generation-cancel gen=$generationId request=${request.id} map=${request.mapId} loaded=$loadedThisPassCount/${request.missingTiles.size} elapsedMs=${SystemClock.elapsedRealtime() - loadStartMs}"
                    }
                    throw error
                }
                if (request.visibleTiles.isNotEmpty() && tileBitmapCache.values.any { bitmap -> bitmap != null }) {
                    val tileResults = loadedTiles.map { it.result }
                    startupPerfTrace?.mark(
                        "raster_tiles_loaded",
                        loadStartMs,
                        "loaded=$loadedThisPassCount requested=${request.missingTiles.size} " +
                            "readMs=${tileResults.sumOf { it.readMs }} " +
                            "decodeMs=${tileResults.sumOf { it.decodeMs }}",
                    )
                }
                val staleRequest = !loadGeneration.isCurrentRequest(request.id)
                val missingAfterLoad = request.visibleTiles.filter { tile ->
                    !tileBitmapCache.containsKey(renderTileKey(tile))
                }
                val ignoredThisPassCount =
                    ignoredEpochCount + ignoredRequestCount + ignoredVisibilityCount
                if (
                    !staleRequest &&
                    (loadedTiles.size != request.missingTiles.size || ignoredThisPassCount > 0 || missingAfterLoad.isNotEmpty())
                ) {
                    Log.w(
                        TileBudgetLogTag,
                        "generation-incomplete gen=$generationId request=${request.id} map=${request.mapId} " +
                            "results=${loadedTiles.size}/${request.missingTiles.size} loaded=$loadedThisPassCount " +
                            "ignored=$ignoredThisPassCount(epoch=$ignoredEpochCount,request=$ignoredRequestCount,visibility=$ignoredVisibilityCount) " +
                            "cacheMissing=${missingAfterLoad.size} " +
                            "tiles=[${missingAfterLoad.joinToString(", ") { tile -> formatTileRef(tile) }}]",
                    )
                }
                if (VerbosePerfLogs) {
                    val tileResults = loadedTiles.map { it.result }
                    val readElapsedMs = tileResults.sumOf { it.readMs }
                    val decodeElapsedMs = tileResults.sumOf { it.decodeMs }
                    val loadedBytes = tileResults.sumOf { it.bytes.toLong() }
                    val loadedDecodedBytes = tileResults.sumOf { it.decodedBytes }
                    perfLogInfo(TileBudgetLogTag) {
                        "generation-finish gen=$generationId request=${request.id} map=${request.mapId} stale=$staleRequest loaded=$loadedThisPassCount/${request.missingTiles.size} bytes=$loadedBytes decodedBytes=$loadedDecodedBytes elapsedMs=${SystemClock.elapsedRealtime() - loadStartMs} readMs=$readElapsedMs decodeMs=$decodeElapsedMs"
                    }
                    perfLogInfo(TileBudgetLogTag) {
                        "batch map=${request.mapId} request=${request.id} stale=$staleRequest loaded=$loadedThisPassCount/${request.missingTiles.size} bytes=$loadedBytes decodedBytes=$loadedDecodedBytes elapsedMs=${SystemClock.elapsedRealtime() - loadStartMs}"
                    }
                }
                request.pageTilePaintTiming?.takeUnless { staleRequest }?.let { timing ->
                    perfLogInfo(TileBudgetLogTag) {
                        "tile-paint-cache id=${timing.id} trigger=${timing.trigger} from=${timing.fromPage} elapsedMs=${SystemClock.elapsedRealtime() - timing.startedMs} loadMs=${SystemClock.elapsedRealtime() - loadStartMs} loaded=$loadedThisPassCount/${request.missingTiles.size}"
                    }
                    withFrameNanos { }
                    perfLogInfo(TileBudgetLogTag) {
                        "tile-paint-frame id=${timing.id} trigger=${timing.trigger} from=${timing.fromPage} elapsedMs=${SystemClock.elapsedRealtime() - timing.startedMs}"
                    }
                    latestOnPageTilePaintTimingComplete.value(timing.id)
                }
                if (VerbosePerfLogs) {
                    val tileResults = loadedTiles.map { it.result }
                    val readElapsedMs = tileResults.sumOf { it.readMs }
                    val decodeElapsedMs = tileResults.sumOf { it.decodeMs }
                    val loadedBytes = tileResults.sumOf { it.bytes.toLong() }
                    val loadedDecodedBytes = tileResults.sumOf { it.decodedBytes }
                    val loadElapsedMs = SystemClock.elapsedRealtime() - loadStartMs
                    val cacheLoadedCount = tileBitmapCache.values.count { it != null }
                    val cacheMissCount = tileBitmapCache.size - cacheLoadedCount
                    val finalDecodedCacheStats = decodedTileBitmapCache.stats()
                    val visibleTileByKey = request.visibleTiles.associateBy { renderTileKey(it) }
                    val cacheCounts = linkedMapOf<String, Int>()
                    tileBitmapCache.forEach { (key, bitmap) ->
                        val tile = visibleTileByKey[key] ?: return@forEach
                        val packageLabel = tile.sources.firstOrNull()?.packageName ?: tile.mapViewId
                        val summaryKey = "$packageLabel@z${tile.zoom}:${if (bitmap != null) "loaded" else "empty"}"
                        cacheCounts[summaryKey] = (cacheCounts[summaryKey] ?: 0) + 1
                    }
                    val cacheSummary = cacheCounts.entries
                        .sortedBy { it.key }
                        .joinToString(", ") { entry -> "${entry.key}=${entry.value}" }
                    perfLogInfo(TileBudgetLogTag) {
                        "cache map=${request.mapId} request=${request.id} stale=$staleRequest entries=${tileBitmapCache.size} loaded=$cacheLoadedCount empty=$cacheMissCount fetched=$loadedThisPassCount bytes=$loadedBytes decodedBytes=$loadedDecodedBytes loadMs=$loadElapsedMs readMs=$readElapsedMs decodeMs=$decodeElapsedMs decodedLru=${finalDecodedCacheStats.entries}/${finalDecodedCacheStats.bytes}B groups=[$cacheSummary]"
                    }
                }
                val nextRequest = loadRequests.tryReceive().getOrNull() ?: break
                perfLogInfo(TileBudgetLogTag) {
                    "tile-load-coalesce fromRequest=${request.id} toRequest=${nextRequest.id} map=${nextRequest.mapId}"
                }
                request = nextRequest
            }
        }
    }
    return tileBitmapCache
}

private data class NexradLayerState(
    val frame: NexradOverlayFrame?,
    val requestRender: () -> Unit,
)

@Composable
private fun rememberNexradLayerState(
    context: Context,
    uiSession: NativeUiSession,
    sessionWorkRunner: UiSessionWorkRunner,
    viewport: MapViewportState,
    surfaceSize: IntSize,
    surfaceWidthPx: Float,
    surfaceHeightPx: Float,
    visible: Boolean,
    enabled: Boolean,
    mapVisible: Boolean,
    liveFeedGeneration: Int,
    devServerBaseUrl: String,
): NexradLayerState {
    var frame by remember(uiSession) { mutableStateOf<NexradOverlayFrame?>(null) }
    val renderRequests = remember(uiSession) { Channel<Unit>(Channel.CONFLATED) }
    val viewportRefreshRequests = remember(uiSession) { Channel<Unit>(Channel.CONFLATED) }
    val latestViewport = rememberUpdatedState(viewport)
    val latestSurfaceSize = rememberUpdatedState(surfaceSize)
    val latestSurfaceWidthPx = rememberUpdatedState(surfaceWidthPx)
    val latestSurfaceHeightPx = rememberUpdatedState(surfaceHeightPx)
    val latestVisible = rememberUpdatedState(mapVisible && visible)
    val latestEnabled = rememberUpdatedState(enabled)
    val latestDevServerBaseUrl = rememberUpdatedState(devServerBaseUrl)
    val latestFrame = rememberUpdatedState(frame)
    DisposableEffect(renderRequests, viewportRefreshRequests) {
        onDispose {
            renderRequests.close()
            viewportRefreshRequests.close()
        }
    }
    LaunchedEffect(uiSession, renderRequests) {
        val deadlineState = NexradRenderDeadlineState()
        while (true) {
            val deadlineElapsedRealtimeMs = deadlineState.deadlineElapsedRealtimeMs
            val request = if (deadlineElapsedRealtimeMs == null) {
                renderRequests.receiveCatching()
            } else {
                val delayMs = (deadlineElapsedRealtimeMs - SystemClock.elapsedRealtime()).coerceAtLeast(0)
                if (delayMs == 0L) {
                    null
                } else {
                    withTimeoutOrNull(delayMs) { renderRequests.receiveCatching() }
                }
            }
            if (request?.isClosed == true) break
            deadlineState.consumeWake()
            val effectStartMs = SystemClock.elapsedRealtime()
            val currentSurfaceSize = latestSurfaceSize.value
            if (currentSurfaceSize.width <= 0 || currentSurfaceSize.height <= 0) {
                frame = null
                perfLogInfo(MapLayerLogTag) { "nexrad skipped reason=empty-surface elapsedMs=${SystemClock.elapsedRealtime() - effectStartMs}" }
                continue
            }
            if (!latestVisible.value || !latestEnabled.value) {
                perfLogInfo(MapLayerLogTag) { "nexrad hidden cachedImages=${frame?.images?.size ?: 0} elapsedMs=${SystemClock.elapsedRealtime() - effectStartMs}" }
                continue
            }
            val currentViewport = latestViewport.value
            val currentSurfaceWidthPx = latestSurfaceWidthPx.value
            val currentSurfaceHeightPx = latestSurfaceHeightPx.value
            val currentDevServerBaseUrl = latestDevServerBaseUrl.value
            try {
                var imageBytes = 0L
                var fetchMs = 0L
                var decodeMs = 0L
                val overlay = sessionWorkRunner.queryNexradOverlay(
                    currentViewport,
                    currentSurfaceSize.width.toDouble(),
                    currentSurfaceSize.height.toDouble(),
                ) { resource ->
                    val fetchStartMs = SystemClock.elapsedRealtime()
                    fetchNexradCoreResource(context, resource, currentDevServerBaseUrl).also {
                        fetchMs += SystemClock.elapsedRealtime() - fetchStartMs
                    }
                }
                withContext(Dispatchers.IO) {
                    prefetchNexradCacheResourcesBestEffort(
                        resources = overlay.cachePlan?.fetchResources.orEmpty(),
                        fetch = { planned ->
                            sessionWorkRunner.nexradTileBytes(planned.src) { resource ->
                                val fetchStartMs = SystemClock.elapsedRealtime()
                                fetchNexradCoreResource(context, resource, currentDevServerBaseUrl).also {
                                    fetchMs += SystemClock.elapsedRealtime() - fetchStartMs
                                }
                            }
                        },
                        reportFailure = { planned, error ->
                            Log.w(
                                "AerobagLayers",
                                "NEXRAD background prefetch failed for ${planned.src}; continuing selected frame",
                                error,
                            )
                        },
                    )
                }
                if (overlay.tiles.isEmpty()) {
                    deadlineState.renderCompleted(
                        SystemClock.elapsedRealtime(),
                        overlay.animation.nextUpdateDelayMs?.toLong(),
                    )
                    frame = null
                    perfLogInfo(MapLayerLogTag) {
                        "nexrad empty status=${overlay.status} animation=${overlay.animation.phase} nextMs=${overlay.animation.nextUpdateDelayMs} elapsedMs=${SystemClock.elapsedRealtime() - effectStartMs}"
                    }
                    continue
                }
                val decodedImagesBySrc = LinkedHashMap<String, androidx.compose.ui.graphics.ImageBitmap>()
                var decodedImageBytes = 0L
                val images = withContext(Dispatchers.IO) {
                    overlay.tiles.map { tile ->
                        val bitmap = decodedImagesBySrc.getOrPut(tile.src) {
                            val bytes = sessionWorkRunner.nexradTileBytes(tile.src) { resource ->
                                val fetchStartMs = SystemClock.elapsedRealtime()
                                fetchNexradCoreResource(context, resource, currentDevServerBaseUrl).also {
                                    fetchMs += SystemClock.elapsedRealtime() - fetchStartMs
                                }
                            }
                            imageBytes += bytes.size
                            val decodeStartMs = SystemClock.elapsedRealtime()
                            val decoded = BitmapFactory.decodeByteArray(bytes, 0, bytes.size)
                                ?: error("failed to decode nexrad tile ${tile.src}")
                            decodeMs += SystemClock.elapsedRealtime() - decodeStartMs
                            decoded.asImageBitmap().also { image ->
                                decodedImageBytes += estimatedImageBitmapBytes(image)
                            }
                        }
                        NexradOverlayImage(tile = tile, bitmap = bitmap)
                    }
                }
                frame = NexradOverlayFrame(
                    images = images,
                    viewport = currentViewport,
                    surfaceWidthPx = currentSurfaceWidthPx,
                    surfaceHeightPx = currentSurfaceHeightPx,
                    decodedImageCount = decodedImagesBySrc.size,
                    decodedBytes = decodedImageBytes,
                    selectedFrameIndex = overlay.animation.selectedFrameIndex,
                    frameCount = overlay.animation.frameCount,
                )
                perfLogInfo(MapLayerLogTag) {
                    "nexrad frame-ready pieces=${images.size} decodedImages=${decodedImagesBySrc.size} res=${overlay.stats.res} animation=${overlay.animation.phase} frame=${overlay.animation.selectedFrameIndex}/${overlay.animation.frameCount} nextMs=${overlay.animation.nextUpdateDelayMs} imageBytes=$imageBytes decodedBytes=$decodedImageBytes fetchMs=$fetchMs decodeMs=$decodeMs elapsedMs=${SystemClock.elapsedRealtime() - effectStartMs}"
                }
                deadlineState.renderCompleted(
                    SystemClock.elapsedRealtime(),
                    overlay.animation.nextUpdateDelayMs?.toLong(),
                )
            } catch (error: CancellationException) {
                throw error
            } catch (error: Throwable) {
                Log.w("AerobagLayers", "nexrad unavailable; retaining previous frame", error)
                deadlineState.renderFailed(SystemClock.elapsedRealtime())
            }
        }
    }
    LaunchedEffect(uiSession, viewportRefreshRequests) {
        for (ignored in viewportRefreshRequests) {
            delay(NexradViewportRefreshThrottleMs)
            val currentSurfaceSize = latestSurfaceSize.value
            val currentFrame = latestFrame.value
            if (
                currentSurfaceSize.width > 0 &&
                currentSurfaceSize.height > 0 &&
                latestVisible.value &&
                latestEnabled.value &&
                currentFrame != null &&
                currentFrame.images.isNotEmpty()
            ) {
                renderRequests.trySend(Unit)
            }
        }
    }
    LaunchedEffect(uiSession, viewport, mapVisible, visible, enabled) {
        if (mapVisible && visible && enabled && latestFrame.value?.images?.isNotEmpty() == true) {
            viewportRefreshRequests.trySend(Unit)
        }
    }
    LaunchedEffect(uiSession, liveFeedGeneration, surfaceSize, visible, enabled, mapVisible, devServerBaseUrl) {
        if (surfaceSize.width <= 0 || surfaceSize.height <= 0) {
            frame = null
            return@LaunchedEffect
        }
        if (mapVisible && visible && enabled) {
            renderRequests.trySend(Unit)
        }
    }
    return NexradLayerState(
        frame = frame,
        requestRender = { renderRequests.trySend(Unit) },
    )
}

private data class TerrainLayerState(
    val images: List<TerrainOverlayImage>,
    val bitmapCache: LinkedHashMap<String, androidx.compose.ui.graphics.ImageBitmap>,
    val requestRender: () -> Unit,
)

@Composable
private fun rememberTerrainLayerState(
    context: Context,
    uiSession: NativeUiSession,
    sessionWorkRunner: UiSessionWorkRunner,
    viewport: MapViewportState,
    surfaceSize: IntSize,
    surfaceWidthPx: Float,
    surfaceHeightPx: Float,
    visible: Boolean,
    mapVisible: Boolean,
    devServerBaseUrl: String,
    ownshipAltitudeBucketFt: Double?,
    ownshipPosition: LatLonPoint?,
    dataStatusState: UiDataStatusState,
    ownshipLauncherLabel: String,
): TerrainLayerState {
    var images by remember(uiSession) { mutableStateOf<List<TerrainOverlayImage>>(emptyList()) }
    var overlayError by remember(uiSession) { mutableStateOf<String?>(null) }
    var lastQueryDiagnostics by remember(uiSession) { mutableStateOf(TerrainOverlayDiagnostics()) }
    var noPaintStartedMs by remember(uiSession) { mutableLongStateOf(0L) }
    var noPaintLastWarningMs by remember(uiSession) { mutableLongStateOf(0L) }
    var staleNoPositionWarningLastMs by remember(uiSession) { mutableLongStateOf(0L) }
    val bitmapCache = remember(uiSession) {
        LinkedHashMap<String, androidx.compose.ui.graphics.ImageBitmap>(
            TerrainTileBitmapCacheMaxEntries,
            0.75f,
            true,
        )
    }
    val inFlightKeys = remember(uiSession) { mutableSetOf<String>() }
    val renderRequests = remember(uiSession) { Channel<Unit>(Channel.CONFLATED) }
    val latestViewport = rememberUpdatedState(viewport)
    val latestSurfaceSize = rememberUpdatedState(surfaceSize)
    val latestSurfaceWidthPx = rememberUpdatedState(surfaceWidthPx)
    val latestSurfaceHeightPx = rememberUpdatedState(surfaceHeightPx)
    val latestMapVisible = rememberUpdatedState(mapVisible)
    val latestVisible = rememberUpdatedState(visible)
    val latestImageCount = rememberUpdatedState(images.size)
    val latestDiagnostics = rememberUpdatedState(lastQueryDiagnostics)
    val latestOwnshipAltitudeBucketFt = rememberUpdatedState(ownshipAltitudeBucketFt)
    val latestOwnshipPosition = rememberUpdatedState(ownshipPosition)
    val latestDataStatus = rememberUpdatedState(dataStatusState)
    val latestOwnshipLauncherLabel = rememberUpdatedState(ownshipLauncherLabel)
    DisposableEffect(renderRequests) {
        onDispose(renderRequests::close)
    }
    LaunchedEffect(uiSession) {
        while (true) {
            delay(10_000)
            val nowMs = SystemClock.elapsedRealtime()
            val currentSurfaceSize = latestSurfaceSize.value
            val altitudeBucketFt = latestOwnshipAltitudeBucketFt.value
            val position = latestOwnshipPosition.value
            val diagnostics = latestDiagnostics.value
            val noPositionTerrainWarning = latestDataStatus.value.boxes.firstOrNull { box ->
                box.id == TerrainWarningStatusId && box.detail.contains(TerrainNoPositionWarningDetail)
            }
            if (position != null && altitudeBucketFt != null && noPositionTerrainWarning != null) {
                if (staleNoPositionWarningLastMs == 0L || nowMs - staleNoPositionWarningLastMs >= 60_000L) {
                    staleNoPositionWarningLastMs = nowMs
                    Log.w(
                        MapLayerLogTag,
                        "terrain stale-no-position-warning-with-ownship " +
                            "ownship=${position.lat},${position.lon} " +
                            "ownshipAltitudeBucketFt=$altitudeBucketFt " +
                            "ownshipLauncher=${latestOwnshipLauncherLabel.value} " +
                            "warningDetail=${noPositionTerrainWarning.detail} " +
                            "lastQueryAgeMs=${if (diagnostics.updatedAtMs > 0L) nowMs - diagnostics.updatedAtMs else null} " +
                            "status=${diagnostics.status} frame=${diagnostics.frameKey} " +
                            "requests=${diagnostics.requestCount} cached=${diagnostics.cachedCount} " +
                            "inFlight=${diagnostics.inFlightCount} missing=${diagnostics.missingCount} " +
                            "workBatch=${diagnostics.workBatchCount} queryAltitudeBucketFt=${diagnostics.altitudeBucketFt} " +
                            "surface=${diagnostics.surfaceWidthPx}x${diagnostics.surfaceHeightPx} " +
                            "zoom=${diagnostics.viewportZoom} " +
                            "centerWorld=${diagnostics.viewportCenterWorldX},${diagnostics.viewportCenterWorldY} " +
                            "error=${diagnostics.error}",
                    )
                }
            } else {
                staleNoPositionWarningLastMs = 0L
            }
            val shouldHaveTerrain =
                latestMapVisible.value &&
                    latestVisible.value &&
                    currentSurfaceSize.width > 0 &&
                    currentSurfaceSize.height > 0 &&
                    position != null &&
                    altitudeBucketFt != null
            if (!shouldHaveTerrain || latestImageCount.value > 0) {
                noPaintStartedMs = 0L
                noPaintLastWarningMs = 0L
                continue
            }
            if (noPaintStartedMs == 0L) {
                noPaintStartedMs = nowMs
                continue
            }
            val noPaintMs = nowMs - noPaintStartedMs
            if (noPaintMs < 60_000L) continue
            if (noPaintLastWarningMs != 0L && nowMs - noPaintLastWarningMs < 60_000L) continue
            noPaintLastWarningMs = nowMs
            Log.w(
                MapLayerLogTag,
                "terrain no-paint-with-altitude durationMs=$noPaintMs " +
                    "ownship=${position.lat},${position.lon} " +
                    "ownshipAltitudeBucketFt=$altitudeBucketFt " +
                    "lastQueryAgeMs=${if (diagnostics.updatedAtMs > 0L) nowMs - diagnostics.updatedAtMs else null} " +
                    "status=${diagnostics.status} frame=${diagnostics.frameKey} " +
                    "requests=${diagnostics.requestCount} cached=${diagnostics.cachedCount} " +
                    "inFlight=${diagnostics.inFlightCount} missing=${diagnostics.missingCount} " +
                    "workBatch=${diagnostics.workBatchCount} queryAltitudeBucketFt=${diagnostics.altitudeBucketFt} " +
                    "surface=${diagnostics.surfaceWidthPx}x${diagnostics.surfaceHeightPx} " +
                    "zoom=${diagnostics.viewportZoom} " +
                    "centerWorld=${diagnostics.viewportCenterWorldX},${diagnostics.viewportCenterWorldY} " +
                    "error=${diagnostics.error}",
            )
        }
    }
    LaunchedEffect(uiSession, renderRequests, devServerBaseUrl) {
        for (ignored in renderRequests) {
            while (true) {
                val effectStartMs = SystemClock.elapsedRealtime()
                val currentSurfaceSize = latestSurfaceSize.value
                if (!latestMapVisible.value || currentSurfaceSize.width <= 0 || currentSurfaceSize.height <= 0) {
                    images = emptyList()
                    overlayError = null
                    perfLogInfo(MapLayerLogTag) { "terrain skipped reason=empty-surface elapsedMs=${SystemClock.elapsedRealtime() - effectStartMs}" }
                    break
                }
                if (!latestVisible.value) {
                    images = emptyList()
                    overlayError = null
                    perfLogInfo(MapLayerLogTag) { "terrain disabled elapsedMs=${SystemClock.elapsedRealtime() - effectStartMs}" }
                    break
                }
                val currentViewport = latestViewport.value
                val currentSurfaceWidthPx = latestSurfaceWidthPx.value
                val currentSurfaceHeightPx = latestSurfaceHeightPx.value
                val query = try {
                    sessionWorkRunner.queryTerrainOverlay(
                        currentViewport,
                        currentSurfaceWidthPx.toDouble(),
                        currentSurfaceHeightPx.toDouble(),
                        bitmapCache.keys.toList(),
                        inFlightKeys.toList(),
                    ) { resource ->
                        fetchTerrainCoreResource(context, resource, devServerBaseUrl)
                    }
                } catch (error: Throwable) {
                    images = emptyList()
                    overlayError = error.message ?: error::class.java.simpleName
                    lastQueryDiagnostics = TerrainOverlayDiagnostics(
                        updatedAtMs = SystemClock.elapsedRealtime(),
                        status = "query-error",
                        viewportZoom = currentViewport.zoom,
                        viewportCenterWorldX = currentViewport.centerWorldX,
                        viewportCenterWorldY = currentViewport.centerWorldY,
                        surfaceWidthPx = currentSurfaceSize.width,
                        surfaceHeightPx = currentSurfaceSize.height,
                        error = overlayError,
                    )
                    Log.w("AerobagLayers", "terrain overlay unavailable", error)
                    break
                }
                val queryMs = SystemClock.elapsedRealtime() - effectStartMs
                lastQueryDiagnostics = query.toDiagnostics(
                    updatedAtMs = SystemClock.elapsedRealtime(),
                    viewport = currentViewport,
                    surfaceSize = currentSurfaceSize,
                )
                if (query.status !is org.aerobag.app.domain.TerrainOverlayStatus.Ready) {
                    images = emptyList()
                    overlayError = null
                    perfLogInfo(MapLayerLogTag) {
                        "terrain not-ready status=${query.status::class.java.simpleName} queryMs=$queryMs elapsedMs=${SystemClock.elapsedRealtime() - effectStartMs}"
                    }
                    break
                }
                val frameKey = query.frameKey
                val altitudeBucketFt = query.altitudeBucketFt
                if (frameKey == null || altitudeBucketFt == null) {
                    images = emptyList()
                    overlayError = null
                    perfLogInfo(MapLayerLogTag) {
                        "terrain not-ready status=missing-frame queryMs=$queryMs elapsedMs=${SystemClock.elapsedRealtime() - effectStartMs}"
                    }
                    break
                }
                if (query.schedule.frameComplete) {
                    val completeImages = terrainImagesForCompleteQuery(bitmapCache, query)
                    if (completeImages != null) {
                        images = completeImages
                        overlayError = null
                        if (completeImages.isNotEmpty()) {
                            noPaintStartedMs = 0L
                            noPaintLastWarningMs = 0L
                        }
                    }
                    perfLogInfo(MapLayerLogTag) {
                        "terrain frame-ready frame=$frameKey requests=${query.tileRequests.size} images=${completeImages?.size ?: 0} cached=${query.schedule.cachedCount} queryMs=$queryMs elapsedMs=${SystemClock.elapsedRealtime() - effectStartMs}"
                    }
                    break
                }
                val workBatch = query.schedule.workBatch
                if (workBatch.isEmpty()) {
                    perfLogInfo(MapLayerLogTag) {
                        "terrain waiting frame=$frameKey requests=${query.tileRequests.size} cached=${query.schedule.cachedCount} inFlight=${query.schedule.inFlightCount} missing=${query.schedule.missingCount} queryMs=$queryMs elapsedMs=${SystemClock.elapsedRealtime() - effectStartMs}"
                    }
                    break
                }
                val batchStartMs = SystemClock.elapsedRealtime()
                var batchRendered = 0
                var batchFetchMs = 0L
                var batchRenderMs = 0L
                var batchParseMs = 0L
                var batchRawBytesTotal = 0L
                var batchFailed = false
                for (request in workBatch) {
                    if (!latestMapVisible.value || !latestVisible.value) break
                    if (bitmapCache.containsKey(request.cacheKey) || inFlightKeys.contains(request.cacheKey)) continue
                    var fetchMs = 0L
                    var renderMs = 0L
                    var parseMs = 0L
                    var rawBytesTotal = 0L
                    inFlightKeys += request.cacheKey
                    try {
                        val renderStartMs = SystemClock.elapsedRealtime()
                        val rawBytes = sessionWorkRunner.renderTerrainOverlayTile(
                            request,
                            altitudeBucketFt,
                        ) { resource ->
                            val fetchStartMs = SystemClock.elapsedRealtime()
                            fetchTerrainCoreResource(context, resource, devServerBaseUrl).also {
                                fetchMs += SystemClock.elapsedRealtime() - fetchStartMs
                            }
                        }.also {
                            renderMs += SystemClock.elapsedRealtime() - renderStartMs
                        }
                        rawBytesTotal += rawBytes.size
                        val parseStartMs = SystemClock.elapsedRealtime()
                        val bitmap = parseTerrainRawRgba(rawBytes)
                        parseMs += SystemClock.elapsedRealtime() - parseStartMs
                        cacheTerrainBitmap(bitmapCache, request, bitmap)
                        batchRendered += 1
                        batchFetchMs += fetchMs
                        batchRenderMs += renderMs
                        batchParseMs += parseMs
                        batchRawBytesTotal += rawBytesTotal
                    } catch (error: CancellationException) {
                        throw error
                    } catch (error: Throwable) {
                        batchFailed = true
                        overlayError = error.message ?: error::class.java.simpleName
                        lastQueryDiagnostics = query.toDiagnostics(
                            updatedAtMs = SystemClock.elapsedRealtime(),
                            viewport = currentViewport,
                            surfaceSize = currentSurfaceSize,
                            error = overlayError,
                        )
                        Log.w("AerobagLayers", "terrain overlay unavailable", error)
                        break
                    } finally {
                        inFlightKeys -= request.cacheKey
                    }
                    yield()
                }
                perfLogInfo(MapLayerLogTag) {
                    "terrain batch-rendered frame=$frameKey requests=${query.tileRequests.size} rendered=$batchRendered batch=${workBatch.size} cached=${query.schedule.cachedCount} missing=${query.schedule.missingCount} rawBytes=$batchRawBytesTotal queryMs=$queryMs fetchMs=$batchFetchMs renderMs=$batchRenderMs parseMs=$batchParseMs batchMs=${SystemClock.elapsedRealtime() - batchStartMs} elapsedMs=${SystemClock.elapsedRealtime() - effectStartMs}"
                }
                if (batchFailed) break
            }
        }
    }
    LaunchedEffect(uiSession, viewport, surfaceSize, visible, mapVisible, devServerBaseUrl, ownshipAltitudeBucketFt, ownshipPosition != null) {
        val effectStartMs = SystemClock.elapsedRealtime()
        if (surfaceSize.width <= 0 || surfaceSize.height <= 0 || !mapVisible) {
            images = emptyList()
            overlayError = null
            perfLogInfo(MapLayerLogTag) { "terrain skipped reason=empty-surface elapsedMs=${SystemClock.elapsedRealtime() - effectStartMs}" }
            return@LaunchedEffect
        }
        if (!visible) {
            images = emptyList()
            overlayError = null
            perfLogInfo(MapLayerLogTag) { "terrain disabled elapsedMs=${SystemClock.elapsedRealtime() - effectStartMs}" }
            return@LaunchedEffect
        }
        renderRequests.trySend(Unit)
    }
    return TerrainLayerState(
        images = images,
        bitmapCache = bitmapCache,
        requestRender = { renderRequests.trySend(Unit) },
    )
}

private fun emptyMapOverlay() = MapOverlayQueryResult(
    visibleFeatures = emptyList(),
    visibleMetars = emptyList(),
    visiblePireps = emptyList(),
    visibleTraffic = emptyList(),
    trafficNextRefreshEpochMs = null,
    airspacePaths = emptyList(),
    tfrPaths = emptyList(),
    airspaceLabels = emptyList(),
    offlineRegions = emptyList(),
)

@Composable
private fun rememberDisplayedMapOverlay(
    context: Context,
    uiSession: NativeUiSession,
    sessionWorkRunner: UiSessionWorkRunner,
    navDataEpoch: Long,
    liveFeedGeneration: Int,
    invalidationRevision: Int,
    viewport: MapViewportState,
    displayViewport: MapViewportState,
    surfaceSize: IntSize,
    overlayWidthPx: Float,
    overlayHeightPx: Float,
    displayWidthPx: Float,
    displayHeightPx: Float,
    densityScale: Float,
    vectorsVisible: Boolean,
    metarsVisible: Boolean,
    trafficVisible: Boolean,
    offlineRegionsVisible: Boolean,
    devServerBaseUrl: String,
    startupPerfTrace: AndroidStartupPerfTrace?,
    onVectorContentReady: () -> Unit,
): MapOverlayQueryResult {
    var committedOverlay by remember(uiSession, navDataEpoch) { mutableStateOf(emptyMapOverlay()) }
    var committedViewport by remember(uiSession, navDataEpoch) { mutableStateOf<MapViewportState?>(null) }
    var committedSurface by remember(uiSession, navDataEpoch) { mutableStateOf<OverlaySurfaceUnits?>(null) }
    var trafficRefreshTick by remember(uiSession) { mutableIntStateOf(0) }
    val latestNavDataEpoch = rememberUpdatedState(navDataEpoch)
    val latestOnVectorContentReady = rememberUpdatedState(onVectorContentReady)
    LaunchedEffect(
        uiSession,
        navDataEpoch,
        liveFeedGeneration,
        invalidationRevision,
        trafficRefreshTick,
        viewport,
        surfaceSize,
        densityScale,
        vectorsVisible,
        metarsVisible,
        trafficVisible,
        offlineRegionsVisible,
        devServerBaseUrl,
    ) {
        if (surfaceSize.width <= 0 || surfaceSize.height <= 0) return@LaunchedEffect
        val overlayStartMs = SystemClock.elapsedRealtime()
        startupPerfTrace?.mark(
            "vector_query_started",
            detail = "surface=${surfaceSize.width}x${surfaceSize.height}",
        )
        val queryEpoch = navDataEpoch
        sessionWorkRunner.submitOverlay(
            viewport = viewport,
            widthPx = overlayWidthPx.toDouble(),
            heightPx = overlayHeightPx.toDouble(),
            pointDisplayScale = densityScale.toDouble(),
            fetchResource = { resource ->
                fetchMapOverlayCoreResource(context, resource, devServerBaseUrl)
            },
            onResult = { outcome ->
                if (queryEpoch != latestNavDataEpoch.value) return@submitOverlay
                val overlay = outcome.overlay
                perfLogInfo(MapLayerLogTag) {
                    val (centerLat, centerLon) = viewportCenterLatLon(viewport)
                    "overlay center=${"%.3f".format(centerLat)},${"%.3f".format(centerLon)} zoom=${"%.2f".format(viewport.zoom)} size=${surfaceSize.width}x${surfaceSize.height} vectorsVisible=$vectorsVisible metarsVisible=$metarsVisible offlineRegionsVisible=$offlineRegionsVisible features=${overlay.visibleFeatures.size} airspace=${overlay.airspacePaths.size} airspaceLabels=${overlay.airspaceLabels.size} offlineRegions=${overlay.offlineRegions.size} metars=${overlay.visibleMetars.size} pireps=${overlay.visiblePireps.size} invalidations=${outcome.invalidations} elapsedMs=${SystemClock.elapsedRealtime() - overlayStartMs}"
                }
                committedOverlay = overlay
                committedViewport = viewport
                committedSurface = OverlaySurfaceUnits(overlayWidthPx, overlayHeightPx)
                latestOnVectorContentReady.value()
                startupPerfTrace?.mark(
                    "vector_query_completed",
                    overlayStartMs,
                    "features=${overlay.visibleFeatures.size}",
                )
            },
            onError = { error ->
                Log.e(MapLayerLogTag, "overlay failed: ${error.message ?: error::class.java.simpleName}", error)
            },
        )
    }
    LaunchedEffect(trafficVisible, committedOverlay.trafficNextRefreshEpochMs) {
        val deadlineEpochMs = committedOverlay.trafficNextRefreshEpochMs
        if (!trafficVisible || deadlineEpochMs == null) return@LaunchedEffect
        delay((deadlineEpochMs - System.currentTimeMillis()).coerceAtLeast(0L))
        trafficRefreshTick += 1
    }
    return remember(
        committedOverlay,
        committedViewport,
        committedSurface,
        displayViewport,
        displayWidthPx,
        displayHeightPx,
    ) {
        transformMapOverlayForDisplay(
            overlay = committedOverlay,
            fromViewport = committedViewport,
            fromSurface = committedSurface,
            toViewport = displayViewport,
            toSurface = OverlaySurfaceUnits(displayWidthPx, displayHeightPx),
        )
    }
}

private data class MapRenderPaints(
    val situationLabelStroke: Paint,
    val situationLabelFill: Paint,
    val tileLabel: Paint,
    val tileLabelBackground: Paint,
    val fixMarkerStrokeColor: Color,
    val fixMarkerFillColor: Color,
    val airportMarkerStrokeColor: Color,
    val airportToweredFillColor: Color,
    val airportUntoweredFillColor: Color,
    val vorMarkerColor: Color,
    val vorMarkerStrokeColor: Color,
    val fixLabelStroke: Paint,
    val airportLabelStroke: Paint,
    val vorLabelFill: Paint,
    val fixLabelFill: Paint,
    val airportToweredLabelFill: Paint,
    val airportUntoweredLabelFill: Paint,
)

@Composable
private fun rememberMapRenderPaints(uiTheme: UiTheme): MapRenderPaints = remember(uiTheme) {
    fun labelPaint(color: Int, style: Paint.Style, strokeWidth: Float = 0f) = Paint().apply {
        isAntiAlias = true
        this.color = color
        this.style = style
        strokeJoin = Paint.Join.ROUND
        this.strokeWidth = strokeWidth
        textAlign = Paint.Align.CENTER
        textSize = 14f
        typeface = Typeface.create(Typeface.DEFAULT_BOLD, Typeface.BOLD)
    }
    MapRenderPaints(
        situationLabelStroke = labelPaint(
            android.graphics.Color.argb(102, 0, 0, 0),
            Paint.Style.STROKE,
            strokeWidth = 5f,
        ).apply { textSize = 16f },
        situationLabelFill = labelPaint(
            android.graphics.Color.WHITE,
            Paint.Style.FILL,
        ).apply { textSize = 16f },
        tileLabel = Paint().apply {
            isAntiAlias = true
            color = android.graphics.Color.WHITE
            textSize = 24f
            typeface = Typeface.create(Typeface.MONOSPACE, Typeface.BOLD)
        },
        tileLabelBackground = Paint().apply {
            isAntiAlias = true
            color = android.graphics.Color.argb(224, 14, 22, 28)
        },
        fixMarkerStrokeColor = Color(0xB3081218),
        fixMarkerFillColor = uiTheme.aviation.intersectionCyan,
        airportMarkerStrokeColor = Color(0xB3081218),
        airportToweredFillColor = uiTheme.aviation.classBDBlue,
        airportUntoweredFillColor = uiTheme.aviation.classCMagenta,
        vorMarkerColor = uiTheme.aviation.classBDBlue,
        vorMarkerStrokeColor = Color(0xD1081218),
        fixLabelStroke = labelPaint(
            android.graphics.Color.argb(179, 8, 18, 24),
            Paint.Style.STROKE,
            strokeWidth = 4f,
        ),
        airportLabelStroke = labelPaint(
            android.graphics.Color.argb(179, 8, 18, 24),
            Paint.Style.STROKE,
            strokeWidth = 3f,
        ),
        vorLabelFill = labelPaint(android.graphics.Color.WHITE, Paint.Style.FILL),
        fixLabelFill = labelPaint(android.graphics.Color.WHITE, Paint.Style.FILL),
        airportToweredLabelFill = labelPaint(android.graphics.Color.WHITE, Paint.Style.FILL),
        airportUntoweredLabelFill = labelPaint(android.graphics.Color.WHITE, Paint.Style.FILL),
    )
}

@Composable
private fun rememberRasterPlanFrame(
    selectedMapId: String,
    displayViewport: MapViewportState,
    surfaceSize: IntSize,
    surfaceWidthDp: Float,
    surfaceHeightDp: Float,
    mapDisplayScale: Double,
    uiSession: NativeUiSession,
    fastTiles: Boolean,
    startupPerfTrace: AndroidStartupPerfTrace?,
    pageTilePaintTiming: PageTilePaintTiming?,
): RasterPlanFrame {
    val json = remember { Json { ignoreUnknownKeys = true } }
    return remember(selectedMapId, displayViewport, surfaceSize, mapDisplayScale, uiSession, fastTiles) {
        if (surfaceSize.width == 0 || surfaceSize.height == 0) {
            RasterPlanFrame()
        } else {
            val planStartMs = SystemClock.elapsedRealtime()
            val plan = json.decodeFromString<WireRasterTilePlan>(
                uiSession.queryRasterTilePlanJson(
                    displayViewport,
                    surfaceWidthDp.toDouble(),
                    surfaceHeightDp.toDouble(),
                    mapDisplayScale,
                ),
            )
            val planMs = SystemClock.elapsedRealtime() - planStartMs
            startupPerfTrace?.mark(
                "raster_plan_ready",
                planStartMs,
                "tiles=${plan.tiles.size}",
            )
            pageTilePaintTiming?.let { timing ->
                perfLogInfo(TileBudgetLogTag) {
                    "tile-paint-plan id=${timing.id} trigger=${timing.trigger} from=${timing.fromPage} elapsedMs=${SystemClock.elapsedRealtime() - timing.startedMs} planMs=$planMs tiles=${plan.tiles.size} fastTiles=$fastTiles"
                }
            }
            RasterPlanFrame(
                tiles = plan.tiles.mapNotNull { tile ->
                    val sources = (listOf(tile.primary) + tile.fallbacks)
                        .mapNotNull { source -> source.toRenderTileSource() }
                    if (sources.isEmpty()) {
                        return@mapNotNull null
                    }
                    RenderTile(
                        x = tile.x,
                        yTms = tile.y_tms,
                        leftPx = tile.left_px.toFloat(),
                        topPx = tile.top_px.toFloat(),
                        sizePx = tile.size_px.toFloat(),
                        zoom = tile.source_zoom,
                        mapViewId = tile.primary.map_view_id,
                        sources = sources,
                    )
                },
                chartReferenceAction = plan.chart_reference_action,
                planId = rasterPlanId(selectedMapId, displayViewport),
            )
        }
    }
}

private fun mapSelectionItemById(
    result: MapSelectionQueryResult,
    itemId: String?,
): MapSelectionItem? {
    if (itemId == null) {
        return null
    }
    return result.categories
        .asSequence()
        .flatMap { it.items.asSequence() }
        .firstOrNull { it.id == itemId }
}

internal data class MapExplorerActions(
    val onPageTilePaintTimingComplete: (Long) -> Unit,
    val onViewportChange: (MapViewportState) -> Unit,
    val onViewportGestureActiveChange: (Boolean) -> Unit,
    val onViewportGestureActivity: () -> Unit,
    val onMapOrientationModeChange: (MapOrientationMode) -> Unit,
    val onSessionSnapshotChange: (UiSessionSnapshot) -> Unit,
    val onSessionCommandFailure: (Throwable) -> Unit,
    val onBeforeMapLayerCommand: () -> Unit,
    val onReloadApplication: () -> Unit,
    val onSelectOwnshipSource: (String) -> Unit,
    val onSituationControlInput: (SituationControlInput) -> Unit,
    val onPlaybackSourcePathChange: (String) -> Unit,
    val onSelectMapFamily: (String) -> Unit,
    val onOpenChartReference: (familyId: String, suggestedChartIds: List<String>) -> Unit,
    val onSelectPage: (AppPage) -> Unit,
    val onOpenPlateTarget: (airportId: String, target: String, chartId: String) -> Unit,
    val onOpenPlan: () -> Unit,
)

@Composable
private fun RunMapSelectionPerfScenario(
    perfScenario: AndroidPerfScenario?,
    uiSession: NativeUiSession,
    surfaceSize: IntSize,
    selectedMapId: String,
    surfaceWidthPx: Float,
    surfaceHeightPx: Float,
    densityScale: Double,
    selectedMapMinZoom: Double,
    interactiveMaxZoom: Double,
    sessionWorkRunner: UiSessionWorkRunner,
    context: Context,
    devServerBaseUrl: String,
    updateViewport: (MapViewportState) -> Unit,
    onMapSelection: (MapSelectionUiState) -> Unit,
) {
    var started by remember(perfScenario?.id, uiSession) { mutableStateOf(false) }
    LaunchedEffect(perfScenario?.id, uiSession, surfaceSize, selectedMapId) {
        val scenario = perfScenario ?: return@LaunchedEffect
        if (scenario.id != AndroidPerfScenarioMapSelectionFreeze || started) {
            return@LaunchedEffect
        }
        if (surfaceWidthPx <= 0f || surfaceHeightPx <= 0f) {
            return@LaunchedEffect
        }
        started = true
        val watchdog = AndroidMainThreadStallWatchdog(scenario)
        val frameGapMonitor = AndroidFrameGapMonitor(scenario)
        watchdog.start()
        frameGapMonitor.start()
        val scenarioStartMs = SystemClock.elapsedRealtime()
        try {
            suspend fun forcePerfSelection(selectionViewport: MapViewportState, stepLabel: String) {
                val clickStartedMs = SystemClock.elapsedRealtime()
                val (lat, lon) = worldToLatLon(selectionViewport.centerWorldX, selectionViewport.centerWorldY)
                Log.i(
                    AndroidPerfScenarioTag,
                    "selection_start scenario=${scenario.id} step=$stepLabel lat=${"%.5f".format(lat)} lon=${"%.5f".format(lon)}",
                )
                val result = sessionWorkRunner.queryMapSelection(
                    viewport = selectionViewport,
                    widthPx = surfaceWidthPx.toDouble(),
                    heightPx = surfaceHeightPx.toDouble(),
                    click = LatLonPoint(lat = lat, lon = lon),
                    pointDisplayScale = densityScale,
                    fetchResource = { resource ->
                        fetchMapOverlayCoreResource(context, resource, devServerBaseUrl)
                    },
                )
                val elapsedMs = SystemClock.elapsedRealtime() - clickStartedMs
                val itemCount = result.categories.sumOf { it.items.size }
                val logLine =
                    "selection_done scenario=${scenario.id} elapsedMs=$elapsedMs thresholdMs=${scenario.slowSelectionThresholdMs} categories=${result.categories.size} items=$itemCount"
                if (elapsedMs > scenario.slowSelectionThresholdMs) {
                    Log.w(AndroidPerfScenarioTag, "threshold_violation kind=slow_selection $logLine")
                } else {
                    Log.i(AndroidPerfScenarioTag, logLine)
                }
                onMapSelection(
                    MapSelectionUiState(
                        point = Offset(surfaceWidthPx / 2f, surfaceHeightPx / 2f),
                        result = result,
                        selectedItem = mapSelectionItemById(result, result.initialSelectedItemId),
                    ),
                )
            }

            Log.i(
                AndroidPerfScenarioTag,
                "start scenario=${scenario.id} surface=${surfaceSize.width}x${surfaceSize.height} density=$densityScale map=$selectedMapId",
            )
            val sfo = latLonToWorld(37.6213, -122.3790)
            val baseViewport = MapViewportState(
                centerWorldX = sfo.x,
                centerWorldY = sfo.y,
                zoom = clampZoom(9.8, selectedMapMinZoom, interactiveMaxZoom),
            )
            updateViewport(baseViewport)
            delay(750)
            val overlayCompletions = (0 until scenario.overlayFanout).map { worker ->
                val completion = CompletableDeferred<Unit>()
                val workerViewport = dragViewport(
                    baseViewport.copy(zoom = baseViewport.zoom + ((worker % 5) - 2) * 0.04),
                    (((worker % 6) - 3) * 120).toFloat(),
                    (((worker % 7) - 3) * 100).toFloat(),
                )
                val overlayStartedMs = SystemClock.elapsedRealtime()
                Log.i(
                    AndroidPerfScenarioTag,
                    "overlay_start scenario=${scenario.id} worker=$worker zoom=${"%.2f".format(workerViewport.zoom)}",
                )
                sessionWorkRunner.submitOverlay(
                    viewport = workerViewport,
                    widthPx = surfaceWidthPx.toDouble(),
                    heightPx = surfaceHeightPx.toDouble(),
                    pointDisplayScale = densityScale,
                    fetchResource = { resource ->
                        fetchMapOverlayCoreResource(context.applicationContext, resource, devServerBaseUrl)
                    },
                    onResult = { outcome ->
                        val overlay = outcome.overlay
                        Log.i(
                            AndroidPerfScenarioTag,
                            "overlay_done scenario=${scenario.id} worker=$worker elapsedMs=${SystemClock.elapsedRealtime() - overlayStartedMs} features=${overlay.visibleFeatures.size} airspace=${overlay.airspacePaths.size} labels=${overlay.airspaceLabels.size} metars=${overlay.visibleMetars.size} pireps=${overlay.visiblePireps.size}",
                        )
                        completion.complete(Unit)
                    },
                    onError = { error ->
                        Log.w(
                            AndroidPerfScenarioTag,
                            "overlay_failed scenario=${scenario.id} worker=$worker elapsedMs=${SystemClock.elapsedRealtime() - overlayStartedMs}: ${error.message}",
                            error,
                        )
                        completion.complete(Unit)
                    },
                    onDropped = { reason ->
                        Log.i(
                            AndroidPerfScenarioTag,
                            "overlay_dropped scenario=${scenario.id} worker=$worker elapsedMs=${SystemClock.elapsedRealtime() - overlayStartedMs}: $reason",
                        )
                        completion.complete(Unit)
                    },
                )
                completion
            }
            delay(75)
            forcePerfSelection(baseViewport, "after_overlay_burst")
            var lastViewport = baseViewport
            repeat(90) { step ->
                val dxPx = (((step % 12) - 6) * 42).toFloat()
                val dyPx = (((step % 10) - 5) * 38).toFloat()
                val zoom = baseViewport.zoom + ((step % 5) - 2) * 0.03
                lastViewport = dragViewport(baseViewport.copy(zoom = zoom), dxPx, dyPx)
                updateViewport(lastViewport)
                if (step == 24) {
                    forcePerfSelection(lastViewport, step.toString())
                }
                delay(35)
            }
            overlayCompletions.awaitAll()
            delay(1_500)
            Log.i(
                AndroidPerfScenarioTag,
                "done scenario=${scenario.id} elapsedMs=${SystemClock.elapsedRealtime() - scenarioStartMs}",
            )
        } catch (error: CancellationException) {
            Log.i(
                AndroidPerfScenarioTag,
                "cancelled scenario=${scenario.id} elapsedMs=${SystemClock.elapsedRealtime() - scenarioStartMs}",
            )
            throw error
        } catch (error: Throwable) {
            Log.e(
                AndroidPerfScenarioTag,
                "failed scenario=${scenario.id} elapsedMs=${SystemClock.elapsedRealtime() - scenarioStartMs}: ${error.message}",
                error,
            )
        } finally {
            frameGapMonitor.stop()
            watchdog.stop()
        }
    }
}

@OptIn(ExperimentalComposeUiApi::class)
@Composable
internal fun MapExplorerPage(
    appCore: NativeAppCoreAdapter,
    page: AppPage,
    pageHistory: List<AppViewSnapshot>,
    uptimeLabel: String,
    uiSession: NativeUiSession,
    sessionWorkRunner: UiSessionWorkRunner,
    shellSessionSnapshot: UiSessionSnapshot,
    sessionRenderModel: SessionRenderModel,
    sessionRenderDiagnostics: SessionRenderDiagnostics,
    uiInvalidationRevisions: UiInvalidationRevisions,
    liveFeedGeneration: Int,
    uiTheme: UiTheme,
    playbackSourcePath: String,
    situationRingCandidates: List<SituationRingCandidate>,
    selectedMap: RasterMapUiState,
    mapFamilyOptions: List<MapFamilyOption>,
    viewport: MapViewportState,
    mapOrientationMode: MapOrientationMode,
    mapOrientationMemory: MapOrientationMemory,
    decodedTileBitmapCache: DecodedTileBitmapCache,
    debugState: UiDebugState,
    perfScenario: AndroidPerfScenario? = null,
    startupPerfTrace: AndroidStartupPerfTrace? = null,
    pageTilePaintTiming: PageTilePaintTiming?,
    actions: MapExplorerActions,
    navElement: NavElementUiView?,
    planUiState: FlightPlanUiState?,
) {
    val uriHandler = LocalUriHandler.current
    val mapCompositionStartedAtMs = startupPerfTrace?.let { SystemClock.elapsedRealtime() }
    SideEffect(sessionRenderDiagnostics::recordMap)
    SideEffect { startupPerfTrace?.mark("map_composed") }
    val highRate by sessionRenderModel.highRateProjectionState
    val sessionSnapshot = remember(shellSessionSnapshot, highRate) {
        shellSessionSnapshot.withHighRateProjection(highRate)
    }
    val ownship = highRate.ownship.render
    val ownshipControls = highRate.ownship.controls
    val flightDataBanner = highRate.flightDataBanner
    val playbackUiState = highRate.playbackUiState
    val playbackPanelState = highRate.playbackPanelState
    val mapFollowUiState = highRate.mapFollowUiState
    val mapFollowTargetViewport = highRate.mapFollowTargetViewport
    val context = LocalContext.current
    val activity = context as? MainActivity
    val density = LocalDensity.current
    val navDataEpoch = sessionSnapshot.navDataEpoch
    LaunchedEffect(navDataEpoch) {
        decodedTileBitmapCache.clear()
    }
    val devServerBaseUrl = remember(context) { loadAndroidDevServerBaseUrl(context.applicationContext) }
    fun applySessionCommand(commandName: String, operation: () -> UiSessionSnapshot): UiSessionSnapshot? =
        try {
            operation().also(actions.onSessionSnapshotChange)
        } catch (error: CancellationException) {
            throw error
        } catch (error: Throwable) {
            Log.w("AerobagSessionCommand", "map command failed command=$commandName", error)
            actions.onSessionCommandFailure(error)
            null
        }
    val focusRequester = remember { FocusRequester() }
    var chartTrayOpen by remember { mutableStateOf(false) }
    var layerTrayOpen by remember { mutableStateOf(false) }
    var openStatusControlId by remember { mutableStateOf<UiSurfaceStatusControlId?>(null) }
    var situationTrayOpen by remember { mutableStateOf(false) }
    var chartSearchText by remember { mutableStateOf("") }
    var chartSearchOpen by remember { mutableStateOf(false) }
    var chartSearchLoading by remember { mutableStateOf(false) }
    var chartSearchError by remember { mutableStateOf<String?>(null) }
    var chartSearchSuggestions by remember { mutableStateOf<List<WaypointIdentifierSuggestion>>(emptyList()) }
    var mapSelection by remember { mutableStateOf<MapSelectionUiState?>(null) }
    val mapSelectionDistanceItemId = mapSelection?.takeIf { it.detailModal == null }?.selectedItem?.id
    val mapSelectionDistanceTarget = mapSelection?.takeIf { it.detailModal == null }?.selectedItem?.distanceTarget
    LaunchedEffect(uiSession, mapSelectionDistanceItemId, mapSelectionDistanceTarget) {
        val itemId = mapSelectionDistanceItemId ?: return@LaunchedEffect
        val target = mapSelectionDistanceTarget ?: return@LaunchedEffect
        while (true) {
            delay(1_000)
            val distance = try {
                withContext(Dispatchers.IO) {
                    uiSession.queryMapSelectionDistance(target)
                }
            } catch (error: CancellationException) {
                throw error
            } catch (error: Throwable) {
                Log.w("AerobagSelection", "map selection distance refresh failed", error)
                continue
            }
            val current = mapSelection ?: continue
            val selected = current.selectedItem ?: continue
            if (
                selected.id != itemId ||
                selected.distanceTarget != target ||
                selected.distance == distance
            ) {
                continue
            }
            val selectedItem = selected.copy(distance = distance)
            mapSelection = current.copy(
                selectedItem = selectedItem,
                result = current.result.copy(
                    categories = current.result.categories.map { category ->
                        category.copy(
                            items = category.items.map { item ->
                                if (item.id == itemId && item.distanceTarget == target) selectedItem else item
                            },
                        )
                    },
                ),
            )
        }
    }
    val chartSearchInspectionGate = remember(uiSession) { ChartSearchInspectionGate() }
    var mapSurfaceBounds by remember { mutableStateOf<Rect?>(null) }
    var mapSelectionTrayBounds by remember { mutableStateOf<Rect?>(null) }
    var surfaceSize by remember { mutableStateOf(IntSize.Zero) }
    val mapCompositionSurfaceWasSized = surfaceSize.width > 0 && surfaceSize.height > 0
    if (mapCompositionSurfaceWasSized) {
        mapCompositionStartedAtMs?.let { startedAtMs ->
            SideEffect {
                startupPerfTrace?.mark("map_sized_composition_committed", startedAtMs)
            }
        }
    }
    var startupVectorContentReady by remember(startupPerfTrace, uiSession) { mutableStateOf(false) }
    var startupVectorFrameReady by remember(startupPerfTrace, uiSession) { mutableStateOf(false) }
    var startupRasterContentReady by remember(startupPerfTrace, uiSession) { mutableStateOf(false) }
    var startupRasterFrameReady by remember(startupPerfTrace, uiSession) { mutableStateOf(false) }
    var flightPlanRouteProjection by remember(uiSession) {
        mutableStateOf(
            FlightPlanRouteProjection(
                flightPlanRouteRevision = -1,
                segments = emptyList(),
            ),
        )
    }
    val flightPlanRoute =
        if (flightPlanRouteProjection.flightPlanRouteRevision == sessionSnapshot.flightPlanRouteRevision) {
            flightPlanRouteProjection.segments
        } else {
            emptyList()
        }
    val flightPlanRouteDistanceAnnotations =
        if (flightPlanRouteProjection.flightPlanRouteRevision == sessionSnapshot.flightPlanRouteRevision) {
            flightPlanRouteProjection.distanceAnnotations
        } else {
            emptyList()
        }
    var mapGestureActive by remember { mutableStateOf(false) }
    val selectedMapId = selectedMap.selectedMapId
    val selectedFamilyId = selectedMap.selectedFamilyId
    val viewportState = remember(selectedMapId) { mutableStateOf(viewport) }
    val followTargetGate = remember(uiSession) { MapFollowTargetGate() }
    var viewportSyncPending by remember(selectedMapId) { mutableStateOf(false) }
    LaunchedEffect(viewport, selectedMapId, mapSelection?.centeredViewport) {
        val ownedViewport = viewportOwnedByCenteredInspection(
            requestedViewport = viewport,
            centeredInspectionViewport = mapSelection?.centeredViewport,
        )
        if (!sameMapViewport(ownedViewport, viewport)) {
            // The exact chart-search result owns the viewport until its inspector is
            // dismissed. Reassert it if a stale parent update arrives after the first
            // successful parent/local acknowledgement.
            viewportState.value = ownedViewport
            viewportSyncPending = true
            actions.onViewportChange(ownedViewport)
            return@LaunchedEffect
        }
        val parentMatchesLocal = sameMapViewport(viewport, viewportState.value)
        perfLogInfo(MapViewportLogTag) {
            "prop-sync map=$selectedMapId parentZoom=${"%.2f".format(viewport.zoom)} localZoom=${"%.2f".format(viewportState.value.zoom)} parentCenter=${"%.3f".format(viewport.centerWorldX)},${"%.3f".format(viewport.centerWorldY)} localCenter=${"%.3f".format(viewportState.value.centerWorldX)},${"%.3f".format(viewportState.value.centerWorldY)} pending=$viewportSyncPending matches=$parentMatchesLocal"
        }
        when {
            !viewportSyncPending -> {
                viewportState.value = viewport
            }
            parentMatchesLocal -> {
                viewportSyncPending = false
            }
            else -> {
                perfLogInfo(MapViewportLogTag) {
                    "prop-sync ignored stale parent map=$selectedMapId parentCenter=${"%.3f".format(viewport.centerWorldX)},${"%.3f".format(viewport.centerWorldY)} localCenter=${"%.3f".format(viewportState.value.centerWorldX)},${"%.3f".format(viewportState.value.centerWorldY)}"
                }
            }
        }
    }
    val currentViewport = viewportState.value
    val surfaceWidthPx = surfaceSize.width.toFloat()
    val surfaceHeightPx = surfaceSize.height.toFloat()
    val plannedMapUpDeg = mapOrientationMemory.resolve(mapOrientationMode, ownship.trackDegTrue)
    val displayViewport = currentViewport.copy(rotationDeg = plannedMapUpDeg)
    val planningDiameterPx = hypot(surfaceWidthPx, surfaceHeightPx)
    val planningEnvelope = ScreenPoint(planningDiameterPx, planningDiameterPx)
    val planningSurfaceSize = IntSize(
        ceil(planningEnvelope.x.toDouble()).toInt(),
        ceil(planningEnvelope.y.toDouble()).toInt(),
    )
    val mapDisplayScale = density.density.toDouble().takeIf { it.isFinite() && it > 0.0 } ?: 1.0
    val interactiveMaxZoom = physicalDisplayMaxZoom(selectedMap.maxZoom, mapDisplayScale)
    val mapLayerState = sessionSnapshot.mapLayerState
    val terrainLayerState = rememberTerrainLayerState(
        context = context,
        uiSession = uiSession,
        sessionWorkRunner = sessionWorkRunner,
        viewport = currentViewport,
        surfaceSize = planningSurfaceSize,
        surfaceWidthPx = planningEnvelope.x,
        surfaceHeightPx = planningEnvelope.y,
        visible = mapLayerState.terrainWarning.visible,
        mapVisible = page == AppPage.Map,
        devServerBaseUrl = devServerBaseUrl,
        ownshipAltitudeBucketFt = ownship.terrainAltitudeBucketFt,
        ownshipPosition = ownship.position,
        dataStatusState = sessionSnapshot.dataStatusState,
        ownshipLauncherLabel = ownshipControls.launcherLabel,
    )
    val terrainOverlay = terrainLayerState.images
    val nexradLayerState = rememberNexradLayerState(
        context = context,
        uiSession = uiSession,
        sessionWorkRunner = sessionWorkRunner,
        viewport = currentViewport,
        surfaceSize = planningSurfaceSize,
        surfaceWidthPx = planningEnvelope.x,
        surfaceHeightPx = planningEnvelope.y,
        visible = mapLayerState.nexrad.visible,
        enabled = mapLayerState.nexrad.enabled,
        mapVisible = page == AppPage.Map,
        liveFeedGeneration = liveFeedGeneration,
        devServerBaseUrl = devServerBaseUrl,
    )
    val nexradFrame = nexradLayerState.frame
    val surfaceWidthDp = remember(surfaceSize, density) { with(density) { surfaceSize.width.toDp().value } }
    val surfaceHeightDp = remember(surfaceSize, density) { with(density) { surfaceSize.height.toDp().value } }
    LaunchedEffect(startupPerfTrace, surfaceSize) {
        if (startupPerfTrace != null && surfaceSize.width > 0 && surfaceSize.height > 0) {
            startupPerfTrace.mark(
                "map_surface_ready",
                detail = "surface=${surfaceSize.width}x${surfaceSize.height}",
            )
        }
    }
    val situationDockLowered = surfaceWidthDp.dp < SituationDockOverlapWidth
    val situationDockTopPadding =
        if (situationDockLowered) ThumbSize + (ThumbGap * 2f) else ThumbGap
    val rasterPlanFrame = rememberRasterPlanFrame(
        selectedMapId = selectedMapId,
        displayViewport = displayViewport,
        surfaceSize = surfaceSize,
        surfaceWidthDp = surfaceWidthDp,
        surfaceHeightDp = surfaceHeightDp,
        mapDisplayScale = mapDisplayScale,
        uiSession = uiSession,
        fastTiles = debugState.fastTiles,
        startupPerfTrace = startupPerfTrace,
        pageTilePaintTiming = pageTilePaintTiming,
    )
    val tiles = rasterPlanFrame.tiles
    val chartReferenceAction = rasterPlanFrame.chartReferenceAction
    LaunchedEffect(startupPerfTrace, startupVectorContentReady) {
        if (startupPerfTrace != null && startupVectorContentReady && !startupVectorFrameReady) {
            withFrameNanos { }
            startupPerfTrace.mark("vector_frame_ready")
            startupVectorFrameReady = true
        }
    }
    LaunchedEffect(startupPerfTrace, startupRasterContentReady) {
        if (startupPerfTrace != null && startupRasterContentReady && !startupRasterFrameReady) {
            withFrameNanos { }
            startupPerfTrace.mark("raster_frame_ready")
            startupRasterFrameReady = true
        }
    }
    LaunchedEffect(
        startupPerfTrace,
        startupVectorFrameReady,
        startupRasterFrameReady,
        surfaceSize,
    ) {
        if (
            startupPerfTrace != null &&
            startupVectorFrameReady &&
            startupRasterFrameReady &&
            surfaceSize.width > 0 &&
            surfaceSize.height > 0
        ) {
            withFrameNanos { }
            // The terminal observation is authoritative and closes the trace. Restate
            // its prerequisites so concurrent frame effects cannot lose their markers.
            startupPerfTrace.mark("vector_frame_ready")
            startupPerfTrace.mark("raster_frame_ready")
            startupPerfTrace.mark(
                "chart_usable",
                detail = "surface=${surfaceSize.width}x${surfaceSize.height} tiles=${tiles.size}",
            )
            startupPerfTrace.finish()
        }
    }
    LaunchedEffect(
        startupPerfTrace,
        mapLayerState.terrainWarning.visible,
        ownship.position,
        ownship.terrainAltitudeBucketFt,
    ) {
        startupPerfTrace?.mark(
            "terrain_startup_state",
            detail = "visible=${mapLayerState.terrainWarning.visible} " +
                "position=${ownship.position != null} altitude=${ownship.terrainAltitudeBucketFt != null}",
        )
    }
    val menuTrayOpen = chartTrayOpen || layerTrayOpen || openStatusControlId != null || situationTrayOpen
    val trayOptions = remember(mapFamilyOptions) {
        mapFamilyOptions.map { option ->
            ChartTrayOption(
                id = option.id,
                label = option.label,
                launcherLabel = option.launcherLabel,
                available = option.enabled,
                disabledReason = option.disabledReason,
                iconResId = chartFamilyIconResId(option.id),
            ) {
                actions.onSelectMapFamily(option.id)
            }
        }
    }
    val layerTrayOptions = remember(mapLayerState) {
        mapLayerState.options.map { option ->
            val toggleState = mapLayerState.toggleState(option.layerId)
            MenuDockOption(
                key = option.layerId.name,
                label = option.label,
                enabled = toggleState.enabled,
                disabledReason = toggleState.disabledReason,
                toggleState = toggleState,
                iconResId = mapLayerIconResId(option.layerId),
            ) {
                val visible = !toggleState.visible
                val startMs = SystemClock.elapsedRealtime()
                actions.onBeforeMapLayerCommand()
                if (applySessionCommand("setMapLayerVisibility") {
                        uiSession.setMapLayerVisibility(option.layerId, visible)
                    } != null) {
                    diagnosticLogInfo(MapLayerLogTag) {
                        "toggle layer=${option.layerId.name} visible=$visible coreMs=${SystemClock.elapsedRealtime() - startMs}"
                    }
                }
            }
        }
    }
    val selectedLauncher = trayOptions.firstOrNull { option -> option.id == selectedFamilyId } ?: trayOptions.first()
    val tileRects = remember(tiles) {
        tiles.associate { tile ->
            val leftPx = tile.leftPx.roundToInt()
            val topPx = tile.topPx.roundToInt()
            val rightPx = (tile.leftPx + tile.sizePx).roundToInt()
            val bottomPx = (tile.topPx + tile.sizePx).roundToInt()
            renderTileKey(tile) to TileRect(
                leftPx = leftPx,
                topPx = topPx,
                widthPx = rightPx - leftPx,
                heightPx = bottomPx - topPx,
            )
        }
    }
    val situationOverlay = remember(ownship, displayViewport, surfaceWidthPx, surfaceHeightPx) {
        resolveSituationOverlay(
            ownship = ownship,
            viewport = displayViewport,
            widthUnits = surfaceWidthPx,
            heightUnits = surfaceHeightPx,
            ringCandidates = situationRingCandidates,
        )
    }
    val mapFollowProbeTag = remember(
        mapFollowUiState.following,
        ownship.drawAircraft,
        ownship.position,
        displayViewport,
        surfaceWidthPx,
        surfaceHeightPx,
    ) {
        val position = ownship.position
        if (ownship.drawAircraft && position != null && surfaceWidthPx > 0f && surfaceHeightPx > 0f) {
            buildMapFollowProbeTag(
                following = mapFollowUiState.following,
                ownshipPosition = position,
                viewport = displayViewport,
                surfaceWidthPx = surfaceWidthPx,
                surfaceHeightPx = surfaceHeightPx,
            )
        } else {
            null
        }
    }
    val mapSelectionCenterProbeTag = remember(
        mapSelection?.centeredTargetLabel,
        mapSelection?.centeredTargetPosition,
        displayViewport,
        surfaceWidthPx,
        surfaceHeightPx,
    ) {
        val label = mapSelection?.centeredTargetLabel
        val position = mapSelection?.centeredTargetPosition
        if (label != null && position != null && surfaceWidthPx > 0f && surfaceHeightPx > 0f) {
            buildMapSelectionCenterProbeTag(
                targetLabel = label,
                targetPosition = position,
                viewport = displayViewport,
                surfaceWidthPx = surfaceWidthPx,
                surfaceHeightPx = surfaceHeightPx,
            )
        } else {
            null
        }
    }
    val mapSelectionProjectionState = remember(mapSelection, mapSelectionCenterProbeTag) {
        val selectedItem = mapSelection?.selectedItem
        val selectedCategoryId = selectedItem?.let { selected ->
            mapSelection?.result?.categories
                ?.firstOrNull { category -> category.items.any { it.id == selected.id } }
                ?.id
        }
        buildMapSelectionProjectionState(
            selectedLabel = selectedItem?.label,
            selectedCategoryId = selectedCategoryId,
            selectedText = mapSelectionHeaderText(selectedItem),
            centerProbeTag = mapSelectionCenterProbeTag,
            detailId = mapSelectionDetailProjectionId(mapSelection?.detailModal),
        )
    }
    fun syncFollowStateForViewport(nextViewport: MapViewportState) {
        if (!mapFollowUiState.following || surfaceWidthPx <= 0f || surfaceHeightPx <= 0f) {
            return
        }
        followTargetGate.beginSync()
        perfLogInfo(MapViewportLogTag) {
            "follow-sync begin revision=${sessionSnapshot.sessionRevision} zoom=${"%.2f".format(nextViewport.zoom)} center=${"%.3f".format(nextViewport.centerWorldX)},${"%.3f".format(nextViewport.centerWorldY)}"
        }
        runCatching {
            uiSession.syncMapFollow(
                nextViewport,
                surfaceWidthPx.toDouble(),
                surfaceHeightPx.toDouble(),
            )
        }.onSuccess { snapshot ->
            val target = snapshot.mapFollowTargetViewport?.let(::mapViewportFromCore)
            perfLogInfo(MapViewportLogTag) {
                "follow-sync result revision=${snapshot.sessionRevision} following=${snapshot.mapFollowUiState.following} targetZoom=${target?.zoom?.let { "%.2f".format(it) }} targetCenter=${target?.let { "%.3f,%.3f".format(it.centerWorldX, it.centerWorldY) }}"
            }
            followTargetGate.acknowledgeSyncSnapshot(
                following = snapshot.mapFollowUiState.following,
                targetRevision = snapshot.sessionRevision,
            )
            actions.onSessionSnapshotChange(snapshot)
        }
            .onFailure {
                followTargetGate.clear()
                Log.w(MapViewportLogTag, "map follow sync failed", it)
            }
    }

    fun updateViewport(
        nextViewport: MapViewportState,
        source: MapViewportUpdateSource,
        syncFollow: Boolean = true,
    ) {
        val ownedViewport = viewportOwnedByCenteredInspection(
            requestedViewport = nextViewport,
            centeredInspectionViewport = mapSelection?.centeredViewport,
        )
        if (!sameMapViewport(ownedViewport, nextViewport)) {
            perfLogInfo(MapViewportLogTag) {
                "ignored viewport update while chart-search inspection owns center"
            }
            return
        }
        // Only direct user input transfers ownership away from a pending search.
        // Automatic follow/replay movement must not starve an explicit inspection.
        chartSearchInspectionGate.viewportUpdated(source)
        val northUpViewport = nextViewport.copy(rotationDeg = 0.0)
        perfLogInfo(MapViewportLogTag) {
            "update map=$selectedMapId from=${"%.2f".format(viewportState.value.zoom)} to=${"%.2f".format(northUpViewport.zoom)} fromCenter=${"%.3f".format(viewportState.value.centerWorldX)},${"%.3f".format(viewportState.value.centerWorldY)} toCenter=${"%.3f".format(northUpViewport.centerWorldX)},${"%.3f".format(northUpViewport.centerWorldY)} syncFollow=$syncFollow"
        }
        viewportState.value = northUpViewport
        viewportSyncPending = true
        actions.onViewportChange(northUpViewport)
        if (syncFollow) {
            syncFollowStateForViewport(northUpViewport)
        }
    }

    fun currentPerfCacheStats(): AndroidPerfCacheStats {
        val rasterStats = decodedTileBitmapCache.stats()
        val (terrainEntries, terrainBytes) = terrainBitmapCacheStats(terrainLayerState.bitmapCache)
        val (nexradEntries, nexradBytes) = nexradFrameStats(nexradFrame)
        return AndroidPerfCacheStats(
            rasterDecodedEntries = rasterStats.entries,
            rasterDecodedBytes = rasterStats.bytes,
            terrainEntries = terrainEntries,
            terrainBytes = terrainBytes,
            nexradEntries = nexradEntries,
            nexradBytes = nexradBytes,
        )
    }

    RunMapSelectionPerfScenario(
        perfScenario = perfScenario,
        uiSession = uiSession,
        surfaceSize = surfaceSize,
        selectedMapId = selectedMapId,
        surfaceWidthPx = surfaceWidthPx,
        surfaceHeightPx = surfaceHeightPx,
        densityScale = density.density.toDouble(),
        selectedMapMinZoom = selectedMap.minZoom,
        interactiveMaxZoom = interactiveMaxZoom,
        sessionWorkRunner = sessionWorkRunner,
        context = context,
        devServerBaseUrl = devServerBaseUrl,
        updateViewport = { nextViewport ->
            updateViewport(nextViewport, MapViewportUpdateSource.Automatic, syncFollow = false)
        },
        onMapSelection = { selection -> mapSelection = selection },
    )

    var memoryStressScenarioStarted by remember(perfScenario?.id, uiSession) { mutableStateOf(false) }
    LaunchedEffect(perfScenario?.id, uiSession, surfaceSize, selectedMapId) {
        val scenario = perfScenario ?: return@LaunchedEffect
        if (scenario.id != AndroidPerfScenarioTerrainNexradMemoryStress || memoryStressScenarioStarted) {
            return@LaunchedEffect
        }
        if (surfaceWidthPx <= 0f || surfaceHeightPx <= 0f) {
            return@LaunchedEffect
        }
        memoryStressScenarioStarted = true
        val watchdog = AndroidMainThreadStallWatchdog(scenario)
        watchdog.start()
        val scenarioStartMs = SystemClock.elapsedRealtime()
        try {
            val scenarioNowMs = System.currentTimeMillis()
            applySessionCommand("registerOwnshipSource") {
                uiSession.registerOwnshipSource(
                    OwnshipSourceRegistration(
                        sourceId = PerfScenarioKorsOwnshipSourceId,
                        sourceKind = OwnshipSourceKind.FlightPlanSimulator,
                        displayName = "Perf KORS Ownship",
                        selectable = true,
                        autoEligible = false,
                    ),
                )
            }
            applySessionCommand("updateOwnshipSourceStatus") {
                uiSession.updateOwnshipSourceStatus(
                    OwnshipSourceStatusUpdate(
                        sourceId = PerfScenarioKorsOwnshipSourceId,
                        connectionState = SourceConnectionState.Connected,
                        enabled = true,
                        statusLabel = "KORS terrain perf",
                    ),
                )
            }
            applySessionCommand("pushSituationSample") {
                uiSession.pushSituationSample(
                    SituationSample(
                        sourceId = PerfScenarioKorsOwnshipSourceId,
                        sourceKind = OwnshipSourceKind.FlightPlanSimulator,
                        eventTimeEpochMs = scenarioNowMs,
                        receivedTimeEpochMs = scenarioNowMs,
                        position = LatLonPoint(
                            lat = PerfScenarioKorsStressCenterLat,
                            lon = PerfScenarioKorsStressCenterLon,
                        ),
                        horizontalAccuracyM = 5.0,
                        verticalAccuracyM = 10.0,
                        trackDegTrue = 315.0,
                        headingDegTrue = 315.0,
                        groundSpeedKt = 120.0,
                        altitudeMslFt = PerfScenarioKorsStressAltitudeMslFt,
                    ),
                )
            }
            applySessionCommand("selectOwnshipSource") {
                uiSession.selectOwnshipSource(OwnshipSelection.Source(PerfScenarioKorsOwnshipSourceId))
            }
            delay(250)
            val scenarioSnapshot = uiSession.refreshSnapshot()
            actions.onSessionSnapshotChange(scenarioSnapshot)
            Log.i(
                AndroidPerfScenarioTag,
                "start scenario=${scenario.id} surface=${surfaceSize.width}x${surfaceSize.height} density=${density.density} map=$selectedMapId terrainLayerEnabled=${mapLayerState.terrainWarning.enabled} nexradLayerEnabled=${mapLayerState.nexrad.enabled} snapshotOwnshipTerrainBucketFt=${scenarioSnapshot.appUiState.ownship.render.terrainAltitudeBucketFt} syntheticOwnship=${PerfScenarioKorsStressCenterLat},${PerfScenarioKorsStressCenterLon} altitudeMslFt=$PerfScenarioKorsStressAltitudeMslFt",
            )
            if (mapLayerState.terrainWarning.enabled && !mapLayerState.terrainWarning.visible) {
                applySessionCommand("setMapLayerVisibility") {
                    uiSession.setMapLayerVisibility(MapLayerId.TerrainWarning, true)
                }
            }
            if (mapLayerState.nexrad.enabled && !mapLayerState.nexrad.visible) {
                applySessionCommand("setMapLayerVisibility") {
                    uiSession.setMapLayerVisibility(MapLayerId.Nexrad, true)
                }
            }
            val center = latLonToWorld(PerfScenarioKorsStressCenterLat, PerfScenarioKorsStressCenterLon)
            val baseViewport = MapViewportState(
                centerWorldX = center.x,
                centerWorldY = center.y,
                zoom = clampZoom(PerfScenarioKorsStressZoom, selectedMap.minZoom, interactiveMaxZoom),
            )
            updateViewport(baseViewport, MapViewportUpdateSource.Automatic, syncFollow = false)
            delay(500)
            val baselineSample = logAndroidPerfMemorySample(scenario, "initial", currentPerfCacheStats())
            val scenarioEndMs = scenarioStartMs + scenario.memoryStressDurationMs
            var sampleIndex = 0
            while (SystemClock.elapsedRealtime() < scenarioEndMs) {
                val angle = sampleIndex.toDouble() * 0.57
                val dxPx = (cos(angle) * 420.0 + sin(angle * 0.41) * 160.0).toFloat()
                val dyPx = (sin(angle) * 360.0 + cos(angle * 0.37) * 140.0).toFloat()
                val nextViewport = dragViewport(
                    baseViewport.copy(zoom = baseViewport.zoom + ((sampleIndex % 5) - 2) * 0.12),
                    dxPx,
                    dyPx,
                )
                updateViewport(nextViewport, MapViewportUpdateSource.Automatic, syncFollow = false)
                nexradLayerState.requestRender()
                terrainLayerState.requestRender()
                delay(scenario.memorySampleIntervalMs)
                val sample = logAndroidPerfMemorySample(
                    scenario,
                    "stress_$sampleIndex",
                    currentPerfCacheStats(),
                )
                val growthBytes = sample.footprintBytes - baselineSample.footprintBytes
                if (growthBytes > scenario.memoryGrowthThresholdBytes) {
                    Log.w(
                        AndroidPerfScenarioTag,
                        "threshold_violation scenario=${scenario.id} kind=memory_growth growthBytes=$growthBytes thresholdBytes=${scenario.memoryGrowthThresholdBytes}",
                    )
                }
                sampleIndex += 1
            }
            delay(1_000)
            logAndroidPerfMemorySample(scenario, "final", currentPerfCacheStats())
            Log.i(
                AndroidPerfScenarioTag,
                "done scenario=${scenario.id} elapsedMs=${SystemClock.elapsedRealtime() - scenarioStartMs}",
            )
        } catch (error: CancellationException) {
            Log.i(
                AndroidPerfScenarioTag,
                "cancelled scenario=${scenario.id} elapsedMs=${SystemClock.elapsedRealtime() - scenarioStartMs}",
            )
            throw error
        } catch (error: Throwable) {
            Log.e(
                AndroidPerfScenarioTag,
                "failed scenario=${scenario.id} elapsedMs=${SystemClock.elapsedRealtime() - scenarioStartMs}: ${error.message}",
                error,
            )
        } finally {
            watchdog.stop()
        }
    }

    fun recenterOnNavRef(navRef: NavRef) {
        runCatching {
            val position = appCore.resolveNavRefPosition(navRef)
            val center = latLonToWorld(position.lat, position.lon)
            updateViewport(
                currentViewport.copy(
                    centerWorldX = center.x,
                    centerWorldY = center.y,
                ),
                MapViewportUpdateSource.Automatic,
            )
            chartSearchText = ""
            chartSearchOpen = false
            chartSearchLoading = false
            chartSearchError = null
            chartSearchSuggestions = emptyList()
        }.onFailure { error ->
            chartSearchLoading = false
            chartSearchError = "Search failed: ${error.message ?: error.toString()}"
        }
    }

    fun inspectNavRef(navRef: NavRef) {
        if (surfaceWidthPx <= 0f || surfaceHeightPx <= 0f) {
            recenterOnNavRef(navRef)
            return
        }
        val inspectionToken = chartSearchInspectionGate.begin()
        sessionWorkRunner.submitMapSelectionForNavRef(
            viewport = currentViewport,
            widthPx = surfaceWidthPx.toDouble(),
            heightPx = surfaceHeightPx.toDouble(),
            navRef = navRef,
            pointDisplayScale = density.density.toDouble(),
            fetchResource = { resource ->
                fetchMapOverlayCoreResource(context, resource, devServerBaseUrl)
            },
            onResult = inspectionResult@ { inspection ->
                if (!chartSearchInspectionGate.owns(inspectionToken)) {
                    return@inspectionResult
                }
                val center = latLonToWorld(inspection.position.lat, inspection.position.lon)
                val nextViewport = currentViewport.copy(
                    centerWorldX = center.x,
                    centerWorldY = center.y,
                    zoom = inspection.targetZoom,
                )
                val point = worldToScreen(
                    nextViewport,
                    Offset(center.x.toFloat(), center.y.toFloat()),
                    surfaceWidthPx,
                    surfaceHeightPx,
                )
                mapSelection = MapSelectionUiState(
                    point = point,
                    result = inspection.selection,
                    selectedItem = mapSelectionItemById(inspection.selection, inspection.selectedItemId),
                    centeredTargetLabel = navRefLabel(navRef),
                    centeredTargetPosition = inspection.position,
                    centeredViewport = nextViewport,
                )
                updateViewport(nextViewport, MapViewportUpdateSource.Automatic)
                chartTrayOpen = false
                layerTrayOpen = false
                openStatusControlId = null
                situationTrayOpen = false
                chartSearchText = ""
                chartSearchOpen = false
                chartSearchLoading = false
                chartSearchError = null
                chartSearchSuggestions = emptyList()
            },
            onError = inspectionError@ { error ->
                if (!chartSearchInspectionGate.owns(inspectionToken)) {
                    return@inspectionError
                }
                chartSearchInspectionGate.invalidate()
                chartSearchLoading = false
                chartSearchError = "Search failed: ${error.message ?: error.toString()}"
            },
            onDropped = inspectionDropped@ { reason ->
                if (!chartSearchInspectionGate.owns(inspectionToken)) {
                    return@inspectionDropped
                }
                chartSearchInspectionGate.invalidate()
                chartSearchLoading = false
                chartSearchError = "Search interrupted: $reason"
            },
        )
    }

    fun submitChartSearch() {
        val query = chartSearchText
        if (query.isBlank()) {
            return
        }
        chartSearchLoading = true
        chartSearchError = null
        runCatching {
            appCore.resolveNavRefIdentifier(query)
        }.onSuccess { navRef ->
            inspectNavRef(navRef)
        }.onFailure { error ->
            chartSearchLoading = false
            chartSearchError = "No waypoint match for $query: ${error.message ?: error.toString()}"
            chartSearchSuggestions = emptyList()
        }
    }

    LaunchedEffect(chartSearchText, currentViewport.centerWorldX, currentViewport.centerWorldY) {
        val query = chartSearchText
        if (query.isBlank()) {
            chartSearchLoading = false
            chartSearchError = null
            chartSearchSuggestions = emptyList()
            return@LaunchedEffect
        }
        chartSearchLoading = true
        chartSearchError = null
        val (centerLat, centerLon) = viewportCenterLatLon(currentViewport)
        try {
            val suggestions = withContext(Dispatchers.IO) {
                appCore.suggestWaypointIdentifiersNear(
                    anchor = LatLonPoint(centerLat, centerLon),
                    query = query,
                    limit = 8,
                )
            }
            chartSearchLoading = false
            chartSearchSuggestions = suggestions
        } catch (error: CancellationException) {
            throw error
        } catch (error: Throwable) {
            chartSearchLoading = false
            chartSearchSuggestions = emptyList()
            chartSearchError = error.message ?: error.toString()
        }
    }

    val aircraftPlanViewPath = rememberAircraftPlanViewPath(sessionSnapshot.appUiState.aircraftPlanViewPath)
    val tileBitmapCache = rememberRasterTileBitmapCache(
        context = context,
        selectedMapId = selectedMapId,
        fastTiles = debugState.fastTiles,
        navDataEpoch = navDataEpoch,
        tiles = tiles,
        currentViewport = currentViewport,
        decodedTileBitmapCache = decodedTileBitmapCache,
        startupPerfTrace = startupPerfTrace,
        pageTilePaintTiming = pageTilePaintTiming,
        onRasterContentReady = { startupRasterContentReady = true },
        onPageTilePaintTimingComplete = actions.onPageTilePaintTimingComplete,
    )
    val airportInfoScope = rememberCoroutineScope()
    val mapRenderPaints = rememberMapRenderPaints(uiTheme)
    LaunchedEffect(selectedMapId) {
        chartTrayOpen = false
        layerTrayOpen = false
        openStatusControlId = null
        situationTrayOpen = false
        mapSelection = null
    }
    LaunchedEffect(uiSession, sessionSnapshot.flightPlanRouteRevision) {
        runCatching {
            uiSession.projectFlightPlanRoute()
        }.onSuccess { projection ->
            flightPlanRouteProjection = projection
            val guidance = sessionSnapshot.appUiState.activePlan?.guidance
            val directTo = guidance?.directTo
            Log.i(
                "AerobagGuidance",
                "route projection revision=${sessionSnapshot.sessionRevision} " +
                    "mode=${guidance?.sequencingMode} " +
                    "activeRow=${guidance?.activeToRowUid} " +
                    "activeSummary=${guidance?.navElement?.activeLegSummary} " +
                    "cdi=${guidance?.navElement?.cdiIndicatorDots} " +
                    "directTarget=${directTo?.target} " +
                    "directTargetRow=${directTo?.targetRowId} " +
                    "statuses=${projection.segments.joinToString(",") { segment -> "${segment.id}:${segment.status}" }}",
            )
        }.onFailure {
            flightPlanRouteProjection = FlightPlanRouteProjection(
                flightPlanRouteRevision = sessionSnapshot.flightPlanRouteRevision,
                segments = emptyList(),
            )
            Log.e("AerobagGuidance", "failed to project flight plan route", it)
        }
    }
    LaunchedEffect(selectedMapId, menuTrayOpen) {
        if (!menuTrayOpen) {
            withFrameNanos { }
            focusRequester.requestFocus()
        }
    }
    LaunchedEffect(uiSession, mapFollowUiState.following, mapFollowTargetViewport, viewport) {
        if (mapFollowUiState.following && mapFollowTargetViewport == null) {
            applySessionCommand("engageMapFollow") { uiSession.engageMapFollow(viewport) }
        }
    }
    LaunchedEffect(
        mapFollowUiState.following,
        mapFollowTargetViewport,
        sessionSnapshot.sessionRevision,
        mapGestureActive,
    ) {
        if (!mapFollowUiState.following) {
            followTargetGate.clear()
            return@LaunchedEffect
        }
        if (mapGestureActive) {
            return@LaunchedEffect
        }
        val target = mapFollowTargetViewport ?: return@LaunchedEffect
        val nextViewport = mapViewportFromCore(target)
        val minimumTargetRevision = followTargetGate.minimumRevision()
        perfLogInfo(MapViewportLogTag) {
            "follow-target revision=${sessionSnapshot.sessionRevision} minimumRevision=$minimumTargetRevision targetZoom=${"%.2f".format(nextViewport.zoom)} targetCenter=${"%.3f".format(nextViewport.centerWorldX)},${"%.3f".format(nextViewport.centerWorldY)} localZoom=${"%.2f".format(viewportState.value.zoom)} localCenter=${"%.3f".format(viewportState.value.centerWorldX)},${"%.3f".format(viewportState.value.centerWorldY)}"
        }
        if (!followTargetGate.shouldApplyTarget(sessionSnapshot.sessionRevision)) {
            perfLogInfo(MapViewportLogTag) {
                "follow-target stale revision=${sessionSnapshot.sessionRevision} minimumRevision=$minimumTargetRevision"
            }
            return@LaunchedEffect
        }
        if (!sameMapViewport(nextViewport, viewportState.value)) {
            updateViewport(nextViewport, MapViewportUpdateSource.Automatic, syncFollow = false)
        }
    }
    val displayedMapOverlay = rememberDisplayedMapOverlay(
        context = context,
        uiSession = uiSession,
        sessionWorkRunner = sessionWorkRunner,
        navDataEpoch = navDataEpoch,
        liveFeedGeneration = liveFeedGeneration,
        invalidationRevision = uiInvalidationRevisions.mapOverlay,
        viewport = currentViewport,
        displayViewport = displayViewport,
        surfaceSize = surfaceSize,
        overlayWidthPx = planningEnvelope.x,
        overlayHeightPx = planningEnvelope.y,
        displayWidthPx = surfaceWidthPx,
        displayHeightPx = surfaceHeightPx,
        densityScale = density.density,
        vectorsVisible = mapLayerState.vectors.visible,
        metarsVisible = mapLayerState.metars.visible,
        trafficVisible = mapLayerState.traffic.visible,
        offlineRegionsVisible = mapLayerState.offlineRegions.visible,
        devServerBaseUrl = devServerBaseUrl,
        startupPerfTrace = startupPerfTrace,
        onVectorContentReady = { startupVectorContentReady = true },
    )
    LaunchedEffect(currentViewport, surfaceWidthPx, surfaceHeightPx, tiles, nexradFrame, terrainOverlay) {
        if (surfaceWidthPx <= 0f || surfaceHeightPx <= 0f) return@LaunchedEffect
        if (!VerbosePerfLogs) return@LaunchedEffect
        val topLeftWorld = screenToWorldOffset(currentViewport, 0f, 0f, surfaceWidthPx, surfaceHeightPx)
        val bottomRightWorld = screenToWorldOffset(currentViewport, surfaceWidthPx, surfaceHeightPx, surfaceWidthPx, surfaceHeightPx)
        val sampleTile = tiles.firstOrNull()
        val sampleTerrain = terrainOverlay.firstOrNull()
        val sampleNexrad = nexradFrame?.images?.firstOrNull()
        val nexradMessage =
            if (sampleNexrad == null) {
                "nexrad=none"
            } else {
                "nexrad=tile=${sampleNexrad.tile.src} nwScreen=${"%.1f".format(sampleNexrad.tile.corners.nw.x)},${"%.1f".format(sampleNexrad.tile.corners.nw.y)} seScreen=${"%.1f".format(sampleNexrad.tile.corners.se.x)},${"%.1f".format(sampleNexrad.tile.corners.se.y)}"
            }
        val terrainMessage =
            if (sampleTerrain == null) {
                "terrain=none"
            } else {
                val tilesAtZoom = 2.0.pow(sampleTerrain.z.toDouble())
                val tileWorldSize = WebMercatorWorldSize / tilesAtZoom
                val yXyz = (tilesAtZoom - 1.0) - sampleTerrain.yTms.toDouble()
                val nwWorld = Offset((sampleTerrain.x * tileWorldSize).toFloat(), (yXyz * tileWorldSize).toFloat())
                val seWorld = Offset(((sampleTerrain.x + 1.0) * tileWorldSize).toFloat(), ((yXyz + 1.0) * tileWorldSize).toFloat())
                val nwScreen = worldToScreen(currentViewport, nwWorld, surfaceWidthPx, surfaceHeightPx)
                val seScreen = worldToScreen(currentViewport, seWorld, surfaceWidthPx, surfaceHeightPx)
                "terrain=z${sampleTerrain.z}/${sampleTerrain.x}/${sampleTerrain.yTms} nwWorld=${"%.3f".format(nwWorld.x)},${"%.3f".format(nwWorld.y)} seWorld=${"%.3f".format(seWorld.x)},${"%.3f".format(seWorld.y)} nwScreen=${"%.1f".format(nwScreen.x)},${"%.1f".format(nwScreen.y)} seScreen=${"%.1f".format(seScreen.x)},${"%.1f".format(seScreen.y)}"
            }
        val chartMessage =
            if (sampleTile == null) {
                "chart=none"
            } else {
                "chart=${sampleTile.mapViewId} z${sampleTile.zoom}/${sampleTile.x}/${sampleTile.yTms} screen=${"%.1f".format(sampleTile.leftPx)},${"%.1f".format(sampleTile.topPx)} size=${"%.1f".format(sampleTile.sizePx)}"
            }
        perfLogInfo(MapLayerLogTag) {
            "viewport zoom=${"%.2f".format(currentViewport.zoom)} center=${"%.3f".format(currentViewport.centerWorldX)},${"%.3f".format(currentViewport.centerWorldY)} worldTL=${"%.3f".format(topLeftWorld.x)},${"%.3f".format(topLeftWorld.y)} worldBR=${"%.3f".format(bottomRightWorld.x)},${"%.3f".format(bottomRightWorld.y)} $chartMessage $terrainMessage $nexradMessage"
        }
    }
    DisposableEffect(activity) {
        if (activity != null) {
            activity.onHardwareZoomDelta = null
        }
        onDispose {
            if (activity != null && activity.onHardwareZoomDelta != null) {
                activity.onHardwareZoomDelta = null
            }
        }
    }
    LaunchedEffect(mapSelection) {
        if (mapSelection == null) {
            mapSelectionTrayBounds = null
        }
    }
    fun requestMapSelection(point: Offset) {
        val selectionViewport = viewportState.value.copy(rotationDeg = plannedMapUpDeg)
        val world = screenToWorld(
            selectionViewport,
            ScreenPoint(point.x, point.y),
            surfaceWidthPx,
            surfaceHeightPx,
        )
        val (lat, lon) = worldToLatLon(world.x, world.y)
        sessionWorkRunner.submitMapSelection(
            viewport = selectionViewport,
            widthPx = surfaceWidthPx.toDouble(),
            heightPx = surfaceHeightPx.toDouble(),
            click = LatLonPoint(lat = lat, lon = lon),
            pointDisplayScale = density.density.toDouble(),
            fetchResource = { resource ->
                fetchMapOverlayCoreResource(context, resource, devServerBaseUrl)
            },
            onResult = { result ->
                mapSelection = MapSelectionUiState(
                    point = point,
                    result = result,
                    selectedItem = mapSelectionItemById(result, result.initialSelectedItemId),
                )
                chartTrayOpen = false
                layerTrayOpen = false
                openStatusControlId = null
                situationTrayOpen = false
            },
            onError = { error ->
                Log.w("AerobagSelection", "map selection failed", error)
            },
        )
    }
    fun performSelectedMapAction(action: MapSelectionAction) {
        val actionUid = action.actionUid ?: return
        val decision = try {
            uiSession.mapSelectionActionDecision(actionUid)
        } catch (error: Throwable) {
            actions.onSessionCommandFailure(error)
            return
        }
        if (decision.performSessionMutation &&
            applySessionCommand("performMapSelectionUiAction") {
                uiSession.performMapSelectionUiAction(actionUid)
            } == null
        ) {
            return
        }
        when (val effect = decision.effect) {
            is MapSelectionActionEffect.ShowWeather -> {
                mapSelection = mapSelection?.copy(
                    detailModal = MapSelectionDetailModalState(
                        title = "WX ${effect.detail.stationId}",
                        weatherDetail = effect.detail,
                    ),
                )
            }
            is MapSelectionActionEffect.LoadAirportInfo -> {
                mapSelection = mapSelection?.copy(
                    detailModal = MapSelectionDetailModalState(
                        title = effect.airportId,
                        text = effect.loadingText,
                    ),
                )
                airportInfoScope.launch {
                    runCatching {
                        withContext(Dispatchers.IO) {
                            uiSession.airportInfo(effect.airportId)
                        }
                    }.onSuccess { detail ->
                        mapSelection = mapSelection?.let { current ->
                            if (
                                current.detailModal?.title == effect.airportId &&
                                current.detailModal.text == effect.loadingText
                            ) {
                                current.copy(
                                    detailModal = MapSelectionDetailModalState(
                                        title = effect.airportId,
                                        airportInfo = detail,
                                    ),
                                )
                            } else {
                                current
                            }
                        }
                    }.onFailure { error ->
                        mapSelection = mapSelection?.let { current ->
                            if (
                                current.detailModal?.title == effect.airportId &&
                                current.detailModal.text == effect.loadingText
                            ) {
                                current.copy(
                                    detailModal = MapSelectionDetailModalState(
                                        title = effect.airportId,
                                        text = "${effect.failurePrefix} ${error.message ?: error}",
                                    ),
                                )
                            } else {
                                current
                            }
                        }
                    }
                }
            }
            is MapSelectionActionEffect.ShowDetail -> {
                mapSelection = mapSelection?.copy(
                    detailModal = MapSelectionDetailModalState(
                        title = effect.title,
                        sourceActionId = action.id,
                        text = effect.text,
                        status = effect.status,
                    ),
                )
            }
            is MapSelectionActionEffect.OpenPlateTarget -> {
                actions.onOpenPlateTarget(effect.airportId, effect.target, effect.chartId)
            }
            is MapSelectionActionEffect.OpenExternalUrl -> uriHandler.openUri(effect.url)
            null -> Unit
        }
        if (decision.dismissSelection) {
            mapSelection = null
        }
    }

    fun toggleOpenMapSelectionTimeDisplay(actionId: String) {
        val previous = mapSelection ?: return
        val selectedItemId = previous.selectedItem?.id
        val sourceActionId = previous.detailModal?.sourceActionId
        if (applySessionCommand("performTimeDisplayAction") {
                uiSession.performTimeDisplayAction(actionId)
            } == null
        ) {
            return
        }
        sessionWorkRunner.submitMapSelection(
            viewport = viewportState.value.copy(rotationDeg = plannedMapUpDeg),
            widthPx = surfaceWidthPx.toDouble(),
            heightPx = surfaceHeightPx.toDouble(),
            click = LatLonPoint(
                lat = previous.result.clickLat,
                lon = previous.result.clickLon,
            ),
            pointDisplayScale = density.density.toDouble(),
            fetchResource = { resource ->
                fetchMapOverlayCoreResource(context, resource, devServerBaseUrl)
            },
            onResult = { result ->
                val current = mapSelection
                if (
                    current == null ||
                    current.result.clickLat != previous.result.clickLat ||
                    current.result.clickLon != previous.result.clickLon
                ) {
                    return@submitMapSelection
                }
                val selectedItem = mapSelectionItemById(result, selectedItemId)
                val detailAction = selectedItem?.actions?.firstOrNull { action -> action.id == sourceActionId }
                val detailEffect = detailAction?.actionUid?.let { actionUid ->
                    uiSession.mapSelectionActionDecision(actionUid).effect as? MapSelectionActionEffect.ShowDetail
                }
                mapSelection = current.copy(
                    result = result,
                    selectedItem = selectedItem,
                    detailModal = if (detailEffect != null) {
                        MapSelectionDetailModalState(
                            title = detailEffect.title,
                            sourceActionId = sourceActionId,
                            text = detailEffect.text,
                            status = detailEffect.status,
                        )
                    } else {
                        current.detailModal
                    },
                )
            },
            onError = { error ->
                Log.w("AerobagSelection", "map selection time refresh failed", error)
            },
        )
    }
    fun mapInputBlockedAt(position: Offset): Boolean {
        if (menuTrayOpen) {
            return true
        }
        val mapBounds = mapSurfaceBounds ?: return false
        val windowPosition = Offset(mapBounds.left + position.x, mapBounds.top + position.y)
        val selectionTrayBlocks = mapSelection != null && mapSelectionTrayBounds?.contains(windowPosition) == true
        return selectionTrayBlocks
    }
    val startupAttributionModifier = if (startupPerfTrace == null) {
        Modifier
    } else {
        Modifier
            .layout { measurable, constraints ->
                val startedAtMs = SystemClock.elapsedRealtime()
                val placeable = measurable.measure(constraints)
                layout(placeable.width, placeable.height) {
                    placeable.placeRelative(0, 0)
                    startupPerfTrace.mark("map_first_layout_completed", startedAtMs)
                    if (mapCompositionSurfaceWasSized) {
                        startupPerfTrace.mark("map_sized_layout_completed", startedAtMs)
                    }
                }
            }
            .drawWithContent {
                val startedAtMs = SystemClock.elapsedRealtime()
                drawContent()
                startupPerfTrace.mark("map_first_draw_completed", startedAtMs)
                if (mapCompositionSurfaceWasSized) {
                    startupPerfTrace.mark("map_sized_draw_completed", startedAtMs)
                }
            }
    }

    Box(
        modifier = Modifier
            .fillMaxSize()
            .e2eIndexedControl(
                semanticTag = "parity:map-surface",
                state = "enabled:true",
            )
            .testTag("parity:map-surface")
            .background(uiTheme.controls.chartSurfaceBg)
            .then(startupAttributionModifier)
            .onSizeChanged { surfaceSize = it }
            .onGloballyPositioned { coordinates -> mapSurfaceBounds = coordinates.boundsInWindow() }
            .focusRequester(focusRequester)
            .onPreviewKeyEvent { keyEvent ->
                if (keyEvent.nativeKeyEvent.action != AndroidKeyEvent.ACTION_DOWN ||
                    surfaceWidthPx == 0f ||
                    surfaceHeightPx == 0f
                ) {
                    return@onPreviewKeyEvent false
                }
                val delta = when (keyEvent.nativeKeyEvent.keyCode) {
                    AndroidKeyEvent.KEYCODE_EQUALS,
                    AndroidKeyEvent.KEYCODE_PLUS,
                    AndroidKeyEvent.KEYCODE_NUMPAD_ADD,
                    -> 0.35
                    AndroidKeyEvent.KEYCODE_MINUS,
                    AndroidKeyEvent.KEYCODE_NUMPAD_SUBTRACT,
                    -> -0.35
                    else -> return@onPreviewKeyEvent false
                }
                perfLogInfo(MapViewportLogTag) {
                    "key-zoom map=$selectedMapId delta=${"%.2f".format(delta)} base=${"%.2f".format(viewportState.value.zoom)}"
                }
                val nextViewport = zoomAroundPoint(
                    viewport = viewportState.value.copy(rotationDeg = plannedMapUpDeg),
                    minZoom = selectedMap.minZoom,
                    maxZoom = interactiveMaxZoom,
                    anchor = ScreenPoint(surfaceWidthPx / 2f, surfaceHeightPx / 2f),
                    widthPx = surfaceWidthPx,
                    heightPx = surfaceHeightPx,
                    nextZoom = clampZoom(viewportState.value.zoom + delta, selectedMap.minZoom, interactiveMaxZoom),
                )
                updateViewport(nextViewport, MapViewportUpdateSource.UserInput)
                true
            }
            .focusable()
            .pointerInput(
                selectedMapId,
                surfaceSize,
                menuTrayOpen,
                mapSelection,
                mapSelectionTrayBounds,
                mapSurfaceBounds,
                mapFollowUiState.following,
                ownshipControls.selection,
                plannedMapUpDeg,
            ) {
                if (surfaceWidthPx == 0f || surfaceHeightPx == 0f) {
                    return@pointerInput
                }
                awaitEachGesture {
                    var dragPointerId: PointerId? = null
                    var dragLastPosition: Offset? = null
                    var pinchSnapshot: org.aerobag.app.domain.PinchSnapshot? = null
                    var gestureViewport = viewportState.value
                    var movedViewportDuringGesture = false
                    var loggedGestureSeed = false
                    try {
                        while (true) {
                            val event = awaitPointerEvent()
                            val activeChanges = event.changes.filter { !it.isConsumed }
                            val pressed = activeChanges.filter { it.pressed }
                            if (pressed.isEmpty()) {
                                val endingDragChange = dragPointerId?.let { pointerId ->
                                    activeChanges.firstOrNull { it.id == pointerId }
                                }
                                val last = dragLastPosition
                                if (endingDragChange != null && last != null && !mapInputBlockedAt(endingDragChange.position)) {
                                    val dx = endingDragChange.position.x - last.x
                                    val dy = endingDragChange.position.y - last.y
                                    if (dx != 0f || dy != 0f) {
                                        gestureViewport = dragViewport(
                                            viewportState.value.copy(rotationDeg = plannedMapUpDeg),
                                            dx = dx,
                                            dy = dy,
                                        )
                                        movedViewportDuringGesture = true
                                        updateViewport(
                                            gestureViewport,
                                            MapViewportUpdateSource.UserInput,
                                            syncFollow = false,
                                        )
                                        actions.onViewportGestureActivity()
                                        endingDragChange.consume()
                                    }
                                }
                                break
                            }
                            if (pressed.any { mapInputBlockedAt(it.position) }) {
                                break
                            }
                            if (!mapGestureActive) {
                                mapGestureActive = true
                                actions.onViewportGestureActiveChange(true)
                            }
                                if (!loggedGestureSeed) {
                                    perfLogInfo(MapViewportLogTag) {
                                        "gesture-start map=$selectedMapId seed=${"%.2f".format(viewportState.value.zoom)} local=${"%.2f".format(viewportState.value.zoom)} center=${"%.3f".format(viewportState.value.centerWorldX)},${"%.3f".format(viewportState.value.centerWorldY)}"
                                    }
                                    gestureViewport = viewportState.value
                                    loggedGestureSeed = true
                            }
                            if (pressed.size == 1) {
                                val change = pressed.first()
                                if (dragPointerId != change.id || dragLastPosition == null) {
                                    dragPointerId = change.id
                                    dragLastPosition = change.position
                                    pinchSnapshot = null
                                    gestureViewport = viewportState.value
                                } else {
                                    val last = dragLastPosition ?: change.position
                                    gestureViewport = viewportState.value
                                    gestureViewport = dragViewport(
                                        gestureViewport.copy(rotationDeg = plannedMapUpDeg),
                                        dx = change.position.x - last.x,
                                        dy = change.position.y - last.y,
                                    )
                                    movedViewportDuringGesture = true
                                    updateViewport(
                                        gestureViewport,
                                        MapViewportUpdateSource.UserInput,
                                        syncFollow = false,
                                    )
                                    actions.onViewportGestureActivity()
                                    dragLastPosition = change.position
                                }
                                change.consume()
                            } else {
                                val first = pressed[0]
                                val second = pressed[1]
                                if (pinchSnapshot == null) {
                                    gestureViewport = viewportState.value
                                    pinchSnapshot = createPinchSnapshot(
                                        viewport = gestureViewport.copy(rotationDeg = plannedMapUpDeg),
                                        first = ScreenPoint(first.position.x, first.position.y),
                                        second = ScreenPoint(second.position.x, second.position.y),
                                        widthPx = surfaceWidthPx,
                                        heightPx = surfaceHeightPx,
                                    )
                                }
                                gestureViewport = viewportState.value
                                gestureViewport =
                                    applyPinchGesture(
                                        snapshot = pinchSnapshot,
                                        currentFirst = ScreenPoint(first.position.x, first.position.y),
                                        currentSecond = ScreenPoint(second.position.x, second.position.y),
                                        minZoom = selectedMap.minZoom,
                                        maxZoom = interactiveMaxZoom,
                                        widthPx = surfaceWidthPx,
                                        heightPx = surfaceHeightPx,
                                    )
                                movedViewportDuringGesture = true
                                updateViewport(
                                    gestureViewport,
                                    MapViewportUpdateSource.UserInput,
                                    syncFollow = false,
                                )
                                actions.onViewportGestureActivity()
                                first.consume()
                                second.consume()
                            }
                        }
                    } finally {
                        val completedGestureSyncViewport = mapFollowSyncViewportForCompletedGesture(
                            movedViewportDuringGesture = movedViewportDuringGesture,
                            finalGestureViewport = gestureViewport.copy(rotationDeg = 0.0),
                        )
                        if (completedGestureSyncViewport != null) {
                            syncFollowStateForViewport(completedGestureSyncViewport)
                        } else if (loggedGestureSeed && dragLastPosition != null) {
                            val point = dragLastPosition
                            requestMapSelection(point)
                        }
                        if (mapGestureActive) {
                            mapGestureActive = false
                            actions.onViewportGestureActiveChange(false)
                        }
                    }
                }
            }
            .pointerInteropFilter { event ->
                if (surfaceWidthPx == 0f || surfaceHeightPx == 0f) {
                    return@pointerInteropFilter false
                }
                if (mapInputBlockedAt(Offset(event.x, event.y))) {
                    return@pointerInteropFilter false
                }
                if (event.action == MotionEvent.ACTION_SCROLL) {
                    val wheelDelta = event.getAxisValue(MotionEvent.AXIS_VSCROLL).takeIf { it != 0f }
                        ?: event.getAxisValue(MotionEvent.AXIS_SCROLL)
                    val nextViewport = zoomAroundPoint(
                        viewport = viewportState.value.copy(rotationDeg = plannedMapUpDeg),
                        minZoom = selectedMap.minZoom,
                        maxZoom = interactiveMaxZoom,
                        anchor = ScreenPoint(surfaceWidthPx / 2f, surfaceHeightPx / 2f),
                        widthPx = surfaceWidthPx,
                        heightPx = surfaceHeightPx,
                        nextZoom = clampZoom(viewportState.value.zoom - wheelDelta * 0.28, selectedMap.minZoom, interactiveMaxZoom),
                    )
                    updateViewport(nextViewport, MapViewportUpdateSource.UserInput)
                    actions.onViewportGestureActivity()
                    true
                } else {
                    false
                }
            },
    ) {
        E2eProjectionView(
            viewId = R.id.e2e_viewport_projection,
            state = buildViewportProjectionState(currentViewport, plannedMapUpDeg),
            modifier = Modifier
                .align(Alignment.TopStart)
                .offset(x = 30.dp)
                .size(1.dp),
        )
        E2eProjectionView(
            viewId = R.id.e2e_map_family_projection,
            state = "$selectedFamilyId:map:$selectedMapId",
            modifier = Modifier
                .align(Alignment.TopStart)
                .offset(x = 2.dp)
                .size(1.dp),
        )
        mapLayerState.options.forEachIndexed { index, option ->
            val state = mapLayerState.toggleState(option.layerId)
            Box(
                modifier = Modifier
                    .align(Alignment.TopStart)
                    .offset(x = (4 + index * 2).dp)
                    .size(1.dp)
                    .testTag(
                        "parity:map-layer:${option.layerId.name}:visible:${state.visible}:enabled:${state.enabled}",
                    ),
            )
        }
        RasterImageLayers(
            tiles = tiles,
            tileRects = tileRects,
            tileBitmapCache = tileBitmapCache,
            tileLabels = debugState.tileLabels,
            tileLabelPaint = mapRenderPaints.tileLabel,
            tileLabelBackgroundPaint = mapRenderPaints.tileLabelBackground,
            nexradFrame = if (mapLayerState.nexrad.visible) nexradFrame else null,
            terrainOverlay = terrainOverlay,
            viewport = currentViewport,
            surfaceWidthPx = surfaceWidthPx,
            surfaceHeightPx = surfaceHeightPx,
            mapUpDeg = plannedMapUpDeg,
        )
        E2eProjectionView(
            viewId = R.id.e2e_raster_state_projection,
            state =
                "plan:${rasterPlanFrame.planId}:maps:${tiles.map { rasterSemanticToken(it.mapViewId) }.distinct().sorted().joinToString(",").ifEmpty { "none" }}:" +
                    "planned:${distinctRenderTileCount(tiles)}:loaded:${tileBitmapCache.values.count { it != null }}:failed:${tileBitmapCache.values.count { it == null }}",
            modifier = Modifier
                .align(Alignment.TopStart)
                .offset(x = 20.dp)
                .size(1.dp),
        )
        E2eProjectionView(
            viewId = R.id.e2e_vector_state_projection,
            state = "features:${displayedMapOverlay.visibleFeatures.size}",
            modifier = Modifier
                .align(Alignment.TopStart)
                .offset(x = 22.dp)
                .size(1.dp),
        )
        E2eProjectionView(
            viewId = R.id.e2e_live_overlay_projection,
            state =
                "metars:${displayedMapOverlay.visibleMetars.size}:" +
                    "pireps:${displayedMapOverlay.visiblePireps.size}:" +
                    "obstacles:${displayedMapOverlay.visibleFeatures.count { it.symbolKind == "obstacle" }}:" +
                    "tfrs:${displayedMapOverlay.tfrPaths.size}",
            modifier = Modifier
                .align(Alignment.TopStart)
                .offset(x = 24.dp)
                .size(1.dp),
        )
        E2eProjectionView(
            viewId = R.id.e2e_nexrad_state_projection,
            state =
                "tiles:${nexradFrame?.images?.size ?: 0}:" +
                    "frame:${nexradFrame?.selectedFrameIndex ?: "none"}:" +
                    "frames:${nexradFrame?.frameCount ?: 0}",
            modifier = Modifier
                .align(Alignment.TopStart)
                .offset(x = 32.dp)
                .size(1.dp),
        )
        E2eProjectionView(
            viewId = R.id.e2e_map_selection_projection,
            state = mapSelectionProjectionState,
            modifier = Modifier
                .align(Alignment.TopStart)
                .offset(x = 28.dp)
                .size(1.dp),
        )
        AirspaceOverlayLayer(displayedMapOverlay, density.density, uiTheme)
        MapFeatureOverlayLayer(
            displayedMapOverlay = displayedMapOverlay,
            uiTheme = uiTheme,
            densityScale = density.density,
            fixMarkerStrokeColor = mapRenderPaints.fixMarkerStrokeColor,
            fixMarkerFillColor = mapRenderPaints.fixMarkerFillColor,
            airportMarkerStrokeColor = mapRenderPaints.airportMarkerStrokeColor,
            airportToweredFillColor = mapRenderPaints.airportToweredFillColor,
            airportUntoweredFillColor = mapRenderPaints.airportUntoweredFillColor,
            vorMarkerColor = mapRenderPaints.vorMarkerColor,
            vorMarkerStrokeColor = mapRenderPaints.vorMarkerStrokeColor,
            fixLabelStrokePaint = mapRenderPaints.fixLabelStroke,
            airportLabelStrokePaint = mapRenderPaints.airportLabelStroke,
            vorLabelFillPaint = mapRenderPaints.vorLabelFill,
            fixLabelFillPaint = mapRenderPaints.fixLabelFill,
            airportToweredLabelFillPaint = mapRenderPaints.airportToweredLabelFill,
            airportUntoweredLabelFillPaint = mapRenderPaints.airportUntoweredLabelFill,
            mapUpDeg = plannedMapUpDeg,
        )
        ObservationOverlayLayer(displayedMapOverlay, density.density, uiTheme)
        OfflineRegionsOverlayLayer(displayedMapOverlay, density.density, uiTheme)
        if (flightPlanRoute.isNotEmpty() && surfaceWidthPx > 0f && surfaceHeightPx > 0f) {
            E2eProjectionView(
                viewId = R.id.e2e_flight_plan_route_overlay_projection,
                state = flightPlanRouteOverlayProjectionState(
                    flightPlanRoute = flightPlanRoute,
                    viewport = displayViewport,
                    surfaceWidthPx = surfaceWidthPx,
                    surfaceHeightPx = surfaceHeightPx,
                ),
                modifier = Modifier
                    .align(Alignment.TopStart)
                    .offset(x = 26.dp)
                    .size(1.dp),
            )
        }
        RouteOverlayLayer(
            flightPlanRoute = flightPlanRoute,
            distanceAnnotations = flightPlanRouteDistanceAnnotations,
            visibleFeatureIds = displayedMapOverlay.flightPlanFeatures.mapTo(mutableSetOf()) { it.id },
            viewport = displayViewport,
            surfaceWidthPx = surfaceWidthPx,
            surfaceHeightPx = surfaceHeightPx,
            densityScale = density.density,
            uiTheme = uiTheme,
        )
        MapFeatureOverlayLayer(
            displayedMapOverlay = displayedMapOverlay,
            uiTheme = uiTheme,
            densityScale = density.density,
            fixMarkerStrokeColor = mapRenderPaints.fixMarkerStrokeColor,
            fixMarkerFillColor = mapRenderPaints.fixMarkerFillColor,
            airportMarkerStrokeColor = mapRenderPaints.airportMarkerStrokeColor,
            airportToweredFillColor = mapRenderPaints.airportToweredFillColor,
            airportUntoweredFillColor = mapRenderPaints.airportUntoweredFillColor,
            vorMarkerColor = mapRenderPaints.vorMarkerColor,
            vorMarkerStrokeColor = mapRenderPaints.vorMarkerStrokeColor,
            fixLabelStrokePaint = mapRenderPaints.fixLabelStroke,
            airportLabelStrokePaint = mapRenderPaints.airportLabelStroke,
            vorLabelFillPaint = mapRenderPaints.vorLabelFill,
            fixLabelFillPaint = mapRenderPaints.fixLabelFill,
            airportToweredLabelFillPaint = mapRenderPaints.airportToweredLabelFill,
            airportUntoweredLabelFillPaint = mapRenderPaints.airportUntoweredLabelFill,
            flightPlanOnly = true,
            mapUpDeg = plannedMapUpDeg,
        )
        TrafficOverlayLayer(
            displayedMapOverlay = displayedMapOverlay,
            densityScale = density.density,
            uiTheme = uiTheme,
            mapUpDeg = plannedMapUpDeg,
        )
        MapSelectionHighlightLayer(
            selectedItem = mapSelection?.selectedItem,
            displayedMapOverlay = displayedMapOverlay,
            viewport = displayViewport,
            surfaceWidthPx = surfaceWidthPx,
            surfaceHeightPx = surfaceHeightPx,
            densityScale = density.density,
            uiTheme = uiTheme,
            mapUpDeg = plannedMapUpDeg,
        )
        SituationOverlayLayer(
            situationOverlay = situationOverlay,
            densityScale = density.density,
            labelStrokePaint = mapRenderPaints.situationLabelStroke,
            labelFillPaint = mapRenderPaints.situationLabelFill,
            aircraftPlanViewPath = aircraftPlanViewPath,
        )
        E2eProjectionView(
            viewId = R.id.e2e_ownship_state_projection,
            state =
                "mode:${ownship.mode.name.lowercase()}:" +
                    "source:${ownshipControls.sources.firstOrNull { it.active }?.sourceId ?: "none"}:" +
                    "draw:${ownship.drawAircraft}:" +
                    "position:${ownship.position?.let { "%.5f,%.5f".format(it.lat, it.lon) } ?: "none"}:" +
                    "track:${ownship.trackDegTrue?.let { "%.1f".format(it) } ?: "none"}",
            modifier = Modifier
                .align(Alignment.TopStart)
                .offset(x = 26.dp)
                .size(1.dp),
        )
        mapFollowProbeTag?.let { tag ->
            E2eProjectionView(
                viewId = R.id.e2e_map_follow_projection,
                state = tag.removePrefix("parity:map-follow-state:"),
                modifier = Modifier
                    .align(Alignment.TopStart)
                    .offset(x = 34.dp)
                    .size(1.dp),
            )
        }
        FlightDataBanner(
            banner = flightDataBanner,
            surfaceSize = surfaceSize,
            situationDockTopPadding = situationDockTopPadding,
            uiTheme = uiTheme,
            onAction = { actionId ->
                applySessionCommand("performTimeDisplayAction") {
                    uiSession.performTimeDisplayAction(actionId)
                }
            },
            modifier = Modifier.align(if (surfaceWidthPx > surfaceHeightPx) Alignment.TopEnd else Alignment.TopCenter),
        )
        Row(
            modifier = Modifier
                .align(Alignment.TopEnd)
                .padding(
                    top = situationDockTopPadding,
                    end = ThumbGap + MenuDockStyle.Situation.buttonWidth + ThumbGap,
                ),
            horizontalArrangement = Arrangement.spacedBy(ThumbGap),
            verticalAlignment = Alignment.Top,
        ) {
            sessionSnapshot.mapStatusControls.controls.forEach { control ->
                DataStatusBadge(
                    dataStatusState = control.state,
                    open = openStatusControlId == control.id,
                    onToggle = {
                        openStatusControlId = control.id.takeUnless { it == openStatusControlId }
                        situationTrayOpen = false
                        chartTrayOpen = false
                        layerTrayOpen = false
                    },
                    onAction = { actionId ->
                        val decision = uiSession.statusActionDecision(actionId)
                        if (decision.performSessionMutation) {
                            applySessionCommand("performStatusAction") {
                                uiSession.performStatusAction(actionId)
                            }
                        }
                        if (decision.platformEffect is UiStatusPlatformEffect.ReloadApplication) {
                            actions.onReloadApplication()
                        }
                    }
                )
            }
        }
        SituationStatusBadge(
            controls = ownshipControls,
            modifier = Modifier
                .align(Alignment.TopEnd)
                .padding(top = situationDockTopPadding, end = ThumbGap),
            open = situationTrayOpen,
            onToggle = {
                situationTrayOpen = !situationTrayOpen
                openStatusControlId = null
                chartTrayOpen = false
                layerTrayOpen = false
            },
            onSelectSource = { source ->
                if (!source.keepTrayOpenOnSelect) situationTrayOpen = false
                actions.onSelectOwnshipSource(source.sourceId)
            },
            onSituationControlInput = actions.onSituationControlInput,
            onTextAction = { actionId, value ->
                applySessionCommand("performOwnshipTextAction") {
                    uiSession.performOwnshipTextAction(actionId, value)
                }
            },
        )

        MapTopLeftControls(
            modifier = Modifier.align(Alignment.TopStart),
            selectedLabel = selectedLauncher.launcherLabel,
            chartReferenceFamilyId = chartReferenceAction?.family_id,
            onOpenChartReference = {
                chartReferenceAction?.let { action ->
                    actions.onOpenChartReference(action.family_id, action.suggested_chart_ids)
                }
            },
            trayOptions = trayOptions,
            trayOpen = chartTrayOpen,
            onToggle = {
                chartTrayOpen = !chartTrayOpen
                layerTrayOpen = false
                openStatusControlId = null
                situationTrayOpen = false
            },
            layerTrayOpen = layerTrayOpen,
            onToggleLayerTray = {
                layerTrayOpen = !layerTrayOpen
                chartTrayOpen = false
                openStatusControlId = null
                situationTrayOpen = false
            },
            layerOptions = layerTrayOptions,
            chartSearchText = chartSearchText,
            chartSearchOpen = chartSearchOpen,
            chartSearchLoading = chartSearchLoading,
            chartSearchError = chartSearchError,
            chartSearchSuggestions = chartSearchSuggestions,
            onChartSearchTextChange = { value ->
                if (value != chartSearchText) {
                    chartSearchInspectionGate.invalidate()
                    mapSelection = null
                }
                chartSearchText = value
                chartSearchOpen = true
            },
            onChartSearchFocus = { chartSearchOpen = true },
            onChartSearchSubmit = { submitChartSearch() },
            onChartSearchSuggestionClick = { suggestion -> inspectNavRef(suggestion.navRef.toNavRef()) },
            centerHereEnabled = mapFollowUiState.canCenterHere || mapFollowUiState.following,
            centerHereSelected = mapFollowUiState.following,
            centerHereDisabledReason = mapFollowUiState.disabledReason,
            onCenterHere = {
                followTargetGate.clear()
                applySessionCommand(if (mapFollowUiState.following) "disengageMapFollow" else "engageMapFollow") {
                    if (mapFollowUiState.following) {
                        uiSession.disengageMapFollow(viewportState.value)
                    } else {
                        uiSession.engageMapFollow(viewportState.value)
                    }
                }
            },
            mapOrientationMode = mapOrientationMode,
            compassNeedleRotationDeg = compassNeedleRotationDegrees(
                plannedMapUpDeg,
                ownship.magneticVariationDeg,
            ),
            onMapOrientationToggle = {
                actions.onMapOrientationModeChange(
                    if (mapOrientationMode == MapOrientationMode.North) {
                        MapOrientationMode.Track
                    } else {
                        MapOrientationMode.North
                    },
                )
            },
        )

        if (playbackPanelState.visible) {
            E2eProjectionView(
                viewId = R.id.e2e_playback_widget_projection,
                state =
                    "status:${playbackUiState.status.name.lowercase()}:" +
                        "cursor:${String.format("%.3f", playbackUiState.cursorSeconds)}:" +
                        "duration:${String.format("%.3f", playbackUiState.durationSeconds)}:" +
                        "rate:${String.format("%.2f", playbackUiState.rate)}:" +
                        "gaps:${playbackUiState.gapSpans.size}",
                modifier = Modifier
                    .align(Alignment.TopStart)
                    .offset(x = 36.dp)
                    .size(1.dp),
            )
            MapPlaybackWidgetOverlay(
                surfaceWidthDp = surfaceWidthDp,
                uiSession = uiSession,
                playbackUiState = playbackUiState,
                sourcePath = playbackSourcePath,
                onSourcePathChange = actions.onPlaybackSourcePathChange,
                onSnapshotChange = actions.onSessionSnapshotChange,
                onSessionCommandFailure = actions.onSessionCommandFailure,
                modifier = Modifier.align(Alignment.BottomStart),
            )
        }

        if (menuTrayOpen) {
            Scrim {
                chartTrayOpen = false
                layerTrayOpen = false
                openStatusControlId = null
                situationTrayOpen = false
            }
        }

        mapSelection?.let { selection ->
            Popup(
                onDismissRequest = { mapSelection = null },
                properties = PopupProperties(focusable = true, clippingEnabled = false),
            ) {
                Box(modifier = Modifier.fillMaxSize()) {
                    Scrim { mapSelection = null }
                    E2eProjectionView(
                        viewId = R.id.e2e_map_selection_projection,
                        state = mapSelectionProjectionState,
                        modifier = Modifier
                            .align(Alignment.TopStart)
                            .size(1.dp),
                    )
                    if (selection.detailModal != null) {
                        selection.detailModal.airportInfo?.let { airportInfo ->
                            AirportInfoModal(
                                detail = airportInfo,
                                onTimeDisplayAction = { actionId ->
                                    applySessionCommand("performTimeDisplayAction") {
                                        uiSession.performTimeDisplayAction(actionId)
                                    }
                                    val airportId = airportInfo.airportId
                                    airportInfoScope.launch {
                                        runCatching {
                                            withContext(Dispatchers.IO) {
                                                uiSession.airportInfo(airportId)
                                            }
                                        }.onSuccess { detail ->
                                            mapSelection = mapSelection?.copy(
                                                detailModal = MapSelectionDetailModalState(
                                                    title = airportId,
                                                    airportInfo = detail,
                                                ),
                                            )
                                        }
                                    }
                                },
                                modifier = Modifier
                                    .align(Alignment.Center)
                                    .zIndex(OverlayPlaneModal),
                            )
                        } ?: selection.detailModal.weatherDetail?.let { weatherDetail ->
                            WeatherDetailModal(
                                detail = weatherDetail,
                                modifier = Modifier
                                    .align(Alignment.Center)
                                    .zIndex(OverlayPlaneModal),
                            )
                        } ?: MapSelectionDetailModal(
                            title = selection.detailModal.title,
                            text = selection.detailModal.text.orEmpty(),
                            status = selection.detailModal.status,
                            onTimeDisplayAction = ::toggleOpenMapSelectionTimeDisplay,
                            modifier = Modifier
                                .align(Alignment.Center)
                                .zIndex(OverlayPlaneModal),
                        )
                    } else {
                        MapSelectionTray(
                            state = selection,
                            centerProbeTag = mapSelectionCenterProbeTag,
                            onBoundsChange = { mapSelectionTrayBounds = it },
                            modifier = Modifier
                                .zIndex(OverlayPlaneModal)
                                .align(
                                    when {
                                        selection.point.x < surfaceWidthPx / 2f && selection.point.y < surfaceHeightPx / 2f -> Alignment.BottomEnd
                                        selection.point.x < surfaceWidthPx / 2f -> Alignment.TopEnd
                                        selection.point.y < surfaceHeightPx / 2f -> Alignment.BottomStart
                                        else -> Alignment.TopStart
                                    },
                                )
                                .padding(ThumbGap),
                            onSelectItem = { item ->
                                mapSelection = selection.copy(
                                    selectedItem = item,
                                    detailModal = null,
                                )
                                item.actions
                                    .firstOrNull { it.actionUid == item.automaticActionUid }
                                    ?.let(::performSelectedMapAction)
                            },
                            onSelectAction = { _, action ->
                                if (!action.enabled) {
                                    action.disabledReason
                                        ?.takeIf { it.isNotBlank() }
                                        ?.let { reason ->
                                            Toast.makeText(context, reason, Toast.LENGTH_SHORT).show()
                                    }
                                    return@MapSelectionTray
                                }
                                performSelectedMapAction(action)
                            },
                        )
                    }
                }
            }
        }

        PrimaryNavigationDock(
            currentPage = page,
            navElement = navElement,
            onHomeClick = { actions.onSelectPage(AppPage.Home) },
            onOpenPlan = actions.onOpenPlan,
            onSelectPage = actions.onSelectPage,
            onOpenChartOrPlate = { actions.onSelectPage(AppPage.Charts) },
            modifier = Modifier
                .align(Alignment.BottomCenter)
                .padding(bottom = ThumbGap),
        )

    }
}

@Composable
private fun MapPlaybackWidgetOverlay(
    surfaceWidthDp: Float,
    uiSession: NativeUiSession,
    playbackUiState: PlaybackUiState,
    sourcePath: String,
    onSourcePathChange: (String) -> Unit,
    onSnapshotChange: (UiSessionSnapshot) -> Unit,
    onSessionCommandFailure: (Throwable) -> Unit,
    modifier: Modifier = Modifier,
) {
    val density = LocalDensity.current
    val configuration = LocalConfiguration.current
    var playbackSourceFocused by remember { mutableStateOf(false) }
    val primaryNavigationWidth = (ThumbSize * 5f) + (ThumbGap * 2f)
    val playbackLeftRoomUnits = surfaceWidthDp / 2f - (primaryNavigationWidth.value / 2f) - (ThumbGap.value * 2f)
    val playbackBottomPadding =
        if (playbackLeftRoomUnits < ThumbSize.value * 2.8f) {
            ThumbGap + ThumbSize + ThumbGap
        } else {
            ThumbGap
        }
    val playbackImePadding = with(density) { WindowInsets.ime.getBottom(this).toDp() }
    val playbackKeyboardPadding =
        if (!playbackSourceFocused) {
            0.dp
        } else if (playbackImePadding > 0.dp) {
            playbackImePadding
        } else {
            (configuration.screenHeightDp * 0.38f).dp
        }
    val visiblePlaybackBottomPadding =
        maxOf(playbackBottomPadding, playbackKeyboardPadding + ThumbGap)

    PlaybackWidget(
        uiSession = uiSession,
        playbackUiState = playbackUiState,
        sourcePath = sourcePath,
        onSourcePathChange = onSourcePathChange,
        onSourceFocusChange = { focused -> playbackSourceFocused = focused },
        onSnapshotChange = onSnapshotChange,
        onSessionCommandFailure = onSessionCommandFailure,
        modifier = modifier.padding(start = ThumbGap, bottom = visiblePlaybackBottomPadding),
    )
}

@Composable
private fun RasterImageLayers(
    tiles: List<RenderTile>,
    tileRects: Map<org.aerobag.app.domain.RenderTileKey, TileRect>,
    tileBitmapCache: Map<org.aerobag.app.domain.RenderTileKey, androidx.compose.ui.graphics.ImageBitmap?>,
    tileLabels: Boolean,
    tileLabelPaint: Paint,
    tileLabelBackgroundPaint: Paint,
    nexradFrame: NexradOverlayFrame?,
    terrainOverlay: List<TerrainOverlayImage>,
    viewport: MapViewportState,
    surfaceWidthPx: Float,
    surfaceHeightPx: Float,
    mapUpDeg: Double,
) {
    Canvas(
        modifier = Modifier
            .fillMaxSize()
            .graphicsLayer {
                rotationZ = -mapUpDeg.toFloat()
                clip = false
            },
    ) {
        tiles.forEach { tile ->
            val tileRect = tileRects.getValue(renderTileKey(tile))
            val bitmap = tileBitmapCache[renderTileKey(tile)]
            if (bitmap != null) {
                drawImage(
                    image = bitmap,
                    dstOffset = IntOffset(tileRect.leftPx, tileRect.topPx),
                    dstSize = IntSize(tileRect.widthPx, tileRect.heightPx),
                )
            }
            if (tileLabels) {
                val label = "z${tile.zoom} x${tile.x} y${tile.yTms}"
                val rectLeft = tileRect.leftPx + 6f
                val rectTop = tileRect.topPx + 6f
                val textWidth = tileLabelPaint.measureText(label)
                drawContext.canvas.nativeCanvas.apply {
                    drawRoundRect(
                        rectLeft,
                        rectTop,
                        rectLeft + textWidth + 16f,
                        rectTop + 30f,
                        8f,
                        8f,
                        tileLabelBackgroundPaint,
                    )
                    drawText(label, rectLeft + 8f, tileRect.topPx + 30f, tileLabelPaint)
                }
            }
        }
        if (nexradFrame != null && nexradFrame.images.isNotEmpty() && surfaceWidthPx > 0f && surfaceHeightPx > 0f) {
            val paint = Paint().apply {
                isAntiAlias = true
                isFilterBitmap = true
                alpha = (0.82f * 255f).roundToInt().coerceIn(0, 255)
            }
            val fromFrame = MapDisplayFrame(
                viewport = nexradFrame.viewport,
                widthPx = nexradFrame.surfaceWidthPx,
                heightPx = nexradFrame.surfaceHeightPx,
            )
            val toFrame = MapDisplayFrame(
                viewport = viewport,
                widthPx = surfaceWidthPx,
                heightPx = surfaceHeightPx,
            )
            fun transformNexradPoint(point: NexradOverlayScreenPoint) =
                toFrame.transformScreenPointFrom(
                    from = fromFrame,
                    point = ScreenPoint(point.x.toFloat(), point.y.toFloat()),
                )
            nexradFrame.images.forEach { image ->
                val tile = image.tile
                val bitmap = image.bitmap.asAndroidBitmap()
                val source = floatArrayOf(
                    tile.sourceX.toFloat(),
                    tile.sourceY.toFloat(),
                    (tile.sourceX + tile.sourceWidth).toFloat(),
                    tile.sourceY.toFloat(),
                    (tile.sourceX + tile.sourceWidth).toFloat(),
                    (tile.sourceY + tile.sourceHeight).toFloat(),
                    tile.sourceX.toFloat(),
                    (tile.sourceY + tile.sourceHeight).toFloat(),
                )
                val nw = transformNexradPoint(tile.corners.nw)
                val ne = transformNexradPoint(tile.corners.ne)
                val se = transformNexradPoint(tile.corners.se)
                val sw = transformNexradPoint(tile.corners.sw)
                val destination = floatArrayOf(nw.x, nw.y, ne.x, ne.y, se.x, se.y, sw.x, sw.y)
                val matrix = Matrix().apply {
                    setPolyToPoly(source, 0, destination, 0, 4)
                }
                val clipPath = AndroidPath().apply {
                    moveTo(nw.x, nw.y)
                    lineTo(ne.x, ne.y)
                    lineTo(se.x, se.y)
                    lineTo(sw.x, sw.y)
                    close()
                }
                drawContext.canvas.nativeCanvas.apply {
                    save()
                    clipPath(clipPath)
                    drawBitmap(bitmap, matrix, paint)
                    restore()
                }
            }
        }
        terrainOverlay.forEach { image ->
            val tilesAtZoom = 2.0.pow(image.z.toDouble())
            val tileWorldSize = WebMercatorWorldSize / tilesAtZoom
            val yXyz = (tilesAtZoom - 1.0) - image.yTms.toDouble()
            val scale = scaleForZoom(viewport.zoom)
            val leftPx = ((image.x * tileWorldSize - viewport.centerWorldX) * scale + surfaceWidthPx / 2f).roundToInt()
            val topPx = ((yXyz * tileWorldSize - viewport.centerWorldY) * scale + surfaceHeightPx / 2f).roundToInt()
            val sizePx = (tileWorldSize * scale).roundToInt().coerceAtLeast(1)
            drawImage(
                image = image.bitmap,
                dstOffset = IntOffset(leftPx, topPx),
                dstSize = IntSize(sizePx, sizePx),
                alpha = 0.68f,
            )
        }
    }
}

@Composable
private fun AirspaceOverlayLayer(
    displayedMapOverlay: MapOverlayQueryResult,
    densityScale: Float,
    uiTheme: UiTheme,
) {
    if (displayedMapOverlay.airspacePaths.isEmpty() && displayedMapOverlay.tfrPaths.isEmpty() && displayedMapOverlay.airspaceLabels.isEmpty()) {
        return
    }
    Canvas(modifier = Modifier.fillMaxSize()) {
        (displayedMapOverlay.airspacePaths + displayedMapOverlay.tfrPaths).forEach { feature ->
            drawAirspaceDisplayPath(uiTheme, feature, densityScale)
        }
        displayedMapOverlay.airspaceLabels.forEach { label ->
            drawAirspaceLimitGlyph(
                uiTheme = uiTheme,
                glyph = label.glyph,
                center = Offset(label.screenX.toFloat(), label.screenY.toFloat()),
                scale = 1f,
            )
        }
    }
}

@Composable
private fun MapFeatureOverlayLayer(
    displayedMapOverlay: MapOverlayQueryResult,
    uiTheme: UiTheme,
    densityScale: Float,
    fixMarkerStrokeColor: Color,
    fixMarkerFillColor: Color,
    airportMarkerStrokeColor: Color,
    airportToweredFillColor: Color,
    airportUntoweredFillColor: Color,
    vorMarkerColor: Color,
    vorMarkerStrokeColor: Color,
    fixLabelStrokePaint: Paint,
    airportLabelStrokePaint: Paint,
    vorLabelFillPaint: Paint,
    fixLabelFillPaint: Paint,
    airportToweredLabelFillPaint: Paint,
    airportUntoweredLabelFillPaint: Paint,
    flightPlanOnly: Boolean = false,
    mapUpDeg: Double,
) {
    val features = if (flightPlanOnly) displayedMapOverlay.flightPlanFeatures else displayedMapOverlay.visibleFeatures
    if (features.isEmpty()) return
    Canvas(modifier = Modifier.fillMaxSize()) {
        fixLabelStrokePaint.textSize = 14f * densityScale
        fixLabelStrokePaint.strokeWidth = 3f * densityScale
        airportLabelStrokePaint.textSize = 14f * densityScale
        airportLabelStrokePaint.strokeWidth = 3f * densityScale
        fixLabelFillPaint.textSize = 14f * densityScale
        airportToweredLabelFillPaint.textSize = 14f * densityScale
        airportUntoweredLabelFillPaint.textSize = 14f * densityScale
        vorLabelFillPaint.textSize = 14f * densityScale
        features.forEach { feature ->
            drawVisibleMapFeature(
                feature = feature,
                densityScale = densityScale,
                uiTheme = uiTheme,
                fixMarkerStrokeColor = fixMarkerStrokeColor,
                fixMarkerFillColor = fixMarkerFillColor,
                airportMarkerStrokeColor = airportMarkerStrokeColor,
                airportToweredFillColor = airportToweredFillColor,
                airportUntoweredFillColor = airportUntoweredFillColor,
                vorMarkerColor = vorMarkerColor,
                vorMarkerStrokeColor = vorMarkerStrokeColor,
                fixLabelStrokePaint = fixLabelStrokePaint,
                airportLabelStrokePaint = airportLabelStrokePaint,
                vorLabelFillPaint = vorLabelFillPaint,
                fixLabelFillPaint = fixLabelFillPaint,
                airportToweredLabelFillPaint = airportToweredLabelFillPaint,
                airportUntoweredLabelFillPaint = airportUntoweredLabelFillPaint,
                mapUpDeg = mapUpDeg,
            )
        }
    }
    features.forEach { feature ->
        val tagLabel = feature.label.trim().takeIf { it.isNotEmpty() } ?: return@forEach
        Box(
            modifier = Modifier
                .offset {
                    val targetSizePx = (ThumbSize * 0.5f).roundToPx()
                    IntOffset(
                        x = feature.screenX.toFloat().roundToInt() - targetSizePx / 2,
                        y = feature.screenY.toFloat().roundToInt() - targetSizePx / 2,
                    )
                }
                .size(ThumbSize * 0.5f)
                .testTag("parity:map-feature:${feature.kind}:$tagLabel:${feature.id}"),
        )
    }
}

private fun androidx.compose.ui.graphics.drawscope.DrawScope.drawVisibleMapFeature(
    feature: VisibleMapFeature,
    densityScale: Float,
    uiTheme: UiTheme,
    fixMarkerStrokeColor: Color,
    fixMarkerFillColor: Color,
    airportMarkerStrokeColor: Color,
    airportToweredFillColor: Color,
    airportUntoweredFillColor: Color,
    vorMarkerColor: Color,
    vorMarkerStrokeColor: Color,
    fixLabelStrokePaint: Paint,
    airportLabelStrokePaint: Paint,
    vorLabelFillPaint: Paint,
    fixLabelFillPaint: Paint,
    airportToweredLabelFillPaint: Paint,
    airportUntoweredLabelFillPaint: Paint,
    contrastOnly: Boolean = false,
    drawLabel: Boolean = true,
    selectedLabel: Boolean = false,
    labelOverride: String? = null,
    mapUpDeg: Double,
) {
    val center = Offset(feature.screenX.toFloat(), feature.screenY.toFloat())
    val contrastColor = Color.White
    val contrastStrokeWidth = 8f * densityScale
    if (feature.symbolKind == "weather_camera") {
        val label = labelOverride ?: feature.label
        weatherCameraSymbol(center, densityScale).forEach { layer ->
            if (contrastOnly) {
                drawNavSymbolLayerAsContrast(layer, densityScale, contrastColor, contrastStrokeWidth)
            } else {
                drawNavSymbolLayer(layer, densityScale, uiTheme)
            }
        }
        if (!contrastOnly && drawLabel) {
            if (selectedLabel) {
                drawSelectedVectorIdentLabel(label, center.x, center.y - 24f * densityScale, densityScale)
            } else {
                drawVectorIdentLabel(
                    label = label,
                    centerX = center.x,
                    baselineY = center.y - 24f * densityScale,
                    strokePaint = fixLabelStrokePaint,
                    fillPaint = fixLabelFillPaint,
                    labelStyle = feature.labelStyle,
                    densityScale = densityScale,
                    uiTheme = uiTheme,
                )
            }
        }
    } else if (feature.symbolKind == "airport") {
        val label = labelOverride ?: feature.label
        val airportFillColor = if (feature.towered) airportToweredFillColor else airportUntoweredFillColor
        val airportLabelPaint = if (feature.towered) airportToweredLabelFillPaint else airportUntoweredLabelFillPaint
        val usesOpenAirportCircle =
            feature.heliport == true ||
                feature.hasWaterRunway == true ||
                feature.hasPavedRunway == false
        if (usesOpenAirportCircle) {
            airportOpenMarkerSymbol(center, densityScale).forEach { layer ->
                if (contrastOnly) {
                    drawNavSymbolLayerAsContrast(layer, densityScale, contrastColor, contrastStrokeWidth)
                } else {
                    drawNavSymbolLayer(layer, densityScale, uiTheme)
                }
            }
        } else if (feature.fuelAvailable) {
            val markerPath = airportFuelMarkerPath(center, densityScale)
            if (contrastOnly) {
                drawPath(markerPath, contrastColor, style = Stroke(width = contrastStrokeWidth, join = StrokeJoin.Round))
            } else {
                drawPath(markerPath, airportFillColor)
                drawPath(markerPath, airportMarkerStrokeColor, style = Stroke(width = 2f * densityScale))
            }
        } else {
            val markerPath = airportCircleMarkerPath(center, densityScale)
            if (contrastOnly) {
                drawPath(markerPath, contrastColor, style = Stroke(width = contrastStrokeWidth, join = StrokeJoin.Round))
            } else {
                drawPath(markerPath, airportFillColor)
                drawPath(markerPath, airportMarkerStrokeColor, style = Stroke(width = 2f * densityScale))
            }
        }
        if (feature.heliport == true) {
            val heliportPath = heliportHPath(center, densityScale)
            drawPath(
                heliportPath,
                if (contrastOnly) contrastColor else airportUntoweredFillColor,
                style = Stroke(width = if (contrastOnly) contrastStrokeWidth else 2.4f * densityScale, cap = StrokeCap.Round),
            )
        } else if (feature.hasWaterRunway == true) {
            rotate(15f, center) {
                val anchorPath = seaplaneAnchorPath(center, densityScale)
                drawPath(
                    anchorPath,
                    if (contrastOnly) contrastColor else airportUntoweredFillColor,
                    style = Stroke(width = if (contrastOnly) contrastStrokeWidth else 2.2f * densityScale, cap = StrokeCap.Round),
                )
            }
        }
        if (!usesOpenAirportCircle) feature.longestRunwayHeadingTrueDeg?.let { headingDeg ->
            if (contrastOnly) {
                return@let
            }
            val headingRad = Math.toRadians(headingDeg - mapUpDeg)
            val runwayHalfLength = (8f * feature.runwayLengthRatio.toFloat().coerceIn(0f, 1f)).coerceAtLeast(1.6f) * densityScale
            val dx = kotlin.math.sin(headingRad).toFloat() * runwayHalfLength
            val dy = (-kotlin.math.cos(headingRad)).toFloat() * runwayHalfLength
            drawLine(
                color = airportMarkerStrokeColor,
                start = Offset(center.x - dx, center.y - dy),
                end = Offset(center.x + dx, center.y + dy),
                strokeWidth = 5f * densityScale,
                cap = StrokeCap.Round,
            )
            drawLine(
                color = Color.White,
                start = Offset(center.x - dx, center.y - dy),
                end = Offset(center.x + dx, center.y + dy),
                strokeWidth = 3f * densityScale,
                cap = StrokeCap.Round,
            )
        }
        if (!contrastOnly && drawLabel) {
            if (selectedLabel) {
                drawSelectedVectorIdentLabel(label, center.x, center.y - 24f * densityScale, densityScale)
            } else {
                drawVectorIdentLabel(
                    label = label,
                    centerX = center.x,
                    baselineY = center.y - 24f * densityScale,
                    strokePaint = airportLabelStrokePaint,
                    fillPaint = airportLabelPaint,
                    labelStyle = feature.labelStyle,
                    densityScale = densityScale,
                    uiTheme = uiTheme,
                )
            }
        }
    } else if (feature.symbolKind == "nav") {
        val label = labelOverride ?: feature.label
        val radius = 8f * densityScale
        val outerHex = vorOuterHexPath(center, radius)
        val band = vorBandPath(center, radius)
        if (contrastOnly) {
            drawPath(band, contrastColor, style = Stroke(width = contrastStrokeWidth, join = StrokeJoin.Round))
            drawPath(outerHex, contrastColor, style = Stroke(width = contrastStrokeWidth, join = StrokeJoin.Round))
        } else {
            drawPath(band, vorMarkerColor)
            drawPath(band, vorMarkerStrokeColor, style = Stroke(width = 1.6f * densityScale))
            drawPath(outerHex, vorMarkerStrokeColor, style = Stroke(width = 1.6f * densityScale))
            if (drawLabel) {
                if (selectedLabel) {
                    drawSelectedVectorIdentLabel(label, center.x, center.y - 24f * densityScale, densityScale)
                } else {
                    drawVectorIdentLabel(
                        label = label,
                        centerX = center.x,
                        baselineY = center.y - 24f * densityScale,
                        strokePaint = fixLabelStrokePaint,
                        fillPaint = vorLabelFillPaint,
                        labelStyle = feature.labelStyle,
                        densityScale = densityScale,
                        uiTheme = uiTheme,
                    )
                }
            }
        }
    } else if (feature.symbolKind == "obstacle") {
        val label = labelOverride ?: feature.label
        val isTallObstacle = feature.obstacleVariant == "tall"
        val obstaclePath = if (isTallObstacle) {
            obstacleTallPath(center, densityScale)
        } else {
            obstacleShortPath(center, densityScale)
        }
        val dotY = if (isTallObstacle) obstacleTallDotY else obstacleShortDotY
        val obstacleColor = obstacleToneColor(uiTheme, feature.obstacleTone)
        val obstacleUnderColor = uiTheme.aviation.obstacleUnder
        if (contrastOnly) {
            drawPath(
                obstaclePath,
                contrastColor,
                style = Stroke(width = contrastStrokeWidth, join = StrokeJoin.Miter),
            )
            drawCircle(
                color = contrastColor,
                radius = obstacleDotRadius * densityScale + contrastStrokeWidth * 0.35f,
                center = Offset(center.x, center.y + dotY * densityScale),
            )
        } else {
            drawPath(
                obstaclePath,
                obstacleUnderColor,
                style = Stroke(width = 2.4f * densityScale, join = StrokeJoin.Miter),
            )
            drawPath(
                obstaclePath,
                obstacleColor,
                style = Stroke(width = 1.2f * densityScale, join = StrokeJoin.Miter),
            )
            drawCircle(
                color = obstacleUnderColor,
                radius = obstacleDotRadius * densityScale,
                center = Offset(center.x, center.y + dotY * densityScale),
            )
            drawCircle(
                color = obstacleColor,
                radius = obstacleDotRadius * densityScale,
                center = Offset(center.x, center.y + dotY * densityScale),
            )
        }
        if (label.isNotEmpty()) {
            if (!contrastOnly && drawLabel) {
                if (selectedLabel) {
                    drawSelectedVectorIdentLabel(label, center.x, center.y - 14f * densityScale, densityScale)
                } else {
                    drawVectorIdentLabel(
                        label = label,
                        centerX = center.x,
                        baselineY = center.y - 14f * densityScale,
                        strokePaint = fixLabelStrokePaint,
                        fillPaint = fixLabelFillPaint,
                        labelStyle = feature.labelStyle,
                        densityScale = densityScale,
                        uiTheme = uiTheme,
                    )
                }
            }
        }
    } else {
        val label = labelOverride ?: feature.label
        val triangle = fixTrianglePath(center, 8f * densityScale)
        if (contrastOnly) {
            drawPath(triangle, contrastColor, style = Stroke(width = contrastStrokeWidth, join = StrokeJoin.Round))
        } else {
            drawPath(triangle, fixMarkerFillColor)
            drawPath(triangle, fixMarkerStrokeColor, style = Stroke(width = 2.5f * densityScale))
            if (drawLabel) {
                if (selectedLabel) {
                    drawSelectedVectorIdentLabel(label, center.x, center.y - 15f * densityScale, densityScale)
                } else {
                    drawVectorIdentLabel(
                        label = label,
                        centerX = center.x,
                        baselineY = center.y - 15f * densityScale,
                        strokePaint = fixLabelStrokePaint,
                        fillPaint = fixLabelFillPaint,
                        labelStyle = feature.labelStyle,
                        densityScale = densityScale,
                        uiTheme = uiTheme,
                    )
                }
            }
        }
    }
}

private fun androidx.compose.ui.graphics.drawscope.DrawScope.drawSelectedVectorIdentLabel(
    label: String,
    centerX: Float,
    baselineY: Float,
    densityScale: Float,
) {
    if (label.isBlank()) return
    val fillPaint = Paint(Paint.ANTI_ALIAS_FLAG).apply {
        color = android.graphics.Color.rgb(8, 18, 24)
        textAlign = Paint.Align.CENTER
        textSize = 14f * densityScale
        typeface = Typeface.create(Typeface.DEFAULT, Typeface.BOLD)
        style = Paint.Style.FILL
    }
    val boxFillPaint = Paint(Paint.ANTI_ALIAS_FLAG).apply {
        color = android.graphics.Color.WHITE
        style = Paint.Style.FILL
    }
    val boxStrokePaint = Paint(Paint.ANTI_ALIAS_FLAG).apply {
        color = android.graphics.Color.rgb(8, 18, 24)
        style = Paint.Style.STROKE
        strokeWidth = 2f * densityScale
    }
    val width = kotlin.math.max(26f * densityScale, fillPaint.measureText(label) + 14f * densityScale)
    val height = 15f * densityScale
    val rect = RectF(
        centerX - width / 2f,
        baselineY - height + 2f * densityScale,
        centerX + width / 2f,
        baselineY + 2f * densityScale,
    )
    drawContext.canvas.nativeCanvas.apply {
        drawRoundRect(rect, 2f * densityScale, 2f * densityScale, boxFillPaint)
        drawRoundRect(rect, 2f * densityScale, 2f * densityScale, boxStrokePaint)
        drawText(label, centerX, baselineY, fillPaint)
    }
}

private fun androidx.compose.ui.graphics.drawscope.DrawScope.drawVectorIdentLabel(
    label: String,
    centerX: Float,
    baselineY: Float,
    strokePaint: Paint,
    fillPaint: Paint,
    labelStyle: String,
    densityScale: Float,
    uiTheme: UiTheme,
) {
    if (label.isBlank()) return
    drawContext.canvas.nativeCanvas.apply {
        if (labelStyle == "default") {
            drawText(label, centerX, baselineY, strokePaint)
            drawText(label, centerX, baselineY, fillPaint)
            return
        }
        val active = labelStyle == "active_flight_plan"
        val textPaint = Paint(fillPaint).apply {
            color = if (active) uiTheme.aviation.classCMagenta.toArgb() else android.graphics.Color.rgb(8, 18, 24)
            style = Paint.Style.FILL
        }
        val boxFillPaint = Paint().apply {
            isAntiAlias = true
            color = if (active) android.graphics.Color.rgb(8, 18, 24) else android.graphics.Color.WHITE
            style = Paint.Style.FILL
        }
        val boxStrokePaint = Paint().apply {
            isAntiAlias = true
            color = if (active) android.graphics.Color.WHITE else android.graphics.Color.rgb(8, 18, 24)
            style = Paint.Style.STROKE
            strokeWidth = 2f * densityScale
        }
        val width = kotlin.math.max(26f * densityScale, textPaint.measureText(label) + 14f * densityScale)
        val height = 15f * densityScale
        val rect = RectF(
            centerX - width / 2f,
            baselineY - height + 2f * densityScale,
            centerX + width / 2f,
            baselineY + 2f * densityScale,
        )
        drawRoundRect(rect, 2f * densityScale, 2f * densityScale, boxFillPaint)
        drawRoundRect(rect, 2f * densityScale, 2f * densityScale, boxStrokePaint)
        drawText(label, centerX, baselineY, textPaint)
    }
}

@Composable
private fun ObservationOverlayLayer(
    displayedMapOverlay: MapOverlayQueryResult,
    densityScale: Float,
    uiTheme: UiTheme,
) {
    if (displayedMapOverlay.visibleMetars.isNotEmpty()) {
        Canvas(modifier = Modifier.fillMaxSize()) {
            displayedMapOverlay.visibleMetars.forEach { feature ->
                drawMetarSymbol(feature, Offset(feature.screenX.toFloat(), feature.screenY.toFloat()), densityScale, uiTheme)
            }
        }
    }
    if (displayedMapOverlay.visiblePireps.isNotEmpty()) {
        Canvas(modifier = Modifier.fillMaxSize()) {
            displayedMapOverlay.visiblePireps.forEach { feature ->
                drawPirepSymbol(feature, Offset(feature.screenX.toFloat(), feature.screenY.toFloat()), densityScale, uiTheme, symbolScale = 0.32f)
            }
        }
    }
}

@Composable
private fun TrafficOverlayLayer(
    displayedMapOverlay: MapOverlayQueryResult,
    densityScale: Float,
    uiTheme: UiTheme,
    mapUpDeg: Double,
) {
    if (displayedMapOverlay.visibleTraffic.isEmpty()) return
    Canvas(modifier = Modifier.fillMaxSize()) {
        displayedMapOverlay.visibleTraffic.forEach { feature ->
            drawAdsbTraffic(feature, densityScale, uiTheme, mapUpDeg, selected = false)
        }
    }
}

private fun DrawScope.drawAdsbTraffic(
    feature: VisibleAdsbTraffic,
    densityScale: Float,
    uiTheme: UiTheme,
    mapUpDeg: Double,
    selected: Boolean,
) {
    val center = Offset(feature.screenX.toFloat(), feature.screenY.toFloat())
    val symbol = Path().apply {
        moveTo(center.x, center.y - 11f * densityScale)
        lineTo(center.x + 8f * densityScale, center.y + 9f * densityScale)
        lineTo(center.x, center.y + 5f * densityScale)
        lineTo(center.x - 8f * densityScale, center.y + 9f * densityScale)
        close()
    }
    rotate(((feature.trackDegTrue ?: 0.0) - mapUpDeg).toFloat(), center) {
        if (selected) {
            drawPath(
                symbol,
                Color.White,
                style = Stroke(width = 10f * densityScale, join = StrokeJoin.Round),
            )
        }
        drawPath(
            symbol,
            uiTheme.aviation.trafficContrast,
            style = Stroke(width = 5f * densityScale, join = StrokeJoin.Round),
        )
        drawPath(symbol, uiTheme.aviation.traffic)
        drawPath(
            symbol,
            uiTheme.aviation.trafficContrast,
            style = Stroke(width = 1.25f * densityScale, join = StrokeJoin.Round),
        )
    }
    val labelStroke = Paint(Paint.ANTI_ALIAS_FLAG).apply {
        color = if (selected) android.graphics.Color.WHITE else uiTheme.aviation.trafficContrast.toArgb()
        textSize = 12f * densityScale
        typeface = Typeface.create(Typeface.DEFAULT, Typeface.BOLD)
        style = Paint.Style.STROKE
        strokeWidth = (if (selected) 7f else 3f) * densityScale
    }
    val regularLabelStroke = Paint(labelStroke).apply {
        color = uiTheme.aviation.trafficContrast.toArgb()
        strokeWidth = 3f * densityScale
    }
    val labelFill = Paint(labelStroke).apply {
        color = uiTheme.aviation.trafficLabel.toArgb()
        style = Paint.Style.FILL
    }
    val x = center.x + 13f * densityScale
    val firstBaseline = center.y - 2f * densityScale
    drawContext.canvas.nativeCanvas.apply {
        if (selected) drawText(feature.label, x, firstBaseline, labelStroke)
        drawText(feature.label, x, firstBaseline, regularLabelStroke)
        drawText(feature.label, x, firstBaseline, labelFill)
        val secondBaseline = firstBaseline + 13.2f * densityScale
        if (selected) drawText(feature.detailLabel, x, secondBaseline, labelStroke)
        drawText(feature.detailLabel, x, secondBaseline, regularLabelStroke)
        drawText(feature.detailLabel, x, secondBaseline, labelFill)
    }
}

@Composable
private fun OfflineRegionsOverlayLayer(
    displayedMapOverlay: MapOverlayQueryResult,
    densityScale: Float,
    uiTheme: UiTheme,
) {
    if (displayedMapOverlay.offlineRegions.isEmpty()) return
    Canvas(modifier = Modifier.fillMaxSize()) {
        val labelStroke = Paint(Paint.ANTI_ALIAS_FLAG).apply {
            color = android.graphics.Color.argb(190, 0, 0, 0)
            textAlign = Paint.Align.CENTER
            textSize = 13f * densityScale
            typeface = Typeface.create(Typeface.DEFAULT, Typeface.BOLD)
            style = Paint.Style.STROKE
            strokeWidth = 4f * densityScale
        }
        val labelFill = Paint(Paint.ANTI_ALIAS_FLAG).apply {
            textAlign = Paint.Align.CENTER
            textSize = 13f * densityScale
            typeface = Typeface.create(Typeface.DEFAULT, Typeface.BOLD)
            style = Paint.Style.FILL
        }
        displayedMapOverlay.offlineRegions.forEach { region ->
            val color = aviationColor(uiTheme, region.colorKey)
            drawOfflineRegion(region, densityScale, uiTheme, selected = false)
            labelFill.color = color.toArgb()
            drawContext.canvas.nativeCanvas.apply {
                val x = region.labelX.toFloat()
                val y = region.labelY.toFloat() + labelFill.textSize * 0.33f
                drawText(region.label, x, y, labelStroke)
                drawText(region.label, x, y, labelFill)
            }
        }
    }
}

private fun androidx.compose.ui.graphics.drawscope.DrawScope.drawOfflineRegion(
    region: org.aerobag.app.domain.OfflineRegionDisplay,
    densityScale: Float,
    uiTheme: UiTheme,
    selected: Boolean,
) {
    val path = Path().apply {
        val first = region.points.firstOrNull() ?: return
        moveTo(first.x.toFloat(), first.y.toFloat())
        region.points.drop(1).forEach { point -> lineTo(point.x.toFloat(), point.y.toFloat()) }
        close()
    }
    val color = aviationColor(uiTheme, region.colorKey)
    drawPath(
        path,
        Color.White.copy(alpha = if (selected) 0.95f else 0.8f),
        style = Stroke(width = (if (selected) 8f else 5f) * densityScale, join = StrokeJoin.Round),
    )
    drawPath(
        path,
        color,
        style = Stroke(width = (if (selected) 4f else 2.5f) * densityScale, join = StrokeJoin.Round),
    )
}

@Composable
private fun RouteOverlayLayer(
    flightPlanRoute: List<FlightPlanRouteSegment>,
    distanceAnnotations: List<FlightPlanRouteDistanceAnnotation>,
    visibleFeatureIds: Set<String>,
    viewport: MapViewportState,
    surfaceWidthPx: Float,
    surfaceHeightPx: Float,
    densityScale: Float,
    uiTheme: UiTheme,
) {
    if (flightPlanRoute.isEmpty() || surfaceWidthPx <= 0f || surfaceHeightPx <= 0f) return
    val distanceTextPaint = remember(densityScale, uiTheme.flightPlanRoute.distancePillFg) {
        Paint().apply {
            isAntiAlias = true
            color = uiTheme.flightPlanRoute.distancePillFg.toArgb()
            textAlign = Paint.Align.CENTER
            textSize = 12f * densityScale
            typeface = Typeface.DEFAULT_BOLD
        }
    }
    val distanceBackgroundPaint = remember(uiTheme.flightPlanRoute.distancePillBg) {
        Paint().apply {
            isAntiAlias = true
            color = uiTheme.flightPlanRoute.distancePillBg.toArgb()
            style = Paint.Style.FILL
        }
    }
    val distanceBorderPaint = remember(densityScale) {
        Paint().apply {
            isAntiAlias = true
            style = Paint.Style.STROKE
            strokeWidth = 2f * densityScale
        }
    }
    val distanceContrastPaint = remember(densityScale, uiTheme.flightPlanRoute.contrast) {
        Paint().apply {
            isAntiAlias = true
            color = uiTheme.flightPlanRoute.contrast.toArgb()
            style = Paint.Style.STROKE
            strokeWidth = 6f * densityScale
        }
    }
    val projectionState = remember(flightPlanRoute, viewport, surfaceWidthPx, surfaceHeightPx) {
        flightPlanRouteOverlayProjectionState(
            flightPlanRoute = flightPlanRoute,
            viewport = viewport,
            surfaceWidthPx = surfaceWidthPx,
            surfaceHeightPx = surfaceHeightPx,
        )
    }
    Canvas(
        modifier = Modifier
            .fillMaxSize()
            .testTag("parity:flight-plan-route-overlay:$projectionState"),
    ) {
        val screenPaths = flightPlanRoute.map { segment ->
            segment.path.ifEmpty { listOf(segment.from, segment.to) }.map { point ->
                latLonToScreen(point.lat, point.lon, viewport, surfaceWidthPx, surfaceHeightPx)
            }
        }
        flightPlanRoute.forEachIndexed { index, segment ->
            drawFlightPlanRoutePath(
                screenPath = screenPaths[index],
                style = segment.style,
                color = routeSegmentColor(uiTheme, segment.status),
                contrastColor = uiTheme.flightPlanRoute.contrast,
                densityScale = densityScale,
                layer = RouteRenderLayer.Contrast,
            )
        }
        val distanceLayouts = layoutRouteDistancePills(
            annotations = distanceAnnotations,
            screenPaths = screenPaths,
            visibleFeatureIds = visibleFeatureIds,
            measurePillWidth = { text ->
                maxOf(26f * densityScale, distanceTextPaint.measureText(text) + 12f * densityScale)
            },
        )
        val pillHeight = 20f * densityScale
        val pillRects = distanceLayouts.map { layout ->
            layout to RectF(
                layout.center.x - layout.widthPx / 2f,
                layout.center.y - pillHeight / 2f,
                layout.center.x + layout.widthPx / 2f,
                layout.center.y + pillHeight / 2f,
            )
        }
        pillRects.forEach { (layout, rect) ->
            drawContext.canvas.nativeCanvas.apply {
                save()
                rotate(layout.rotationDegrees, layout.center.x, layout.center.y)
                drawRoundRect(rect, pillHeight / 2f, pillHeight / 2f, distanceContrastPaint)
                restore()
            }
        }
        flightPlanRoute.forEachIndexed { index, segment ->
            drawFlightPlanRoutePath(
                screenPath = screenPaths[index],
                style = segment.style,
                color = routeSegmentColor(uiTheme, segment.status),
                contrastColor = uiTheme.flightPlanRoute.contrast,
                densityScale = densityScale,
                layer = RouteRenderLayer.Color,
            )
        }
        pillRects.forEach { (layout, rect) ->
            drawContext.canvas.nativeCanvas.apply {
                save()
                rotate(layout.rotationDegrees, layout.center.x, layout.center.y)
                drawRoundRect(rect, pillHeight / 2f, pillHeight / 2f, distanceBackgroundPaint)
                restore()
            }
        }
        pillRects.forEach { (layout, rect) ->
            distanceBorderPaint.color = routeSegmentColor(uiTheme, layout.annotation.status).toArgb()
            val baseline = layout.center.y -
                (distanceTextPaint.fontMetrics.ascent + distanceTextPaint.fontMetrics.descent) / 2f
            drawContext.canvas.nativeCanvas.apply {
                save()
                rotate(layout.rotationDegrees, layout.center.x, layout.center.y)
                drawRoundRect(rect, pillHeight / 2f, pillHeight / 2f, distanceBorderPaint)
                drawText(layout.annotation.text, layout.center.x, baseline, distanceTextPaint)
                restore()
            }
        }
    }
}

private fun flightPlanRouteOverlayProjectionState(
    flightPlanRoute: List<FlightPlanRouteSegment>,
    viewport: MapViewportState,
    surfaceWidthPx: Float,
    surfaceHeightPx: Float,
): String =
    "segments:${flightPlanRoute.size}:visible:" +
        countVisibleRouteSegments(
            flightPlanRoute = flightPlanRoute,
            viewport = viewport,
            surfaceWidthPx = surfaceWidthPx,
            surfaceHeightPx = surfaceHeightPx,
        )

private fun countVisibleRouteSegments(
    flightPlanRoute: List<FlightPlanRouteSegment>,
    viewport: MapViewportState,
    surfaceWidthPx: Float,
    surfaceHeightPx: Float,
): Int =
    flightPlanRoute.count { segment ->
        val screenPath = segment.path.ifEmpty { listOf(segment.from, segment.to) }.map { point ->
            latLonToScreen(point.lat, point.lon, viewport, surfaceWidthPx, surfaceHeightPx)
        }
        if (screenPath.size < 2) {
            return@count false
        }
        val minX = screenPath.minOf { it.x }
        val maxX = screenPath.maxOf { it.x }
        val minY = screenPath.minOf { it.y }
        val maxY = screenPath.maxOf { it.y }
        val marginPx = 24f
        maxX >= -marginPx &&
            minX <= surfaceWidthPx + marginPx &&
            maxY >= -marginPx &&
            minY <= surfaceHeightPx + marginPx
    }

@Composable
private fun MapSelectionHighlightLayer(
    selectedItem: MapSelectionItem?,
    displayedMapOverlay: MapOverlayQueryResult,
    viewport: MapViewportState,
    surfaceWidthPx: Float,
    surfaceHeightPx: Float,
    densityScale: Float,
    uiTheme: UiTheme,
    mapUpDeg: Double,
) {
    val item = selectedItem ?: return
    Canvas(modifier = Modifier.fillMaxSize()) {
        when (val highlight = item.highlight) {
            is MapSelectionHighlight.FeatureRef -> {
                val feature = displayedMapOverlay.visibleFeatures.firstOrNull { it.id == highlight.id }
                if (feature != null) {
                    drawVisibleMapFeature(
                        feature = feature,
                        densityScale = densityScale,
                        uiTheme = uiTheme,
                        fixMarkerStrokeColor = Color.Transparent,
                        fixMarkerFillColor = Color.Transparent,
                        airportMarkerStrokeColor = Color.Transparent,
                        airportToweredFillColor = Color.Transparent,
                        airportUntoweredFillColor = Color.Transparent,
                        vorMarkerColor = Color.Transparent,
                        vorMarkerStrokeColor = Color.Transparent,
                        fixLabelStrokePaint = Paint(),
                        airportLabelStrokePaint = Paint(),
                        vorLabelFillPaint = Paint(),
                        fixLabelFillPaint = Paint(),
                        airportToweredLabelFillPaint = Paint(),
                        airportUntoweredLabelFillPaint = Paint(),
                        contrastOnly = true,
                        mapUpDeg = mapUpDeg,
                    )
                    drawVisibleMapFeature(
                        feature = feature,
                        densityScale = densityScale,
                        uiTheme = uiTheme,
                        fixMarkerStrokeColor = Color(0xCC06121A),
                        fixMarkerFillColor = uiTheme.aviation.intersectionCyan,
                        airportMarkerStrokeColor = Color(0xCC06121A),
                        airportToweredFillColor = Color(0xFF0F4C81),
                        airportUntoweredFillColor = uiTheme.aviation.classCMagenta,
                        vorMarkerColor = uiTheme.aviation.classBDBlue,
                        vorMarkerStrokeColor = Color(0xCC06121A),
                        fixLabelStrokePaint = Paint(Paint.ANTI_ALIAS_FLAG).apply {
                            color = android.graphics.Color.argb(205, 0, 0, 0)
                            textAlign = Paint.Align.CENTER
                            typeface = Typeface.create(Typeface.DEFAULT, Typeface.BOLD)
                            style = Paint.Style.STROKE
                        },
                        airportLabelStrokePaint = Paint(Paint.ANTI_ALIAS_FLAG).apply {
                            color = android.graphics.Color.argb(205, 0, 0, 0)
                            textAlign = Paint.Align.CENTER
                            typeface = Typeface.create(Typeface.DEFAULT, Typeface.BOLD)
                            style = Paint.Style.STROKE
                        },
                        vorLabelFillPaint = Paint(Paint.ANTI_ALIAS_FLAG).apply {
                            color = uiTheme.aviation.classBDBlue.toArgb()
                            textAlign = Paint.Align.CENTER
                            typeface = Typeface.create(Typeface.DEFAULT, Typeface.BOLD)
                            style = Paint.Style.FILL
                        },
                        fixLabelFillPaint = Paint(Paint.ANTI_ALIAS_FLAG).apply {
                            color = uiTheme.aviation.intersectionCyan.toArgb()
                            textAlign = Paint.Align.CENTER
                            typeface = Typeface.create(Typeface.DEFAULT, Typeface.BOLD)
                            style = Paint.Style.FILL
                        },
                        airportToweredLabelFillPaint = Paint(Paint.ANTI_ALIAS_FLAG).apply {
                            color = uiTheme.aviation.classBDBlue.toArgb()
                            textAlign = Paint.Align.CENTER
                            typeface = Typeface.create(Typeface.DEFAULT, Typeface.BOLD)
                            style = Paint.Style.FILL
                        },
                        airportUntoweredLabelFillPaint = Paint(Paint.ANTI_ALIAS_FLAG).apply {
                            color = uiTheme.aviation.classCMagenta.toArgb()
                            textAlign = Paint.Align.CENTER
                            typeface = Typeface.create(Typeface.DEFAULT, Typeface.BOLD)
                            style = Paint.Style.FILL
                        },
                        selectedLabel = true,
                        labelOverride = item.label,
                        mapUpDeg = mapUpDeg,
                    )
                }
                (displayedMapOverlay.airspacePaths + displayedMapOverlay.tfrPaths).firstOrNull { it.id == highlight.id }?.let { path ->
                    drawAirspaceDisplayPathContrast(path, densityScale)
                    drawAirspaceDisplayPath(uiTheme, path, densityScale)
                }
            }
            is MapSelectionHighlight.Metar -> {
                val feature = displayedMapOverlay.visibleMetars.firstOrNull { it.stationId == highlight.stationId } ?: item.metarFeature
                if (feature != null) {
                    drawCircle(Color.White, radius = 16f * densityScale, center = Offset(feature.screenX.toFloat(), feature.screenY.toFloat()), style = Stroke(width = 4f * densityScale))
                    drawMetarSymbol(feature, Offset(feature.screenX.toFloat(), feature.screenY.toFloat()), densityScale, uiTheme)
                }
            }
            is MapSelectionHighlight.Pirep -> {
                val feature = displayedMapOverlay.visiblePireps.firstOrNull { it.id == highlight.id } ?: item.pirepFeature
                if (feature != null) {
                    val center = Offset(feature.screenX.toFloat(), feature.screenY.toFloat())
                    drawCircle(Color.White, radius = 25f * densityScale, center = center, style = Stroke(width = 4f * densityScale))
                    drawPirepSymbol(feature, center, densityScale, uiTheme, symbolScale = 0.32f)
                }
            }
            is MapSelectionHighlight.AdsbTraffic -> {
                displayedMapOverlay.visibleTraffic.firstOrNull { it.id == highlight.id }?.let { traffic ->
                    drawAdsbTraffic(traffic, densityScale, uiTheme, mapUpDeg, selected = true)
                }
            }
            is MapSelectionHighlight.OfflineRegion -> {
                displayedMapOverlay.offlineRegions.firstOrNull { it.id == highlight.id }?.let { region ->
                    drawOfflineRegion(region, densityScale, uiTheme, selected = true)
                }
            }
            is MapSelectionHighlight.Spot -> {
                val point = latLonToScreen(highlight.lat, highlight.lon, viewport, surfaceWidthPx, surfaceHeightPx)
                drawMapSelectionSpotSymbol(point, densityScale, uiTheme)
            }
        }
    }
}

@Composable
private fun SituationOverlayLayer(
    situationOverlay: SituationOverlay?,
    densityScale: Float,
    labelStrokePaint: Paint,
    labelFillPaint: Paint,
    aircraftPlanViewPath: Path?,
) {
    if (situationOverlay == null) return
    Canvas(modifier = Modifier.fillMaxSize()) {
        val center = situationOverlay.pointUnits
        val ring = situationOverlay.ring
        if (ring != null) {
            val ringRadius = ring.radiusUnits
            drawCircle(
                color = Color(0x66000000),
                radius = ringRadius,
                center = center,
                style = Stroke(width = 16f),
            )
            drawCircle(
                color = Color.White,
                radius = ringRadius,
                center = center,
                style = Stroke(width = 6f),
            )
            ring.tickMarks.forEach { tick ->
                val inner = tick.innerUnits
                val outer = tick.outerUnits
                drawLine(Color(0x66000000), inner, outer, strokeWidth = 8f)
                drawLine(Color.White, inner, outer, strokeWidth = 6f)
            }
            drawContext.canvas.nativeCanvas.apply {
                labelStrokePaint.textSize = 16f * densityScale
                labelFillPaint.textSize = 16f * densityScale
                ring.cardinalLabels.forEach { label ->
                    val point = label.pointUnits
                    save()
                    rotate(label.rotationDeg, point.x, point.y)
                    drawText(label.text, point.x, point.y + labelFillPaint.textSize * 0.33f, labelStrokePaint)
                    drawText(label.text, point.x, point.y + labelFillPaint.textSize * 0.33f, labelFillPaint)
                    restore()
                }
            }
            drawCircle(
                color = Color.White,
                radius = ringRadius,
                center = center,
                style = Stroke(width = 6f),
            )
        }
        if (situationOverlay.predictorUnits != null) {
            val predictor = situationOverlay.predictorUnits
            val shaftEnd = arrowShaftEndPoint(center, predictor)
            drawLine(Color(0x66000000), center, shaftEnd, strokeWidth = 8f)
            drawLine(Color.White, center, shaftEnd, strokeWidth = 6f)
            val arrow = arrowHeadPath(center, predictor)
            drawPath(arrow, Color.White)
            drawPath(arrow, Color(0x66000000), style = Stroke(width = 1.5f))
        }
        if (ring != null) {
            drawContext.canvas.nativeCanvas.apply {
                val labelPoint = ring.labelPointUnits
                save()
                rotate(ring.labelRotationDeg, labelPoint.x, labelPoint.y)
                labelStrokePaint.textSize = 16f * densityScale
                labelFillPaint.textSize = 16f * densityScale
                drawText(ring.labelText, labelPoint.x, labelPoint.y + labelFillPaint.textSize * 0.33f, labelStrokePaint)
                drawText(ring.labelText, labelPoint.x, labelPoint.y + labelFillPaint.textSize * 0.33f, labelFillPaint)
                restore()
            }
        }
        if (aircraftPlanViewPath != null) {
            drawAircraftPlanView(
                path = aircraftPlanViewPath,
                center = center,
                headingDeg = situationOverlay.headingDeg,
                wingspanPx = ThumbSize.toPx() * 1.44f,
            )
        }
    }
}

@Composable
@OptIn(ExperimentalComposeUiApi::class)
internal fun MapSelectionTray(
    state: MapSelectionUiState,
    modifier: Modifier,
    centerProbeTag: String? = null,
    onBoundsChange: (Rect?) -> Unit = {},
    onSelectItem: (MapSelectionItem) -> Unit,
    onSelectAction: (MapSelectionItem, MapSelectionAction) -> Unit,
) {
    val uiTheme = LocalAerobagUiTheme.current
    val selectedItem = state.selectedItem
    val actionSlots = selectedItem?.actions.orEmpty()
    val actionRows = actionSlots.chunked(3)
    Surface(
        modifier = modifier
            .e2eIndexedControl(
                semanticTag = "parity:map-selection-tray",
                state = "enabled:true",
            )
            .testTag("parity:map-selection-tray")
            .semantics { testTagsAsResourceId = true }
            .onGloballyPositioned { coordinates ->
                onBoundsChange(coordinates.boundsInWindow())
            }
            .width(ThumbSize * 4.4f),
        shape = RoundedCornerShape(ThumbRadius),
        color = uiTheme.controls.panelBg.copy(alpha = 0.96f),
        contentColor = uiTheme.controls.panelFg,
        shadowElevation = 8.dp,
        border = BorderStroke(1.dp, uiTheme.controls.panelBorder.copy(alpha = 0.85f)),
    ) {
        Column(modifier = Modifier.padding(ThumbGap * 0.7f), verticalArrangement = Arrangement.spacedBy(ThumbGap * 0.55f)) {
            centerProbeTag?.let { tag ->
                Box(modifier = Modifier.size(1.dp).testTag(tag))
            }
            state.result.categories.forEach { category ->
                Row(
                    modifier = Modifier.horizontalScroll(rememberScrollState()),
                    horizontalArrangement = Arrangement.spacedBy(ThumbGap * 0.45f),
                ) {
                    if (category.items.isEmpty()) {
                        Text(
                            text = "no ${category.label.lowercase()}s",
                            modifier = Modifier.height(ThumbSize).width(ThumbSize * 1.28f).wrapContentSize(),
                            style = MaterialTheme.typography.labelSmall,
                            color = Color(0xFF697780),
                            textAlign = TextAlign.Center,
                        )
                    } else {
                        category.items.forEach { item ->
                            MapSelectionItemButton(
                                item = item,
                                selected = item.id == selectedItem?.id,
                                testTag = "parity:map-selection-item:${category.id}-${item.label}",
                                onClick = { onSelectItem(item) },
                            )
                        }
                    }
                }
            }
            Surface(
                shape = RoundedCornerShape(ThumbRadius),
                color = uiTheme.controls.panelBg.copy(alpha = 0.72f),
            ) {
                Column(modifier = Modifier.padding(ThumbGap * 0.6f), verticalArrangement = Arrangement.spacedBy(ThumbGap * 0.45f)) {
                    MapSelectionHeader(selectedItem)
                    actionRows.firstOrNull()?.let { actions ->
                        MapSelectionActionRow(
                            actions = actions,
                            selectedItem = selectedItem,
                            onSelectAction = onSelectAction,
                        )
                    }
                    if (selectedItem?.detailText != null) {
                        MapSelectionInlineDetailText(selectedItem.detailText)
                    } else {
                        actionRows.drop(1).forEach { actions ->
                            MapSelectionActionRow(
                                actions = actions,
                                selectedItem = selectedItem,
                                onSelectAction = onSelectAction,
                            )
                        }
                    }
                }
            }
        }
    }
}

@Composable
@OptIn(ExperimentalComposeUiApi::class)
internal fun MapSelectionHeader(selectedItem: MapSelectionItem?) {
    val uiTheme = LocalAerobagUiTheme.current
    val headerHeight = with(LocalDensity.current) { 34.sp.toDp() }
    val headerTextStyle = MaterialTheme.typography.labelMedium.copy(lineHeight = 15.sp)
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .height(headerHeight)
            .testTag("parity:map-selection-selected:${selectedItem?.label ?: "none"}")
            .semantics { testTagsAsResourceId = true },
        verticalArrangement = Arrangement.Center,
    ) {
        Text(
            text = buildAnnotatedString {
                if (selectedItem != null) {
                    withStyle(SpanStyle(fontWeight = FontWeight.Bold)) {
                        append(selectedItem.label)
                    }
                    mapSelectionHeaderDetailText(selectedItem)
                        .takeIf { it.isNotEmpty() }
                        ?.let { append(" · $it") }
                } else {
                    append(" ")
                }
            },
            style = headerTextStyle,
            color = uiTheme.controls.panelFg,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
        )
        Text(
            text = selectedItem?.secondaryDescription?.trim()?.takeIf { it.isNotEmpty() } ?: "\u00a0",
            style = headerTextStyle.copy(
                fontWeight = FontWeight.Bold,
            ),
            color = uiTheme.controls.panelFg.copy(alpha = 0.72f),
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
        )
    }
}

@Composable
internal fun MapSelectionActionRow(
    actions: List<MapSelectionAction>,
    selectedItem: MapSelectionItem?,
    onSelectAction: (MapSelectionItem, MapSelectionAction) -> Unit,
) {
    Row(
        modifier = Modifier.height(ThumbSize),
        horizontalArrangement = Arrangement.spacedBy(ThumbGap * 0.45f),
    ) {
        repeat(3) { index ->
            val action = actions.getOrNull(index)
            if (action == null || action.placeholder) {
                Spacer(modifier = Modifier.width(ThumbSize * 1.2f).height(ThumbSize))
            } else {
                val actionEnabled = action.enabled && !action.displayOnly
                val acceptsTap = !action.displayOnly &&
                    (actionEnabled || !action.disabledReason.isNullOrBlank())
                MapSelectionActionButton(
                    action = action,
                    enabled = actionEnabled,
                    acceptsTap = acceptsTap,
                    onClick = {
                        if (selectedItem != null) onSelectAction(selectedItem, action)
                    },
                )
            }
        }
    }
}

@Composable
internal fun MapSelectionInlineDetailText(detail: String) {
    val uiTheme = LocalAerobagUiTheme.current
    Surface(
        modifier = Modifier
            .fillMaxWidth()
            .height(ThumbSize),
        shape = RoundedCornerShape(ThumbRadius),
        color = Color.White.copy(alpha = 0.82f),
        contentColor = uiTheme.controls.panelFg,
    ) {
        Text(
            text = detail,
            modifier = Modifier.padding(horizontal = ThumbSize * 0.08f, vertical = ThumbSize * 0.07f),
            style = MaterialTheme.typography.labelSmall.copy(
                fontSize = 12.sp,
                fontWeight = FontWeight.Bold,
                lineHeight = 13.sp,
            ),
            color = uiTheme.controls.panelFg,
            maxLines = 4,
            overflow = TextOverflow.Clip,
        )
    }
}

@Composable
@OptIn(ExperimentalComposeUiApi::class)
internal fun MapSelectionDetailModal(
    title: String,
    text: String,
    status: MapSelectionDetailStatus? = null,
    onTimeDisplayAction: (String) -> Unit = {},
    modifier: Modifier = Modifier,
) {
    val uiTheme = LocalAerobagUiTheme.current
    Surface(
        modifier = modifier
            .testTag("parity:map-selection-detail-modal:$title")
            .semantics { testTagsAsResourceId = true }
            .widthIn(max = ThumbSize * 9.5f)
            .heightIn(max = ThumbSize * 11.5f),
        shape = RoundedCornerShape(ThumbRadius + 4.dp),
        color = uiTheme.controls.panelBg.copy(alpha = 0.98f),
        contentColor = uiTheme.controls.panelFg,
        shadowElevation = 8.dp,
        border = BorderStroke(1.dp, uiTheme.controls.panelBorder.copy(alpha = 0.85f)),
    ) {
        Column(
            modifier = Modifier.padding(ThumbSize * 0.18f),
            verticalArrangement = Arrangement.spacedBy(ThumbGap * 0.85f),
        ) {
            Text(
                text = title.uppercase(),
                style = MaterialTheme.typography.labelMedium.copy(
                    fontWeight = FontWeight.Black,
                    letterSpacing = 0.4.sp,
                ),
                color = uiTheme.controls.panelFg,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
            status?.let { timing ->
                Text(
                    text = timing.text,
                    modifier = if (timing.actionId != null) {
                        Modifier.clickable { onTimeDisplayAction(timing.actionId) }
                    } else {
                        Modifier
                    },
                    style = MaterialTheme.typography.labelMedium.copy(
                        fontWeight = FontWeight.Black,
                        lineHeight = 18.sp,
                        textDecoration = if (timing.actionId == null) {
                            TextDecoration.None
                        } else {
                            TextDecoration.Underline
                        },
                    ),
                    color = aviationColor(uiTheme, timing.colorKey),
                )
            }
            WeatherDetailSection(
                label = null,
                ageLabel = null,
                ageWarning = false,
                text = text,
                expanded = true,
            )
        }
    }
}

@Composable
@OptIn(ExperimentalComposeUiApi::class)
internal fun WeatherDetailModal(
    detail: WeatherDetailUiView,
    modifier: Modifier = Modifier,
) {
    val uiTheme = LocalAerobagUiTheme.current
    Surface(
        modifier = modifier
            .testTag("parity:weather-detail-modal")
            .semantics { testTagsAsResourceId = true }
            .widthIn(max = ThumbSize * 10.5f)
            .heightIn(max = ThumbSize * 11.5f),
        shape = RoundedCornerShape(ThumbRadius + 4.dp),
        color = uiTheme.controls.panelBg.copy(alpha = 0.98f),
        contentColor = uiTheme.controls.panelFg,
        shadowElevation = 8.dp,
        border = BorderStroke(1.dp, uiTheme.controls.panelBorder.copy(alpha = 0.85f)),
    ) {
        Column(
            modifier = Modifier
                .verticalScroll(rememberScrollState())
                .padding(ThumbSize * 0.18f),
            verticalArrangement = Arrangement.spacedBy(ThumbGap * 0.85f),
        ) {
            Text(
                text = detail.title.uppercase(),
                style = MaterialTheme.typography.labelMedium.copy(
                    fontWeight = FontWeight.Black,
                    letterSpacing = 0.4.sp,
                ),
                color = uiTheme.controls.panelFg,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
            Text(
                text = detail.advisoryText,
                modifier = Modifier
                    .fillMaxWidth()
                    .background(uiTheme.controls.dataStatusWarningBg, RoundedCornerShape(ThumbRadius))
                    .border(1.dp, uiTheme.controls.dataStatusWarningStroke, RoundedCornerShape(ThumbRadius))
                    .padding(ThumbSize * 0.13f),
                style = MaterialTheme.typography.bodyMedium.copy(
                    fontWeight = FontWeight.Bold,
                    lineHeight = 18.sp,
                ),
                color = lerp(Color.Black, uiTheme.controls.dataStatusWarningStroke, 0.3f),
            )
            detail.sections.forEach { section ->
                when (section.kind) {
                    WeatherDetailSectionKind.Text -> WeatherDetailSection(
                        label = section.label,
                        ageLabel = section.trailingLabel,
                        ageWarning = section.trailingWarning,
                        text = section.text,
                        emptyText = section.emptyText,
                        constrainHeight = false,
                    )
                    WeatherDetailSectionKind.Notams -> AirportNotamSection(
                        notams = section.notams,
                        label = section.label,
                        trailingLabel = section.trailingLabel.orEmpty(),
                        emptyText = section.emptyText,
                    )
                }
            }
        }
    }
}

@Composable
@OptIn(ExperimentalComposeUiApi::class)
internal fun AirportInfoModal(
    detail: AirportInfoUiView,
    modifier: Modifier = Modifier,
    onTimeDisplayAction: (String) -> Unit,
) {
    val context = LocalContext.current
    val uiTheme = LocalAerobagUiTheme.current
    val airportIdentityStyle = MaterialTheme.typography.labelSmall.copy(fontWeight = FontWeight.Bold)
    val scrollState = rememberScrollState()
    Surface(
        modifier = modifier
            .testTag("parity:airport-info-modal:${detail.airportId}")
            .semantics { testTagsAsResourceId = true }
            .widthIn(max = ThumbSize * 10.5f)
            .heightIn(max = ThumbSize * 11.5f),
        shape = RoundedCornerShape(ThumbRadius + 4.dp),
        color = uiTheme.controls.panelBg.copy(alpha = 0.98f),
        contentColor = uiTheme.controls.panelFg,
        shadowElevation = 8.dp,
        border = BorderStroke(1.dp, uiTheme.controls.panelBorder.copy(alpha = 0.85f)),
    ) {
        Column(
            modifier = Modifier
                .testTag("parity:airport-info-scroll:${scrollState.value}")
                .verticalScroll(scrollState)
                .padding(ThumbSize * 0.18f),
            verticalArrangement = Arrangement.spacedBy(ThumbGap * 0.65f),
        ) {
            Text(
                text = detail.airportId.uppercase(),
                style = MaterialTheme.typography.labelMedium.copy(fontWeight = FontWeight.Black),
                color = uiTheme.controls.panelFg,
            )
            Text(
                text = detail.name,
                style = airportIdentityStyle,
                color = uiTheme.controls.panelMuted,
            )
            detail.locationLabel?.let { location ->
                Text(
                    text = location,
                    style = airportIdentityStyle,
                    color = uiTheme.controls.panelMuted,
                )
            }
            detail.factSections.forEach { section ->
                section.title?.let { AirportInfoSectionTitle(it) }
                section.facts.forEach { fact ->
                    Box(
                        modifier = Modifier.testTag(
                            "parity:airport-info-fact:${fact.label}:${fact.value}",
                        ),
                    ) {
                        AirportInfoFact(
                            label = fact.label,
                            value = fact.value,
                            nextInLabel = fact.nextInLabel,
                            semanticTag = fact.actionId?.let {
                                "parity:airport-info-time-toggle"
                            },
                            onClick = when {
                            fact.actionId != null -> {
                                { onTimeDisplayAction(fact.actionId) }
                            }
                            fact.linkUrl != null -> {
                                {
                                    val uri = Uri.parse(fact.linkUrl)
                                try {
                                    context.startActivity(
                                        Intent(
                                            if (uri.scheme == "tel") {
                                                Intent.ACTION_DIAL
                                            } else {
                                                Intent.ACTION_VIEW
                                            },
                                            uri,
                                        ),
                                    )
                                } catch (_: ActivityNotFoundException) {
                                    Toast.makeText(
                                        context,
                                        "No application can open this link.",
                                        Toast.LENGTH_SHORT,
                                    ).show()
                                }
                            }
                            }
                            else -> null
                            },
                        )
                    }
                }
            }
            if (detail.runways.isNotEmpty()) {
                AirportInfoSectionTitle(detail.runwaysSectionTitle)
                Box(
                    modifier = Modifier
                        .size(1.dp)
                        .testTag(
                            "parity:airport-info-runways:complex:${detail.runwayDiagramComplex}:count:${detail.runways.size}",
                        ),
                )
                detail.runways.forEachIndexed { index, runway ->
                    Row(
                        modifier = Modifier
                            .testTag(
                                "parity:airport-info-runway:${runway.endALabel}:${runway.endBLabel}",
                            )
                            .fillMaxWidth()
                            .heightIn(min = ThumbSize * 2.25f),
                        horizontalArrangement = Arrangement.spacedBy(ThumbGap * 0.8f),
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        AirportRunwayDiagram(
                            runways = detail.runways,
                            activeRunwayIndex = index,
                            complex = detail.runwayDiagramComplex,
                        )
                        Column(
                            modifier = Modifier.weight(1f),
                            verticalArrangement = Arrangement.spacedBy(ThumbGap * 0.18f),
                        ) {
                            Text("${runway.endALabel} /", fontWeight = FontWeight.Bold)
                            Text(runway.endBLabel, fontWeight = FontWeight.Bold)
                            Text(runway.dimensionsLabel, fontWeight = FontWeight.Bold)
                            Text(runway.surfaceLabel, fontWeight = FontWeight.Bold)
                        }
                    }
                }
            }
        }
    }
}

@Composable
private fun AirportInfoSectionTitle(label: String) {
    val uiTheme = LocalAerobagUiTheme.current
    Text(
        text = label.uppercase(),
        modifier = Modifier
            .fillMaxWidth()
            .padding(top = ThumbGap * 0.55f),
        style = MaterialTheme.typography.labelSmall.copy(fontWeight = FontWeight.Black),
        color = uiTheme.controls.panelMuted,
    )
}

@Composable
private fun AirportInfoFact(
    label: String,
    value: String,
    nextInLabel: String? = null,
    semanticTag: String? = null,
    onClick: (() -> Unit)? = null,
) {
    val uiTheme = LocalAerobagUiTheme.current
    Row(
        modifier = Modifier.fillMaxWidth(),
        horizontalArrangement = Arrangement.spacedBy(ThumbGap * 0.7f),
        verticalAlignment = Alignment.Top,
    ) {
        Text(
            text = label,
            modifier = Modifier.weight(0.8f),
            style = MaterialTheme.typography.labelSmall.copy(fontWeight = FontWeight.Bold),
            color = uiTheme.controls.panelMuted,
        )
        Row(
            modifier = Modifier.weight(1.2f),
            horizontalArrangement = Arrangement.spacedBy(ThumbGap * 0.45f),
        ) {
            Text(
                text = value,
                modifier = if (onClick == null) {
                    Modifier.weight(1f)
                } else {
                    val base = Modifier.weight(1f)
                    val indexed = if (semanticTag == null) {
                        base
                    } else {
                        base
                            .e2eIndexedControl(
                                semanticTag = semanticTag,
                                state = "enabled:true:selected:false:text:${Uri.encode(value)}",
                            )
                            .testTag(semanticTag)
                    }
                    indexed.clickable(onClick = onClick)
                },
                style = MaterialTheme.typography.bodyMedium.copy(fontWeight = FontWeight.Bold),
                color = if (onClick == null) {
                    uiTheme.controls.panelFg
                } else {
                    uiTheme.aviation.classBDBlue
                },
            )
            nextInLabel?.let {
                Text(
                    text = "◷ $it",
                    style = MaterialTheme.typography.bodyMedium.copy(fontWeight = FontWeight.Black),
                    color = uiTheme.aviation.classBDBlue,
                )
            }
        }
    }
}

@Composable
private fun AirportRunwayDiagram(
    runways: List<AirportRunwayUiView>,
    activeRunwayIndex: Int,
    complex: Boolean,
) {
    val uiTheme = LocalAerobagUiTheme.current
    val activeRunway = runways[activeRunwayIndex]
    val runwayColor = when (activeRunway.surfaceColorKey) {
        "airport_runway_turf" -> uiTheme.aviation.airportRunwayTurf
        "airport_runway_unpaved" -> uiTheme.aviation.airportRunwayUnpaved
        "airport_runway_water" -> uiTheme.aviation.airportRunwayWater
        else -> uiTheme.aviation.airportRunwayPaved
    }
    Canvas(
        modifier = Modifier
            .size(ThumbSize * 2.25f)
            .border(1.dp, uiTheme.controls.panelBorder)
            .background(uiTheme.controls.mapSelectionDisplayBg.copy(alpha = 0.72f)),
    ) {
        val center = Offset(size.width / 2f, size.height / 2f)
        val unit = size.minDimension * 0.84f
        val point = { x: Double, y: Double ->
            Offset(
                center.x + x.toFloat() * unit,
                center.y + y.toFloat() * unit,
            )
        }
        val minimumExtent = 2.dp.toPx()
        val displayedRunways = if (complex) {
            runways.withIndex().sortedBy { it.index == activeRunwayIndex }
        } else {
            listOf(IndexedValue(activeRunwayIndex, activeRunway))
        }
        displayedRunways.forEach { (index, runway) ->
            val rawStart = point(runway.diagramEndAX, runway.diagramEndAY)
            val rawEnd = point(runway.diagramEndBX, runway.diagramEndBY)
            val rawDelta = rawEnd - rawStart
            val rawLength = rawDelta.getDistance()
            val direction = if (rawLength > 0f) {
                rawDelta / rawLength
            } else {
                Offset(0f, -1f)
            }
            val displayLength = rawLength.coerceAtLeast(minimumExtent)
            val runwayCenter = (rawStart + rawEnd) / 2f
            val halfLength = displayLength / 2f
            val halfWidth =
                (runway.diagramWidthRatio.toFloat() * unit).coerceAtLeast(minimumExtent) / 2f
            val perpendicular = Offset(-direction.y, direction.x) * halfWidth
            val start = runwayCenter - direction * halfLength
            val end = runwayCenter + direction * halfLength
            val path = Path().apply {
                moveTo(start.x + perpendicular.x, start.y + perpendicular.y)
                lineTo(end.x + perpendicular.x, end.y + perpendicular.y)
                lineTo(end.x - perpendicular.x, end.y - perpendicular.y)
                lineTo(start.x - perpendicular.x, start.y - perpendicular.y)
                close()
            }
            if (index == activeRunwayIndex) {
                drawPath(path = path, color = runwayColor)
            } else {
                drawPath(
                    path = path,
                    color = uiTheme.aviation.airportRunwayInactive,
                    style = Stroke(width = 1.25.dp.toPx()),
                )
            }
        }
        listOfNotNull(
            activeRunway.diagramEndAPattern,
            activeRunway.diagramEndBPattern,
        ).forEach { pattern ->
            val path = Path().apply {
                val base = point(pattern.baseX, pattern.baseY)
                val corner = point(pattern.cornerX, pattern.cornerY)
                val final = point(pattern.finalX, pattern.finalY)
                moveTo(base.x, base.y)
                lineTo(corner.x, corner.y)
                lineTo(final.x, final.y)
            }
            drawPath(
                path = path,
                color = uiTheme.aviation.airportRunwayPattern,
                style = Stroke(
                    width = 1.5.dp.toPx(),
                    cap = StrokeCap.Square,
                    join = StrokeJoin.Miter,
                ),
            )
        }
    }
}

@Composable
internal fun AirportNotamSection(
    notams: List<AirportNotamUiView>,
    label: String = "NOTAM",
    trailingLabel: String = notams.size.toString(),
    emptyText: String,
) {
    val uiTheme = LocalAerobagUiTheme.current
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .background(
                uiTheme.controls.mapSelectionDisplayBg.copy(alpha = 0.72f),
                RoundedCornerShape(ThumbRadius),
            )
            .border(
                1.dp,
                uiTheme.controls.panelBorder.copy(alpha = 0.4f),
                RoundedCornerShape(ThumbRadius),
            )
            .padding(ThumbSize * 0.13f),
        verticalArrangement = Arrangement.spacedBy(ThumbGap * 0.45f),
    ) {
        Row(
            modifier = Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.SpaceBetween,
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text(
                text = label,
                style = MaterialTheme.typography.labelSmall.copy(
                    fontWeight = FontWeight.Black,
                    letterSpacing = 0.6.sp,
                ),
                color = uiTheme.controls.panelFg,
            )
            Text(
                text = trailingLabel,
                style = MaterialTheme.typography.labelSmall.copy(fontWeight = FontWeight.Black),
                color = uiTheme.controls.panelFg,
            )
        }
        if (notams.isEmpty()) {
            Text(
                text = emptyText,
                style = MaterialTheme.typography.bodyMedium.copy(fontWeight = FontWeight.Bold),
                color = uiTheme.controls.panelFg.copy(alpha = 0.65f),
            )
        } else {
            Column(
                verticalArrangement = Arrangement.spacedBy(ThumbGap * 0.65f),
            ) {
                notams.forEach { notam ->
                    Column(
                        modifier = Modifier
                            .fillMaxWidth()
                            .background(
                                uiTheme.controls.mapSelectionDisplayBg,
                                RoundedCornerShape(ThumbRadius * 0.75f),
                            )
                            .border(
                                1.dp,
                                uiTheme.controls.panelBorder.copy(alpha = 0.5f),
                                RoundedCornerShape(ThumbRadius * 0.75f),
                            )
                            .padding(ThumbSize * 0.11f),
                        verticalArrangement = Arrangement.spacedBy(ThumbGap * 0.35f),
                    ) {
                        Text(
                            text = notam.label,
                            style = MaterialTheme.typography.labelSmall.copy(
                                fontWeight = FontWeight.Black,
                                letterSpacing = 0.6.sp,
                            ),
                            color = uiTheme.controls.panelFg,
                        )
                        Text(
                            text = notam.text,
                            style = MaterialTheme.typography.bodyMedium.copy(
                                fontSize = 15.sp,
                                lineHeight = 19.sp,
                                fontWeight = FontWeight.Bold,
                                fontFamily = FontFamily.Monospace,
                            ),
                            color = uiTheme.controls.panelFg,
                        )
                    }
                }
            }
        }
    }
}

@Composable
@OptIn(ExperimentalComposeUiApi::class)
internal fun ProcedureNotamModal(
    detail: PlateProcedureNotamDetail,
    modifier: Modifier = Modifier,
) {
    val uiTheme = LocalAerobagUiTheme.current
    Surface(
        modifier = modifier
            .testTag("parity:procedure-notam-modal")
            .semantics { testTagsAsResourceId = true }
            .widthIn(max = ThumbSize * 10.5f)
            .heightIn(max = ThumbSize * 11.5f),
        shape = RoundedCornerShape(ThumbRadius + 4.dp),
        color = uiTheme.controls.panelBg.copy(alpha = 0.98f),
        contentColor = uiTheme.controls.panelFg,
        shadowElevation = 8.dp,
        border = BorderStroke(1.dp, uiTheme.controls.panelBorder.copy(alpha = 0.85f)),
    ) {
        Column(
            modifier = Modifier
                .verticalScroll(rememberScrollState())
                .padding(ThumbSize * 0.18f),
            verticalArrangement = Arrangement.spacedBy(ThumbGap * 0.85f),
        ) {
            Text(
                text = detail.title.uppercase(),
                style = MaterialTheme.typography.labelMedium.copy(
                    fontWeight = FontWeight.Black,
                    letterSpacing = 0.4.sp,
                ),
                color = uiTheme.controls.panelFg,
            )
            Text(
                text = detail.advisoryText,
                modifier = Modifier
                    .fillMaxWidth()
                    .background(uiTheme.controls.dataStatusWarningBg, RoundedCornerShape(ThumbRadius))
                    .border(1.dp, uiTheme.controls.dataStatusWarningStroke, RoundedCornerShape(ThumbRadius))
                    .padding(ThumbSize * 0.13f),
                style = MaterialTheme.typography.bodyMedium.copy(
                    fontWeight = FontWeight.Bold,
                    lineHeight = 18.sp,
                ),
                color = lerp(Color.Black, uiTheme.controls.dataStatusWarningStroke, 0.3f),
            )
            AirportNotamSection(
                notams = detail.notams,
                emptyText = detail.emptyText,
            )
        }
    }
}

@Composable
private fun WeatherDetailSection(
    label: String?,
    ageLabel: String?,
    ageWarning: Boolean,
    text: String?,
    emptyText: String? = null,
    expanded: Boolean = false,
    constrainHeight: Boolean = true,
) {
    val uiTheme = LocalAerobagUiTheme.current
    val minHeight = if (expanded) ThumbSize * 6.4f else ThumbSize * 1.2f
    val maxHeight = if (expanded) ThumbSize * 6.8f else ThumbSize * 3.3f
    val heightModifier = if (constrainHeight) {
        Modifier.heightIn(min = minHeight, max = maxHeight)
    } else {
        Modifier
    }
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .then(heightModifier)
            .background(uiTheme.controls.mapSelectionDisplayBg.copy(alpha = 0.72f), RoundedCornerShape(ThumbRadius))
            .border(1.dp, uiTheme.controls.panelBorder.copy(alpha = 0.4f), RoundedCornerShape(ThumbRadius))
            .padding(ThumbSize * 0.13f),
        verticalArrangement = Arrangement.spacedBy(ThumbGap * 0.45f),
    ) {
        if (label != null || ageLabel != null) {
            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.SpaceBetween,
                verticalAlignment = Alignment.CenterVertically,
            ) {
                label?.let { labelText ->
                    Text(
                        text = labelText,
                        style = MaterialTheme.typography.labelSmall.copy(
                            fontWeight = FontWeight.Black,
                            letterSpacing = 0.6.sp,
                        ),
                        color = uiTheme.controls.panelFg,
                    )
                } ?: Spacer(modifier = Modifier)
                ageLabel?.let { labelText ->
                    Text(
                        text = labelText,
                        style = MaterialTheme.typography.labelSmall.copy(
                            fontWeight = FontWeight.Black,
                            letterSpacing = 0.6.sp,
                        ),
                        color = if (ageWarning) uiTheme.controls.dataStatusWarningStroke else uiTheme.controls.panelFg,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                    )
                }
            }
        }
        Text(
            text = text ?: emptyText ?: "No ${label ?: "text"} available.",
            modifier = Modifier
                .fillMaxWidth()
                .then(
                    if (constrainHeight) {
                        Modifier.verticalScroll(rememberScrollState())
                    } else {
                        Modifier
                    },
                ),
            style = MaterialTheme.typography.bodyMedium.copy(
                fontSize = 16.sp,
                lineHeight = 20.sp,
                fontWeight = FontWeight.Bold,
                fontFamily = if (text == null) FontFamily.Default else FontFamily.Monospace,
            ),
            color = if (text == null) uiTheme.controls.panelFg.copy(alpha = 0.65f) else uiTheme.controls.panelFg,
        )
    }
}

private fun immediateWeatherDetailForMapSelectionItem(item: MapSelectionItem): WeatherDetailUiView? {
    return item.weatherDetail.takeIf { item.metarFeature != null }
}

@Composable
internal fun MapSelectionItemButton(
    item: MapSelectionItem,
    selected: Boolean,
    testTag: String,
    onClick: () -> Unit,
) {
    val uiTheme = LocalAerobagUiTheme.current
    val containerColor = if (selected) uiTheme.controls.buttonChecked else uiTheme.controls.buttonUnchecked
    SelectedControlHighlightFrame(
        selected = selected,
        modifier = Modifier.size(ThumbSize + 8.dp),
    ) {
        Surface(
            modifier = Modifier
                .fillMaxSize()
                .e2eIndexedControl(
                    semanticTag = testTag,
                    state =
                        "enabled:true:selected:$selected:" +
                            "text:${android.net.Uri.encode(item.label)}",
                )
                .testTag(testTag)
                .clickable(onClick = onClick),
            shape = RoundedCornerShape(ThumbRadius),
            color = containerColor,
            contentColor = uiTheme.controls.buttonFg,
            border = BorderStroke(1.dp, lerp(containerColor, Color.Black, 0.22f)),
        ) {
            Column(
                modifier = Modifier.fillMaxSize().padding(3.dp),
                horizontalAlignment = Alignment.CenterHorizontally,
                verticalArrangement = Arrangement.Center,
            ) {
                MapSelectionItemIcon(item, Modifier.weight(1f).fillMaxWidth())
                Text(
                    text = item.label,
                    style = MaterialTheme.typography.labelSmall.copy(
                        fontSize = IconButtonLabelFontSize,
                        fontWeight = FontWeight.Bold,
                    ),
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                    textAlign = TextAlign.Center,
                )
            }
        }
    }
}

@Composable
internal fun MapSelectionItemIcon(item: MapSelectionItem, modifier: Modifier) {
    val uiTheme = LocalAerobagUiTheme.current
    when {
        item.symbolFeature != null -> PlanWaypointSymbol(item.symbolFeature, modifier)
        item.metarFeature != null -> Canvas(modifier = modifier) {
            drawMetarSymbol(item.metarFeature, Offset(size.width / 2f, size.height / 2f), density, uiTheme)
        }
        item.pirepFeature != null -> Canvas(modifier = modifier) {
            drawPirepSymbol(item.pirepFeature, Offset(size.width / 2f, size.height * 0.43f), density, uiTheme)
        }
        item.highlight is MapSelectionHighlight.Spot -> Canvas(modifier = modifier) {
            val scale = size.minDimension / 46f
            val center = Offset(size.width / 2f, size.height * 0.9f)
            drawMapSelectionSpotSymbol(center, scale, uiTheme)
        }
        item.airspaceIcon != null -> Canvas(modifier = modifier) {
            drawAirspaceIcon(uiTheme, item.airspaceIcon)
        }
        else -> Box(modifier = modifier, contentAlignment = Alignment.Center) {
            Text(item.sublabel.ifBlank { item.label }, style = MaterialTheme.typography.labelSmall, textAlign = TextAlign.Center)
        }
    }
}

internal fun androidx.compose.ui.graphics.drawscope.DrawScope.drawMapSelectionSpotSymbol(center: Offset, scale: Float, uiTheme: UiTheme) {
    mapSelectionSpotSymbol(center, scale).forEach { layer ->
        val drawLayer = {
            drawNavSymbolLayer(layer, scale, uiTheme)
        }
        if (layer.transformDegrees != null) {
            rotate(layer.transformDegrees, center) { drawLayer() }
        } else {
            drawLayer()
        }
    }
}

internal fun androidx.compose.ui.graphics.drawscope.DrawScope.drawAirspaceIcon(uiTheme: UiTheme, feature: AirspaceDisplayPath) {
    val iconSize = size.minDimension
    val scale = iconSize / 64f
    val left = (size.width - iconSize) / 2f
    val top = (size.height - iconSize) / 2f
    translate(left = left, top = top) {
        scale(scale = scale, pivot = Offset.Zero) {
            drawAirspaceDisplayPath(uiTheme, feature)
        }
    }
}

internal fun androidx.compose.ui.graphics.drawscope.DrawScope.drawNavSymbolLayer(
    layer: NavSymbolLayer,
    scale: Float,
    uiTheme: UiTheme,
    dynamicColors: Map<String, Color> = emptyMap(),
) {
    navSymbolColor(layer.fill, uiTheme, dynamicColors)?.let { fill ->
        drawPath(layer.path, fill)
    }
    navSymbolColor(layer.stroke, uiTheme, dynamicColors)?.let { stroke ->
        drawPath(
            layer.path,
            stroke,
            style = Stroke(
                width = (layer.strokeWidth ?: 1f) * scale,
                cap = navSymbolStrokeCap(layer.lineCap),
                join = navSymbolStrokeJoin(layer.lineJoin),
            ),
        )
    }
}

private fun androidx.compose.ui.graphics.drawscope.DrawScope.drawNavSymbolLayerAsContrast(
    layer: NavSymbolLayer,
    scale: Float,
    color: Color,
    strokeWidth: Float,
) {
    if (layer.fill != null && layer.fill != "none") {
        drawPath(layer.path, color, style = Stroke(width = strokeWidth, join = navSymbolStrokeJoin(layer.lineJoin)))
    }
    if (layer.stroke != null && layer.stroke != "none") {
        drawPath(
            layer.path,
            color,
            style = Stroke(
                width = kotlin.math.max(strokeWidth, ((layer.strokeWidth ?: 1f) * scale) + strokeWidth),
                cap = navSymbolStrokeCap(layer.lineCap),
                join = navSymbolStrokeJoin(layer.lineJoin),
            ),
        )
    }
}

internal fun navSymbolColor(token: String?, uiTheme: UiTheme, dynamicColors: Map<String, Color>): Color? = when (token) {
    null, "none" -> null
    "white" -> Color.White
    "white_90" -> Color.White.copy(alpha = 0.9f)
    "white_68" -> Color.White.copy(alpha = 0.68f)
    "paper" -> Color(0xFFFFFEF8)
    "pirep_ink" -> Color(0xFF071015)
    "ink_70" -> Color(0xB3081218)
    "ink_75" -> Color(0xBF081218)
    "class_c_magenta" -> uiTheme.aviation.classCMagenta
    "button_unchecked" -> uiTheme.controls.buttonUnchecked
    "button_icon" -> uiTheme.controls.buttonFg
    "button_icon_secondary" -> uiTheme.controls.buttonIconSecondary
    "flight_plan_guidance" -> uiTheme.flightPlanRoute.guidanceArrow
    "compass_north" -> uiTheme.controls.compassNorth
    "compass_south" -> uiTheme.controls.compassSouth
    else -> dynamicColors[token]
}

internal fun navSymbolStrokeCap(value: String?): StrokeCap = when (value) {
    "round" -> StrokeCap.Round
    "square" -> StrokeCap.Square
    else -> StrokeCap.Butt
}

internal fun navSymbolStrokeJoin(value: String?): StrokeJoin = when (value) {
    "bevel" -> StrokeJoin.Bevel
    "miter" -> StrokeJoin.Miter
    else -> StrokeJoin.Round
}

@Composable
internal fun MapSelectionActionButton(
    action: MapSelectionAction,
    enabled: Boolean,
    acceptsTap: Boolean,
    onClick: () -> Unit,
) {
    val uiTheme = LocalAerobagUiTheme.current
    val containerColor = when {
        action.displayOnly -> uiTheme.controls.buttonChecked
        enabled -> uiTheme.controls.buttonUnchecked
        else -> uiTheme.controls.buttonDisabled
    }
    Surface(
        modifier = Modifier
            .width(ThumbSize * 1.2f)
            .height(ThumbSize)
            .e2eIndexedControl(
                semanticTag = "parity:map-selection-action:${action.id}",
                state =
                    "enabled:${enabled && acceptsTap}:selected:${action.displayOnly}:" +
                        "text:${android.net.Uri.encode(buttonLabel(action.label))}",
            )
            .testTag("parity:map-selection-action:${action.id}")
            .alpha(if (action.label.isBlank()) 0f else 1f)
            .semantics {
                if (!enabled) {
                    disabled()
                    action.disabledReason?.let { stateDescription = it }
                }
            }
            .then(if (acceptsTap) Modifier.clickable(onClick = onClick) else Modifier),
        shape = RoundedCornerShape(ThumbRadius),
        color = containerColor,
        contentColor = uiTheme.controls.buttonFg,
        border = BorderStroke(
            1.dp,
            lerp(containerColor, Color.Black, 0.22f),
        ),
    ) {
        Box(modifier = Modifier.fillMaxSize().padding(3.dp)) {
            if (action.airspaceLimit != null) {
                Canvas(modifier = Modifier.fillMaxSize()) {
                    drawAirspaceLimitGlyph(uiTheme, action.airspaceLimit, Offset(size.width / 2f, size.height / 2f), 1.45f)
                }
            } else {
                if (hasActionSymbol(action.id)) {
                    ActionIcon(
                        actionId = action.id,
                        enabled = enabled,
                        modifier = Modifier
                            .align(Alignment.TopCenter)
                            .size(ThumbSize * 0.68f),
                    )
                }
                OutlinedButtonLabel(
                    text = buttonLabel(action.label),
                    modifier = Modifier
                        .align(Alignment.BottomCenter)
                        .fillMaxWidth()
                        .padding(horizontal = 1.dp, vertical = 2.dp),
                    style = MaterialTheme.typography.labelSmall.copy(
                        fontSize = IconButtonLabelFontSize,
                        fontWeight = FontWeight.Bold,
                        lineHeight = IconButtonLabelFontSize,
                    ),
                    color = uiTheme.controls.buttonFg,
                    maxLines = 2,
                )
            }
        }
    }
}

@Composable
internal fun AirportInsertPanel(
    state: AndroidAirportInsertState,
    modifier: Modifier,
    onTextChange: (String) -> Unit,
    onSubmit: () -> Unit,
    onSuggestionClick: (WaypointIdentifierSuggestion) -> Unit,
) {
    val focusRequester = remember { FocusRequester() }
    val keyboardController = LocalSoftwareKeyboardController.current
    val submitAction = rememberCurrentAction(onSubmit)
    var e2eFocused by remember { mutableStateOf(false) }
    LaunchedEffect(state.rowUid, state.before) {
        focusRequester.requestFocus()
        keyboardController?.show()
    }
    Surface(
        modifier = modifier,
        shape = RoundedCornerShape(ThumbRadius),
        color = Color(0xF7FCF8F1),
        contentColor = Color(0xFF132129),
        border = BorderStroke(1.dp, Color(0x334E626C)),
        shadowElevation = 8.dp,
    ) {
        Column(
            modifier = Modifier.padding(ThumbGap),
            verticalArrangement = Arrangement.spacedBy(ThumbGap),
        ) {
            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.spacedBy(ThumbGap),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Text(
                    text = if (state.before) "INSERT BEFORE" else "INSERT AFTER",
                    style = MaterialTheme.typography.labelSmall,
                    fontWeight = FontWeight.ExtraBold,
                    color = Color(0xFF52656D),
                )
                BasicTextField(
                    value = state.airportId,
                    onValueChange = onTextChange,
                    singleLine = true,
                    keyboardOptions =
                        KeyboardOptions(
                            capitalization = KeyboardCapitalization.Characters,
                            autoCorrectEnabled = false,
                            keyboardType = KeyboardType.Password,
                            imeAction = ImeAction.Done,
                            platformImeOptions =
                                PlatformImeOptions(
                                    privateImeOptions = "com.google.android.inputmethod.latin.forceAscii",
                                ),
                        ),
                    keyboardActions = KeyboardActions(onDone = { submitAction() }),
                    textStyle =
                        MaterialTheme.typography.headlineMedium.copy(
                            color = Color(0xFF132129),
                            fontWeight = FontWeight.ExtraBold,
                            textAlign = TextAlign.Center,
                        ),
                    modifier =
                        Modifier
                            .e2eIndexedTextControl(
                                semanticTag = "parity:plan-insert-airport-input",
                                text = state.airportId,
                                enabled = !state.loading,
                                focused = e2eFocused,
                            )
                            .testTag("parity:plan-insert-airport-input")
                            .weight(1f)
                            .height(ThumbSize)
                            .focusRequester(focusRequester)
                            .onFocusChanged { focusState -> e2eFocused = focusState.isFocused }
                            .clip(RoundedCornerShape(ThumbRadius))
                            .background(Color.White)
                            .border(1.dp, Color(0x334E626C), RoundedCornerShape(ThumbRadius))
                            .padding(horizontal = ThumbGap, vertical = ThumbSize * 0.18f),
                )
                CompactSquareButton(label = "Enter", modifier = Modifier.width(ThumbSize * 1.4f).height(ThumbSize), onClick = submitAction)
            }
            if (state.error != null) {
                Text(
                    text = state.error,
                    style = MaterialTheme.typography.labelSmall,
                    fontWeight = FontWeight.Bold,
                    color = Color(0xFFD45A7A),
                )
            }
            if (state.loading) {
                Text("Searching...", style = MaterialTheme.typography.labelSmall, color = Color(0xFF52656D))
            }
            Column(
                modifier = Modifier.fillMaxWidth(),
                horizontalAlignment = Alignment.CenterHorizontally,
                verticalArrangement = Arrangement.spacedBy(ThumbGap / 2f),
            ) {
                state.suggestions.forEach { suggestion ->
                    val detail = suggestion.distanceText
                    val friendlyName = waypointSuggestionFriendlyName(suggestion)
                    MenuPanelRow(
                        label = if (friendlyName == null) {
                            "${suggestion.identifier}  $detail"
                        } else {
                            "${suggestion.identifier}  $detail\n$friendlyName"
                        },
                        active = false,
                        enabled = true,
                        testTag = "parity:plan-insert-suggestion:${suggestion.identifier}",
                        width = ThumbSize * 3f,
                        onSelect = { onSuggestionClick(suggestion) },
                    )
                }
            }
        }
    }
}
