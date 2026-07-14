package org.aerobag.app

import android.Manifest
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
import androidx.appcompat.content.res.AppCompatResources
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
import androidx.compose.ui.graphics.drawscope.rotate
import androidx.compose.ui.graphics.drawscope.scale
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.drawscope.translate
import androidx.compose.ui.graphics.nativeCanvas
import androidx.compose.ui.graphics.lerp
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.input.key.onPreviewKeyEvent
import androidx.compose.ui.input.pointer.PointerId
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.input.pointer.pointerInteropFilter
import androidx.compose.ui.input.pointer.positionChanged
import androidx.compose.ui.layout.boundsInWindow
import androidx.compose.ui.layout.onGloballyPositioned
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalFocusManager
import androidx.compose.ui.platform.LocalSoftwareKeyboardController
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.selected
import androidx.compose.ui.semantics.testTagsAsResourceId
import androidx.compose.ui.semantics.disabled
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.SpanStyle
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
import kotlinx.coroutines.Job
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
import kotlinx.coroutines.yield
import org.aerobag.app.domain.ChartAirport
import org.aerobag.app.domain.ChartAsset
import org.aerobag.app.domain.AppState
import org.aerobag.app.domain.AirwayPresentationPlan
import org.aerobag.app.domain.AirwaySuggestion
import org.aerobag.app.domain.WaypointIdentifierSuggestion
import org.aerobag.app.domain.CoreMapViewport
import org.aerobag.app.domain.DerivedChartPageState
import org.aerobag.app.domain.FlightDataBannerModel
import org.aerobag.app.domain.FlightPlan
import org.aerobag.app.domain.FlightPlanEntryPreview
import org.aerobag.app.domain.FlightPlanUiMutation
import org.aerobag.app.domain.FlightPlanDisplayRowKind
import org.aerobag.app.domain.FlightPlanDisplayRowUiView
import org.aerobag.app.domain.FlightPlanRowActionUiView
import org.aerobag.app.domain.FlightPlanRouteSegment
import org.aerobag.app.domain.FlightPlanUiState
import org.aerobag.app.domain.GuidanceState
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
import org.aerobag.app.domain.MapFollowUiState
import org.aerobag.app.domain.MapOverlayQueryResult
import org.aerobag.app.domain.MapSelectionAction
import org.aerobag.app.domain.MapSelectionHighlight
import org.aerobag.app.domain.MapSelectionItem
import org.aerobag.app.domain.MapSelectionNavigationAction
import org.aerobag.app.domain.MapSelectionQueryResult
import org.aerobag.app.domain.MapFamilyOption
import org.aerobag.app.domain.MapViewportState
import org.aerobag.app.domain.NativeAppCoreAdapter
import org.aerobag.app.domain.NativeBindings
import org.aerobag.app.domain.NativeUiSession
import org.aerobag.app.domain.NavKvStore
import org.aerobag.app.domain.NavRef
import org.aerobag.app.domain.NavElementUiView
import org.aerobag.app.domain.OwnshipControlModel
import org.aerobag.app.domain.OwnshipMode
import org.aerobag.app.domain.OwnshipRenderState
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
import org.aerobag.app.domain.ResolvedLeg
import org.aerobag.app.domain.ResolvedLegSource
import org.aerobag.app.domain.RenderTile
import org.aerobag.app.domain.RenderTileSource
import org.aerobag.app.domain.RouteSegmentStatus
import org.aerobag.app.domain.RouteComponentViewKind
import org.aerobag.app.domain.RouteComponent
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
import org.aerobag.app.domain.UiPlaybackPanelState
import org.aerobag.app.domain.UiTheme
import org.aerobag.app.domain.UiThemeLoader
import org.aerobag.app.domain.UiSessionSnapshot
import org.aerobag.app.domain.VisibleMapFeature
import org.aerobag.app.domain.VisibleMetarFeature
import org.aerobag.app.domain.VisiblePirepFeature
import org.aerobag.app.domain.WeatherDetailUiView
import org.aerobag.app.domain.AirportNotamUiView
import org.aerobag.app.domain.applyPinchGesture
import org.aerobag.app.domain.clampZoom
import org.aerobag.app.domain.createInitialImageViewport
import org.aerobag.app.domain.createPinchSnapshot
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
import org.aerobag.app.generated.NexradOverlayTile
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.jsonPrimitive
import org.aerobag.app.generated.airportCircleMarkerPath
import org.aerobag.app.generated.airportFuelMarkerPath
import org.aerobag.app.generated.airportOpenMarkerSymbol
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
)

private data class RasterPlanFrame(
    val tiles: List<RenderTile> = emptyList(),
    val chartReferenceAction: WireChartReferenceAction? = null,
)

private const val TerrainTileBitmapCacheMaxEntries = 256
private const val NexradViewportRefreshThrottleMs = 1_000L
private const val PerfScenarioKorsOwnshipSourceId = "perf:kors-terrain-ownship"
private const val PerfScenarioKorsStressCenterLat = 48.6760
private const val PerfScenarioKorsStressCenterLon = -122.8600
private const val PerfScenarioKorsStressZoom = 10.8
private const val PerfScenarioKorsStressAltitudeMslFt = 1_000.0

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

private data class RasterTileLoadRequest(
    val id: Long,
    val mapId: String,
    val zoom: Double,
    val centerLat: Double,
    val centerLon: Double,
    val visibleTiles: List<org.aerobag.app.domain.RenderTile>,
    val missingTiles: List<org.aerobag.app.domain.RenderTile>,
    val pageTilePaintTiming: PageTilePaintTiming?,
)

@OptIn(ExperimentalComposeUiApi::class)
@Composable
internal fun MapExplorerPage(
    appCore: NativeAppCoreAdapter,
    page: AppPage,
    pageHistory: List<AppViewSnapshot>,
    uptimeLabel: String,
    uiSession: NativeUiSession,
    sessionSnapshot: UiSessionSnapshot,
    uiInvalidationRevisions: UiInvalidationRevisions,
    liveFeedGeneration: Int,
    uiTheme: UiTheme,
    ownship: OwnshipRenderState,
    flightDataBanner: FlightDataBannerModel,
    playbackUiState: PlaybackUiState,
    playbackPanelState: UiPlaybackPanelState,
    playbackSourcePath: String,
    mapFollowUiState: MapFollowUiState,
    mapFollowTargetViewport: CoreMapViewport?,
    situationRingCandidates: List<SituationRingCandidate>,
    selectedMap: RasterMapUiState,
    mapFamilyOptions: List<MapFamilyOption>,
    viewport: MapViewportState,
    decodedTileBitmapCache: DecodedTileBitmapCache,
    debugState: UiDebugState,
    perfScenario: AndroidPerfScenario? = null,
    pageTilePaintTiming: PageTilePaintTiming?,
    ownshipControls: OwnshipControlModel,
    onPageTilePaintTimingComplete: (Long) -> Unit,
    onViewportChange: (MapViewportState) -> Unit,
    onSessionSnapshotChange: (UiSessionSnapshot) -> Unit,
    onSessionCommandFailure: (Throwable) -> Unit,
    onSelectOwnshipSource: (String) -> Unit,
    onSituationControlInput: (SituationControlInput) -> Unit,
    onPlaybackSourcePathChange: (String) -> Unit,
    onSelectMapFamily: (String) -> Unit,
    onOpenChartReference: (familyId: String, suggestedChartIds: List<String>) -> Unit,
    onSelectPage: (AppPage) -> Unit,
    onOpenPlateTarget: (airportId: String, target: String, chartId: String) -> Unit,
    onOpenPlan: () -> Unit,
    navElement: NavElementUiView?,
    plan: org.aerobag.app.domain.FlightPlan,
    planUiState: FlightPlanUiState?,
) {
    val context = LocalContext.current
    val activity = context as? MainActivity
    val density = LocalDensity.current
    val json = remember { Json { ignoreUnknownKeys = true } }
    val sessionWorkRunner = remember(uiSession) { UiSessionWorkRunner(uiSession) }
    DisposableEffect(sessionWorkRunner) {
        onDispose {
            sessionWorkRunner.close()
        }
    }
    val devServerBaseUrl = remember(context) { loadAndroidDevServerBaseUrl(context.applicationContext) }
    fun applySessionCommand(commandName: String, operation: () -> UiSessionSnapshot): UiSessionSnapshot? =
        try {
            operation().also(onSessionSnapshotChange)
        } catch (error: CancellationException) {
            throw error
        } catch (error: Throwable) {
            Log.w("AerobagSessionCommand", "map command failed command=$commandName", error)
            onSessionCommandFailure(error)
            null
        }
    val focusRequester = remember { FocusRequester() }
    var chartTrayOpen by remember { mutableStateOf(false) }
    var layerTrayOpen by remember { mutableStateOf(false) }
    var dataStatusTrayOpen by remember { mutableStateOf(false) }
    var situationTrayOpen by remember { mutableStateOf(false) }
    var chartSearchText by remember { mutableStateOf("") }
    var chartSearchOpen by remember { mutableStateOf(false) }
    var chartSearchLoading by remember { mutableStateOf(false) }
    var chartSearchError by remember { mutableStateOf<String?>(null) }
    var chartSearchSuggestions by remember { mutableStateOf<List<WaypointIdentifierSuggestion>>(emptyList()) }
    var mapSelection by remember { mutableStateOf<MapSelectionUiState?>(null) }
    var mapSurfaceBounds by remember { mutableStateOf<Rect?>(null) }
    var mapSelectionTrayBounds by remember { mutableStateOf<Rect?>(null) }
    var surfaceSize by remember { mutableStateOf(IntSize.Zero) }
    var committedMapOverlay by remember(uiSession) {
        mutableStateOf(
            MapOverlayQueryResult(
                visibleFeatures = emptyList(),
                visibleMetars = emptyList(),
                visiblePireps = emptyList(),
                airspacePaths = emptyList(),
                tfrPaths = emptyList(),
                airspaceLabels = emptyList(),
                offlineRegions = emptyList(),
            ),
        )
    }
    var committedOverlayViewport by remember(uiSession) { mutableStateOf<MapViewportState?>(null) }
    var committedOverlaySurfaceUnits by remember(uiSession) { mutableStateOf<OverlaySurfaceUnits?>(null) }
    var mapOverlayError by remember(uiSession) { mutableStateOf<String?>(null) }
    var nexradFrame by remember(uiSession) { mutableStateOf<NexradOverlayFrame?>(null) }
    var terrainOverlay by remember(uiSession) { mutableStateOf<List<TerrainOverlayImage>>(emptyList()) }
    var terrainOverlayError by remember(uiSession) { mutableStateOf<String?>(null) }
    var terrainLastQueryDiagnostics by remember(uiSession) { mutableStateOf(TerrainOverlayDiagnostics()) }
    var terrainNoPaintStartedMs by remember(uiSession) { mutableLongStateOf(0L) }
    var terrainNoPaintLastWarningMs by remember(uiSession) { mutableLongStateOf(0L) }
    var terrainStaleNoPositionWarningLastMs by remember(uiSession) { mutableLongStateOf(0L) }
    val terrainTileBitmapCache = remember(uiSession) {
        LinkedHashMap<String, androidx.compose.ui.graphics.ImageBitmap>(
            TerrainTileBitmapCacheMaxEntries,
            0.75f,
            true,
        )
    }
    val terrainTileInFlightKeys = remember(uiSession) { mutableSetOf<String>() }
    val terrainRenderRequests = remember(uiSession) {
        Channel<Unit>(Channel.CONFLATED)
    }
    val nexradRenderRequests = remember(uiSession) {
        Channel<Unit>(Channel.CONFLATED)
    }
    val nexradViewportRefreshRequests = remember(uiSession) {
        Channel<Unit>(Channel.CONFLATED)
    }
    DisposableEffect(terrainRenderRequests, nexradRenderRequests, nexradViewportRefreshRequests) {
        onDispose {
            terrainRenderRequests.close()
            nexradRenderRequests.close()
            nexradViewportRefreshRequests.close()
        }
    }
    var flightPlanRoute by remember(plan.id, plan.version) { mutableStateOf<List<FlightPlanRouteSegment>>(emptyList()) }
    var mapGestureActive by remember { mutableStateOf(false) }
    val selectedMapId = selectedMap.selectedMapId
    val selectedFamilyId = selectedMap.selectedFamilyId
    val viewportState = remember(selectedMapId) { mutableStateOf(viewport) }
    val followTargetGate = remember(uiSession) { MapFollowTargetGate() }
    var viewportSyncPending by remember(selectedMapId) { mutableStateOf(false) }
    LaunchedEffect(viewport, selectedMapId) {
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
    val mapDisplayScale = density.density.toDouble().takeIf { it.isFinite() && it > 0.0 } ?: 1.0
    val interactiveMaxZoom = physicalDisplayMaxZoom(selectedMap.maxZoom, mapDisplayScale)
    val mapLayerState = sessionSnapshot.mapLayerState
    val terrainViewportState = rememberUpdatedState(currentViewport)
    val terrainSurfaceSizeState = rememberUpdatedState(surfaceSize)
    val terrainSurfaceWidthPxState = rememberUpdatedState(surfaceWidthPx)
    val terrainSurfaceHeightPxState = rememberUpdatedState(surfaceHeightPx)
    val terrainMapVisibleState = rememberUpdatedState(page == AppPage.Map)
    val nexradViewportState = rememberUpdatedState(currentViewport)
    val nexradSurfaceSizeState = rememberUpdatedState(surfaceSize)
    val nexradSurfaceWidthPxState = rememberUpdatedState(surfaceWidthPx)
    val nexradSurfaceHeightPxState = rememberUpdatedState(surfaceHeightPx)
    val nexradVisibleState = rememberUpdatedState(page == AppPage.Map && mapLayerState.nexrad.visible)
    val nexradEnabledState = rememberUpdatedState(mapLayerState.nexrad.enabled)
    val nexradDevServerBaseUrlState = rememberUpdatedState(devServerBaseUrl)
    val nexradFrameState = rememberUpdatedState(nexradFrame)
    val terrainOverlayImageCountState = rememberUpdatedState(terrainOverlay.size)
    val terrainLastQueryDiagnosticsState = rememberUpdatedState(terrainLastQueryDiagnostics)
    val terrainOwnshipAltitudeBucketState = rememberUpdatedState(ownship.terrainAltitudeBucketFt)
    val terrainOwnshipPositionState = rememberUpdatedState(ownship.position)
    val terrainDataStatusState = rememberUpdatedState(sessionSnapshot.dataStatusState)
    val terrainOwnshipLauncherLabelState = rememberUpdatedState(ownshipControls.launcherLabel)
    val surfaceWidthDp = remember(surfaceSize, density) { with(density) { surfaceSize.width.toDp().value } }
    val surfaceHeightDp = remember(surfaceSize, density) { with(density) { surfaceSize.height.toDp().value } }
    val situationDockLowered = surfaceWidthDp.dp < SituationDockOverlapWidth
    val situationDockTopPadding =
        if (situationDockLowered) ThumbSize + (ThumbGap * 2f) else ThumbGap
    val rasterPlanFrame = remember(selectedMapId, currentViewport, surfaceSize, mapDisplayScale, uiSession, debugState.fastTiles) {
        if (surfaceSize.width == 0 || surfaceSize.height == 0) {
            RasterPlanFrame()
        } else {
            val planStartMs = SystemClock.elapsedRealtime()
            val plan = json.decodeFromString<WireRasterTilePlan>(
                uiSession.queryRasterTilePlanJson(
                    currentViewport,
                    surfaceWidthDp.toDouble(),
                    surfaceHeightDp.toDouble(),
                    mapDisplayScale,
                ),
            )
            val planMs = SystemClock.elapsedRealtime() - planStartMs
            pageTilePaintTiming?.let { timing ->
                perfLogInfo(TileBudgetLogTag) {
                    "tile-paint-plan id=${timing.id} trigger=${timing.trigger} from=${timing.fromPage} elapsedMs=${SystemClock.elapsedRealtime() - timing.startedMs} planMs=$planMs tiles=${plan.tiles.size} fastTiles=${debugState.fastTiles}"
                }
            }
            val renderTiles = plan.tiles.mapNotNull { tile ->
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
            }
            RasterPlanFrame(renderTiles, plan.chart_reference_action)
        }
    }
    val tiles = rasterPlanFrame.tiles
    val chartReferenceAction = rasterPlanFrame.chartReferenceAction
    val terrainVisibleState = rememberUpdatedState(mapLayerState.terrainWarning.visible)
    LaunchedEffect(uiSession) {
        while (true) {
            delay(10_000)
            val nowMs = SystemClock.elapsedRealtime()
            val latestSurfaceSize = terrainSurfaceSizeState.value
            val altitudeBucketFt = terrainOwnshipAltitudeBucketState.value
            val ownshipPosition = terrainOwnshipPositionState.value
            val diagnostics = terrainLastQueryDiagnosticsState.value
            val noPositionTerrainWarning = terrainDataStatusState.value.boxes.firstOrNull { box ->
                box.id == TerrainWarningStatusId && box.detail.contains(TerrainNoPositionWarningDetail)
            }
            if (ownshipPosition != null && altitudeBucketFt != null && noPositionTerrainWarning != null) {
                if (terrainStaleNoPositionWarningLastMs == 0L || nowMs - terrainStaleNoPositionWarningLastMs >= 60_000L) {
                    terrainStaleNoPositionWarningLastMs = nowMs
                    Log.w(
                        MapLayerLogTag,
                        "terrain stale-no-position-warning-with-ownship " +
                            "ownship=${ownshipPosition.lat},${ownshipPosition.lon} " +
                            "ownshipAltitudeBucketFt=$altitudeBucketFt " +
                            "ownshipLauncher=${terrainOwnshipLauncherLabelState.value} " +
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
                terrainStaleNoPositionWarningLastMs = 0L
            }
            val shouldHaveTerrain =
                terrainMapVisibleState.value &&
                    terrainVisibleState.value &&
                    latestSurfaceSize.width > 0 &&
                    latestSurfaceSize.height > 0 &&
                    ownshipPosition != null &&
                    altitudeBucketFt != null
            if (!shouldHaveTerrain || terrainOverlayImageCountState.value > 0) {
                terrainNoPaintStartedMs = 0L
                terrainNoPaintLastWarningMs = 0L
                continue
            }
            if (terrainNoPaintStartedMs == 0L) {
                terrainNoPaintStartedMs = nowMs
                continue
            }
            val noPaintMs = nowMs - terrainNoPaintStartedMs
            if (noPaintMs < 60_000L) {
                continue
            }
            if (terrainNoPaintLastWarningMs != 0L && nowMs - terrainNoPaintLastWarningMs < 60_000L) {
                continue
            }
            terrainNoPaintLastWarningMs = nowMs
            Log.w(
                MapLayerLogTag,
                "terrain no-paint-with-altitude durationMs=$noPaintMs " +
                    "ownship=${ownshipPosition.lat},${ownshipPosition.lon} " +
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
    val menuTrayOpen = chartTrayOpen || layerTrayOpen || dataStatusTrayOpen || situationTrayOpen
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
                onSelectMapFamily(option.id)
            }
        }
    }
    val layerTrayOptions = remember(mapLayerState) {
        listOf(
            MenuDockOption(
                key = "metars",
                label = "Observations",
                enabled = mapLayerState.metars.enabled,
                disabledReason = mapLayerState.metars.disabledReason,
                toggleState = mapLayerState.metars,
                iconResId = mapLayerIconResId(MapLayerId.Metars),
            ) {
                val visible = !mapLayerState.metars.visible
                val startMs = SystemClock.elapsedRealtime()
                if (applySessionCommand("setMapLayerVisibility") {
                        uiSession.setMapLayerVisibility(MapLayerId.Metars, visible)
                    } != null) {
                    diagnosticLogInfo(MapLayerLogTag) {
                        "toggle layer=metars visible=$visible coreMs=${SystemClock.elapsedRealtime() - startMs}"
                    }
                }
            },
            MenuDockOption(
                key = "vectors",
                label = "Vectors",
                enabled = mapLayerState.vectors.enabled,
                disabledReason = mapLayerState.vectors.disabledReason,
                toggleState = mapLayerState.vectors,
                iconResId = mapLayerIconResId(MapLayerId.Vectors),
            ) {
                val visible = !mapLayerState.vectors.visible
                val startMs = SystemClock.elapsedRealtime()
                if (applySessionCommand("setMapLayerVisibility") {
                        uiSession.setMapLayerVisibility(MapLayerId.Vectors, visible)
                    } != null) {
                    diagnosticLogInfo(MapLayerLogTag) {
                        "toggle layer=vectors visible=$visible coreMs=${SystemClock.elapsedRealtime() - startMs}"
                    }
                }
            },
            MenuDockOption(
                key = "nexrad",
                label = "NEXRAD",
                enabled = mapLayerState.nexrad.enabled,
                disabledReason = mapLayerState.nexrad.disabledReason,
                toggleState = mapLayerState.nexrad,
                iconResId = mapLayerIconResId(MapLayerId.Nexrad),
            ) {
                val visible = !mapLayerState.nexrad.visible
                val startMs = SystemClock.elapsedRealtime()
                if (applySessionCommand("setMapLayerVisibility") {
                        uiSession.setMapLayerVisibility(MapLayerId.Nexrad, visible)
                    } != null) {
                    diagnosticLogInfo(MapLayerLogTag) {
                        "toggle layer=nexrad visible=$visible coreMs=${SystemClock.elapsedRealtime() - startMs}"
                    }
                }
            },
            MenuDockOption(
                key = "terrain_warning",
                label = "Terrain Warning",
                enabled = mapLayerState.terrainWarning.enabled,
                disabledReason = mapLayerState.terrainWarning.disabledReason,
                toggleState = mapLayerState.terrainWarning,
                iconResId = mapLayerIconResId(MapLayerId.TerrainWarning),
            ) {
                val visible = !mapLayerState.terrainWarning.visible
                val startMs = SystemClock.elapsedRealtime()
                if (applySessionCommand("setMapLayerVisibility") {
                        uiSession.setMapLayerVisibility(MapLayerId.TerrainWarning, visible)
                    } != null) {
                    diagnosticLogInfo(MapLayerLogTag) {
                        "toggle layer=terrain_warning visible=$visible coreMs=${SystemClock.elapsedRealtime() - startMs}"
                    }
                }
            },
            MenuDockOption(
                key = "world_basemap",
                label = "World Map",
                enabled = mapLayerState.worldBasemap.enabled,
                disabledReason = mapLayerState.worldBasemap.disabledReason,
                toggleState = mapLayerState.worldBasemap,
                iconResId = mapLayerIconResId(MapLayerId.WorldBasemap),
            ) {
                val visible = !mapLayerState.worldBasemap.visible
                val startMs = SystemClock.elapsedRealtime()
                if (applySessionCommand("setMapLayerVisibility") {
                        uiSession.setMapLayerVisibility(MapLayerId.WorldBasemap, visible)
                    } != null) {
                    diagnosticLogInfo(MapLayerLogTag) {
                        "toggle layer=world_basemap visible=$visible coreMs=${SystemClock.elapsedRealtime() - startMs}"
                    }
                }
            },
            MenuDockOption(
                key = "offline_regions",
                label = "Offline Regions",
                enabled = mapLayerState.offlineRegions.enabled,
                disabledReason = mapLayerState.offlineRegions.disabledReason,
                toggleState = mapLayerState.offlineRegions,
                iconResId = mapLayerIconResId(MapLayerId.OfflineRegions),
            ) {
                val visible = !mapLayerState.offlineRegions.visible
                val startMs = SystemClock.elapsedRealtime()
                if (applySessionCommand("setMapLayerVisibility") {
                        uiSession.setMapLayerVisibility(MapLayerId.OfflineRegions, visible)
                    } != null) {
                    diagnosticLogInfo(MapLayerLogTag) {
                        "toggle layer=offline_regions visible=$visible coreMs=${SystemClock.elapsedRealtime() - startMs}"
                    }
                }
            },
        )
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
    val situationOverlay = remember(ownship, currentViewport, surfaceWidthPx, surfaceHeightPx) {
        resolveSituationOverlay(
            ownship = ownship,
            viewport = currentViewport,
            widthUnits = surfaceWidthPx,
            heightUnits = surfaceHeightPx,
            ringCandidates = situationRingCandidates,
        )
    }
    val mapFollowProbeTag = remember(
        mapFollowUiState.following,
        ownship.drawAircraft,
        ownship.position,
        currentViewport,
        surfaceWidthPx,
        surfaceHeightPx,
    ) {
        val position = ownship.position
        if (ownship.drawAircraft && position != null && surfaceWidthPx > 0f && surfaceHeightPx > 0f) {
            buildMapFollowProbeTag(
                following = mapFollowUiState.following,
                ownshipPosition = position,
                viewport = currentViewport,
                surfaceWidthPx = surfaceWidthPx,
                surfaceHeightPx = surfaceHeightPx,
            )
        } else {
            null
        }
    }
    fun syncFollowStateForViewport(nextViewport: MapViewportState) {
        if (!mapFollowUiState.following || surfaceWidthPx <= 0f || surfaceHeightPx <= 0f) {
            return
        }
        followTargetGate.beginSync(nextViewport)
        runCatching {
            uiSession.syncMapFollow(
                nextViewport,
                surfaceWidthPx.toDouble(),
                surfaceHeightPx.toDouble(),
            )
        }.onSuccess { snapshot ->
            followTargetGate.acknowledgeSyncSnapshot(
                following = snapshot.mapFollowUiState.following,
                targetViewport = snapshot.mapFollowTargetViewport?.let(::mapViewportFromCore),
            )
            onSessionSnapshotChange(snapshot)
        }
            .onFailure {
                followTargetGate.clear()
                Log.w(MapViewportLogTag, "map follow sync failed", it)
            }
    }

    fun updateViewport(nextViewport: MapViewportState, syncFollow: Boolean = true) {
        perfLogInfo(MapViewportLogTag) {
            "update map=$selectedMapId from=${"%.2f".format(viewportState.value.zoom)} to=${"%.2f".format(nextViewport.zoom)} fromCenter=${"%.3f".format(viewportState.value.centerWorldX)},${"%.3f".format(viewportState.value.centerWorldY)} toCenter=${"%.3f".format(nextViewport.centerWorldX)},${"%.3f".format(nextViewport.centerWorldY)} syncFollow=$syncFollow"
        }
        viewportState.value = nextViewport
        viewportSyncPending = true
        onViewportChange(nextViewport)
        if (syncFollow) {
            syncFollowStateForViewport(nextViewport)
        }
    }

    fun currentPerfCacheStats(): AndroidPerfCacheStats {
        val rasterStats = decodedTileBitmapCache.stats()
        val (terrainEntries, terrainBytes) = terrainBitmapCacheStats(terrainTileBitmapCache)
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

    var perfScenarioStarted by remember(perfScenario?.id, uiSession) { mutableStateOf(false) }
    LaunchedEffect(perfScenario?.id, uiSession, surfaceSize, selectedMapId) {
        val scenario = perfScenario ?: return@LaunchedEffect
        if (scenario.id != AndroidPerfScenarioMapSelectionFreeze || perfScenarioStarted) {
            return@LaunchedEffect
        }
        if (surfaceWidthPx <= 0f || surfaceHeightPx <= 0f) {
            return@LaunchedEffect
        }
        perfScenarioStarted = true
        val watchdog = AndroidMainThreadStallWatchdog(scenario)
        watchdog.start()
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
                    pointDisplayScale = density.density.toDouble(),
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
                mapSelection = MapSelectionUiState(
                    point = Offset(surfaceWidthPx / 2f, surfaceHeightPx / 2f),
                    result = result,
                    selectedItem = null,
                )
            }

            Log.i(
                AndroidPerfScenarioTag,
                "start scenario=${scenario.id} surface=${surfaceSize.width}x${surfaceSize.height} density=${density.density} map=$selectedMapId",
            )
            val sfo = latLonToWorld(37.6213, -122.3790)
            val baseViewport = MapViewportState(
                centerWorldX = sfo.x,
                centerWorldY = sfo.y,
                zoom = clampZoom(9.8, selectedMap.minZoom, interactiveMaxZoom),
            )
            updateViewport(baseViewport, syncFollow = false)
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
                    pointDisplayScale = density.density.toDouble(),
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
                updateViewport(lastViewport, syncFollow = false)
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
            watchdog.stop()
        }
    }

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
            onSessionSnapshotChange(scenarioSnapshot)
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
            updateViewport(baseViewport, syncFollow = false)
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
                updateViewport(nextViewport, syncFollow = false)
                nexradRenderRequests.trySend(Unit)
                terrainRenderRequests.trySend(Unit)
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

    fun mapSelectionItemById(result: MapSelectionQueryResult, itemId: String?): MapSelectionItem? {
        if (itemId == null) {
            return null
        }
        return result.categories
            .asSequence()
            .flatMap { it.items.asSequence() }
            .firstOrNull { it.id == itemId }
    }

    fun inspectNavRef(navRef: NavRef) {
        if (surfaceWidthPx <= 0f || surfaceHeightPx <= 0f) {
            recenterOnNavRef(navRef)
            return
        }
        sessionWorkRunner.submitMapSelectionForNavRef(
            viewport = currentViewport,
            widthPx = surfaceWidthPx.toDouble(),
            heightPx = surfaceHeightPx.toDouble(),
            navRef = navRef,
            pointDisplayScale = density.density.toDouble(),
            onResult = { inspection ->
                val center = latLonToWorld(inspection.position.lat, inspection.position.lon)
                val nextViewport = currentViewport.copy(
                    centerWorldX = center.x,
                    centerWorldY = center.y,
                    zoom = inspection.targetZoom,
                )
                updateViewport(nextViewport)
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
                )
                chartTrayOpen = false
                layerTrayOpen = false
                dataStatusTrayOpen = false
                situationTrayOpen = false
                chartSearchText = ""
                chartSearchOpen = false
                chartSearchLoading = false
                chartSearchError = null
                chartSearchSuggestions = emptyList()
            },
            onError = { error ->
                chartSearchLoading = false
                chartSearchError = "Search failed: ${error.message ?: error.toString()}"
            },
        )
    }

    fun submitChartSearch() {
        val query = chartSearchText.trim().uppercase()
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
        val prefix = chartSearchText.trim().uppercase()
        if (prefix.isBlank()) {
            chartSearchLoading = false
            chartSearchError = null
            chartSearchSuggestions = emptyList()
            return@LaunchedEffect
        }
        chartSearchLoading = true
        chartSearchError = null
        val (centerLat, centerLon) = viewportCenterLatLon(currentViewport)
        runCatching {
            withContext(Dispatchers.IO) {
                appCore.suggestWaypointIdentifiersNear(
                    anchor = LatLonPoint(centerLat, centerLon),
                    prefix = prefix,
                    limit = 8,
                )
            }
        }.onSuccess { suggestions ->
            chartSearchLoading = false
            chartSearchSuggestions = suggestions
        }.onFailure { error ->
            chartSearchLoading = false
            chartSearchSuggestions = emptyList()
            chartSearchError = error.message ?: error.toString()
        }
    }

    val aircraftDrawable = remember(context) { AppCompatResources.getDrawable(context, R.drawable.plan_view_icon)?.mutate() }
    val outlinePaint = remember {
        Paint().apply {
            isAntiAlias = true
            color = android.graphics.Color.argb(102, 0, 0, 0)
            style = Paint.Style.STROKE
            strokeCap = Paint.Cap.ROUND
            strokeJoin = Paint.Join.ROUND
        }
    }
    val fillPaint = remember {
        Paint().apply {
            isAntiAlias = true
            color = android.graphics.Color.WHITE
            style = Paint.Style.FILL
        }
    }
    val labelStrokePaint = remember {
        Paint().apply {
            isAntiAlias = true
            color = android.graphics.Color.argb(102, 0, 0, 0)
            style = Paint.Style.STROKE
            strokeJoin = Paint.Join.ROUND
            strokeWidth = 5f
            textAlign = Paint.Align.CENTER
            textSize = 16f
            typeface = android.graphics.Typeface.create(android.graphics.Typeface.DEFAULT_BOLD, android.graphics.Typeface.BOLD)
        }
    }
    val labelFillPaint = remember {
        Paint().apply {
            isAntiAlias = true
            color = android.graphics.Color.WHITE
            style = Paint.Style.FILL
            textAlign = Paint.Align.CENTER
            textSize = 16f
            typeface = android.graphics.Typeface.create(android.graphics.Typeface.DEFAULT_BOLD, android.graphics.Typeface.BOLD)
        }
    }
    val tileBitmapCache = remember(selectedMapId, debugState.fastTiles) {
        mutableStateMapOf<org.aerobag.app.domain.RenderTileKey, androidx.compose.ui.graphics.ImageBitmap?>()
    }
    val visibleTileKeys = remember(tiles) {
        tiles.mapTo(LinkedHashSet()) { tile -> renderTileKey(tile) }
    }
    val latestVisibleTileKeysState = rememberUpdatedState(visibleTileKeys)
    val rasterTileBitmapLoaderScope = rememberCoroutineScope()
    val rasterTileBitmapLoader = remember(context.applicationContext, rasterTileBitmapLoaderScope) {
        RasterTileBitmapLoader(context.applicationContext, rasterTileBitmapLoaderScope)
    }
    val rasterTileLoadRequests = remember { Channel<RasterTileLoadRequest>(Channel.CONFLATED) }
    var nextRasterTileLoadRequestId by remember { mutableLongStateOf(1L) }
    var latestRasterTileLoadRequestId by remember { mutableLongStateOf(0L) }
    val latestRasterTileLoadRequestIdState = rememberUpdatedState(latestRasterTileLoadRequestId)
    DisposableEffect(rasterTileBitmapLoader) {
        onDispose {
            rasterTileLoadRequests.close()
            rasterTileBitmapLoader.close()
        }
    }
    LaunchedEffect(tiles, selectedMapId, debugState.fastTiles) {
        val staleLocalKeys = tileBitmapCache.keys.filter { key -> key !in visibleTileKeys }
        staleLocalKeys.forEach { key -> tileBitmapCache.remove(key) }
        var decodedCacheHits = 0
        tiles.forEach { tile ->
            val renderKey = renderTileKey(tile)
            if (!tileBitmapCache.containsKey(renderKey)) {
                val bitmap = decodedTileBitmapCache.get(decodedTileCacheKey(tile))
                if (bitmap != null) {
                    tileBitmapCache[renderKey] = bitmap
                    decodedCacheHits += 1
                }
            }
        }
        val missingTiles = tiles.filter { tile -> !tileBitmapCache.containsKey(renderTileKey(tile)) }
        perfLogInfo(TileBudgetLogTag) {
            val decodedCacheStats = decodedTileBitmapCache.stats()
            val (localBitmapEntries, localBitmapBytes) = rasterLocalBitmapCacheStats(tileBitmapCache)
            "visible map=$selectedMapId total=${tiles.size} missing=${missingTiles.size} localCache=${tileBitmapCache.size}/${localBitmapBytes}B localBitmaps=$localBitmapEntries pruned=${staleLocalKeys.size} decodedLru=${decodedCacheStats.entries}/${decodedCacheStats.bytes}B lruHits=$decodedCacheHits fastTiles=${debugState.fastTiles} groups=[${formatTileBudgetSummary(tiles)}]"
        }
        if (missingTiles.isEmpty()) {
            pageTilePaintTiming?.takeIf { tiles.isNotEmpty() }?.let { timing ->
                withFrameNanos { }
                perfLogInfo(TileBudgetLogTag) {
                    "tile-paint-frame id=${timing.id} trigger=${timing.trigger} from=${timing.fromPage} elapsedMs=${SystemClock.elapsedRealtime() - timing.startedMs} cacheOnly=true"
                }
                onPageTilePaintTimingComplete(timing.id)
            }
            return@LaunchedEffect
        }
        val (viewportLat, viewportLon) = viewportCenterLatLon(currentViewport)
        val requestId = nextRasterTileLoadRequestId++
        latestRasterTileLoadRequestId = requestId
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
        if (rasterTileLoadRequests.trySend(request).isFailure) {
            Log.w(TileBudgetLogTag, "tile-load-request-drop request=$requestId map=$selectedMapId")
        }
    }
    LaunchedEffect(rasterTileBitmapLoader, tileBitmapCache, selectedMapId, debugState.fastTiles) {
        for (initialRequest in rasterTileLoadRequests) {
            var request = initialRequest
            while (true) {
                val loadStartMs = SystemClock.elapsedRealtime()
                val generationId = TileLoadGenerationIds.incrementAndGet()
                perfLogInfo(TileBudgetLogTag) {
                    "generation-start gen=$generationId request=${request.id} map=${request.mapId} zoom=${"%.2f".format(request.zoom)} center=${"%.3f".format(request.centerLat)},${"%.3f".format(request.centerLon)} total=${request.visibleTiles.size} missing=${request.missingTiles.size} cache=${tileBitmapCache.size}"
                }
                var loadedThisPassCount = 0
                val loadedTiles = try {
                    rasterTileBitmapLoader.loadVisibleTileBitmaps(
                        request.mapId,
                        generationId,
                        request.missingTiles,
                    ) { loaded ->
                        if (request.id != latestRasterTileLoadRequestIdState.value || loaded.result.key !in latestVisibleTileKeysState.value) {
                            return@loadVisibleTileBitmaps
                        }
                        tileBitmapCache[loaded.result.key] = loaded.result.bitmap
                        val bitmap = loaded.result.bitmap
                        if (bitmap != null) {
                            loadedThisPassCount += 1
                            decodedTileBitmapCache.put(decodedTileCacheKey(loaded.tile), bitmap, loaded.result.decodedBytes)
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
                val staleRequest = request.id != latestRasterTileLoadRequestIdState.value
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
                    onPageTilePaintTimingComplete(timing.id)
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
                val nextRequest = rasterTileLoadRequests.tryReceive().getOrNull() ?: break
                perfLogInfo(TileBudgetLogTag) {
                    "tile-load-coalesce fromRequest=${request.id} toRequest=${nextRequest.id} map=${nextRequest.mapId}"
                }
                request = nextRequest
            }
        }
    }
    val tileLabelPaint = remember {
        Paint().apply {
            isAntiAlias = true
            color = android.graphics.Color.WHITE
            textSize = 24f
            typeface = android.graphics.Typeface.create(android.graphics.Typeface.MONOSPACE, android.graphics.Typeface.BOLD)
        }
    }
    val tileLabelBackgroundPaint = remember {
        Paint().apply {
            isAntiAlias = true
            color = android.graphics.Color.argb(224, 14, 22, 28)
        }
    }
    val fixMarkerStrokeColor = Color(0xB3081218)
    val fixMarkerFillColor = uiTheme.aviation.intersectionCyan
    val airportMarkerStrokeColor = Color(0xB3081218)
    val airportToweredFillColor = uiTheme.aviation.classBDBlue
    val airportUntoweredFillColor = uiTheme.aviation.classCMagenta
    val vorMarkerColor = uiTheme.aviation.classBDBlue
    val vorMarkerStrokeColor = Color(0xD1081218)
    val fixLabelStrokePaint = remember {
        Paint().apply {
            isAntiAlias = true
            color = android.graphics.Color.argb(179, 8, 18, 24)
            style = Paint.Style.STROKE
            strokeJoin = Paint.Join.ROUND
            strokeWidth = 4f
            textAlign = Paint.Align.CENTER
            textSize = 14f
            typeface = android.graphics.Typeface.create(android.graphics.Typeface.DEFAULT_BOLD, android.graphics.Typeface.BOLD)
        }
    }
    val airportLabelStrokePaint = remember {
        Paint().apply {
            isAntiAlias = true
            color = android.graphics.Color.argb(179, 8, 18, 24)
            style = Paint.Style.STROKE
            strokeJoin = Paint.Join.ROUND
            strokeWidth = 3f
            textAlign = Paint.Align.CENTER
            textSize = 14f
            typeface = Typeface.create(Typeface.DEFAULT_BOLD, Typeface.BOLD)
        }
    }
    val vorLabelFillPaint = remember {
        Paint().apply {
            isAntiAlias = true
            color = android.graphics.Color.WHITE
            style = Paint.Style.FILL
            textAlign = Paint.Align.CENTER
            textSize = 14f
            typeface = Typeface.create(Typeface.DEFAULT_BOLD, Typeface.BOLD)
        }
    }
    val fixLabelFillPaint = remember {
        Paint().apply {
            isAntiAlias = true
            color = android.graphics.Color.WHITE
            style = Paint.Style.FILL
            textAlign = Paint.Align.CENTER
            textSize = 14f
            typeface = Typeface.create(Typeface.DEFAULT_BOLD, Typeface.BOLD)
        }
    }
    val airportToweredLabelFillPaint = remember {
        Paint().apply {
            isAntiAlias = true
            color = android.graphics.Color.WHITE
            style = Paint.Style.FILL
            textAlign = Paint.Align.CENTER
            textSize = 14f
            typeface = Typeface.create(Typeface.DEFAULT_BOLD, Typeface.BOLD)
        }
    }
    val airportUntoweredLabelFillPaint = remember {
        Paint().apply {
            isAntiAlias = true
            color = android.graphics.Color.WHITE
            style = Paint.Style.FILL
            textAlign = Paint.Align.CENTER
            textSize = 14f
            typeface = Typeface.create(Typeface.DEFAULT_BOLD, Typeface.BOLD)
        }
    }

    LaunchedEffect(selectedMapId) {
        chartTrayOpen = false
        layerTrayOpen = false
        dataStatusTrayOpen = false
        situationTrayOpen = false
        mapSelection = null
    }
    LaunchedEffect(uiSession, plan.id, plan.version, plan.guidance, plan.resolvedLegs, uiInvalidationRevisions.flightPlanRoute) {
        runCatching {
            uiSession.projectFlightPlanRoute()
        }.onSuccess {
            flightPlanRoute = it
            val guidance = sessionSnapshot.appUiState.activePlan?.guidance
            val directTo = guidance?.directTo
            Log.i(
                "AerobagGuidance",
                "route projection revision=${sessionSnapshot.sessionRevision} " +
                    "mode=${guidance?.sequencingMode} " +
                    "activeLeg=${guidance?.activeLegIndex} " +
                    "activeSummary=${guidance?.navElement?.activeLegSummary} " +
                    "cdi=${guidance?.navElement?.cdiIndicatorDots} " +
                    "directTarget=${directTo?.target} " +
                    "directTargetLeg=${directTo?.targetLegId} " +
                    "directResume=${directTo?.resumeLegId} " +
                    "directTargetComponent=${directTo?.targetComponentUid} " +
                    "statuses=${it.joinToString(",") { segment -> "${segment.id}:${segment.status}" }}",
            )
        }.onFailure {
            flightPlanRoute = emptyList()
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
    LaunchedEffect(mapFollowUiState.following, mapFollowTargetViewport, mapGestureActive) {
        if (!mapFollowUiState.following) {
            followTargetGate.clear()
            return@LaunchedEffect
        }
        if (mapGestureActive) {
            return@LaunchedEffect
        }
        val target = mapFollowTargetViewport ?: return@LaunchedEffect
        val nextViewport = mapViewportFromCore(target)
        val awaitedViewport = followTargetGate.awaitedViewport()
        if (!followTargetGate.shouldApplyTarget(nextViewport)) {
            perfLogInfo(MapViewportLogTag) {
                "follow-target stale targetZoom=${"%.2f".format(nextViewport.zoom)} awaitedZoom=${awaitedViewport?.zoom?.let { "%.2f".format(it) }}"
            }
            return@LaunchedEffect
        }
        if (!sameMapViewport(nextViewport, viewportState.value)) {
            updateViewport(nextViewport, syncFollow = false)
        }
    }
    LaunchedEffect(uiSession, liveFeedGeneration, uiInvalidationRevisions.mapOverlay, currentViewport, surfaceSize, density.density, mapLayerState.vectors.visible, mapLayerState.metars.visible, mapLayerState.offlineRegions.visible, devServerBaseUrl) {
        if (surfaceSize.width <= 0 || surfaceSize.height <= 0) {
            mapOverlayError = null
            return@LaunchedEffect
        }
        val overlayWidthPx = surfaceSize.width.toFloat()
        val overlayHeightPx = surfaceSize.height.toFloat()
        val overlayStartMs = SystemClock.elapsedRealtime()
        sessionWorkRunner.submitOverlay(
            viewport = currentViewport,
            widthPx = overlayWidthPx.toDouble(),
            heightPx = overlayHeightPx.toDouble(),
            pointDisplayScale = density.density.toDouble(),
            fetchResource = { resource ->
                fetchMapOverlayCoreResource(context, resource, devServerBaseUrl)
            },
            onResult = { outcome ->
                if (outcome.invalidations.contains("session_snapshot")) {
                    applySessionCommand("refreshSnapshot") { uiSession.refreshSnapshot() }
                }
                val overlay = outcome.overlay
                perfLogInfo(MapLayerLogTag) {
                    val (centerLat, centerLon) = viewportCenterLatLon(currentViewport)
                    "overlay center=${"%.3f".format(centerLat)},${"%.3f".format(centerLon)} zoom=${"%.2f".format(currentViewport.zoom)} size=${surfaceSize.width}x${surfaceSize.height} vectorsVisible=${mapLayerState.vectors.visible} metarsVisible=${mapLayerState.metars.visible} offlineRegionsVisible=${mapLayerState.offlineRegions.visible} features=${overlay.visibleFeatures.size} airspace=${overlay.airspacePaths.size} airspaceLabels=${overlay.airspaceLabels.size} offlineRegions=${overlay.offlineRegions.size} metars=${overlay.visibleMetars.size} pireps=${overlay.visiblePireps.size} invalidations=${outcome.invalidations} elapsedMs=${SystemClock.elapsedRealtime() - overlayStartMs}"
                }
                committedMapOverlay = outcome.overlay
                committedOverlayViewport = currentViewport
                committedOverlaySurfaceUnits = OverlaySurfaceUnits(overlayWidthPx, overlayHeightPx)
                mapOverlayError = null
            },
            onError = { error ->
                mapOverlayError = error.message ?: error::class.java.simpleName
                Log.e(MapLayerLogTag, "overlay failed: $mapOverlayError", error)
            }
        )
    }
    LaunchedEffect(uiSession, nexradRenderRequests) {
        var nexradAnimationJob: Job? = null
        fun scheduleNexradAnimation(delayMs: Int?) {
            nexradAnimationJob?.cancel()
            nexradAnimationJob = null
            if (delayMs == null) {
                return
            }
            nexradAnimationJob = launch {
                delay(delayMs.coerceAtLeast(0).toLong())
                nexradRenderRequests.trySend(Unit)
            }
        }
        for (ignored in nexradRenderRequests) {
            val effectStartMs = SystemClock.elapsedRealtime()
            scheduleNexradAnimation(null)
            val latestSurfaceSize = nexradSurfaceSizeState.value
            if (latestSurfaceSize.width <= 0 || latestSurfaceSize.height <= 0) {
                nexradFrame = null
                perfLogInfo(MapLayerLogTag) { "nexrad skipped reason=empty-surface elapsedMs=${SystemClock.elapsedRealtime() - effectStartMs}" }
                continue
            }
            if (!nexradVisibleState.value || !nexradEnabledState.value) {
                perfLogInfo(MapLayerLogTag) { "nexrad hidden cachedImages=${nexradFrame?.images?.size ?: 0} elapsedMs=${SystemClock.elapsedRealtime() - effectStartMs}" }
                continue
            }
            val latestViewport = nexradViewportState.value
            val latestSurfaceWidthPx = nexradSurfaceWidthPxState.value
            val latestSurfaceHeightPx = nexradSurfaceHeightPxState.value
            val latestDevServerBaseUrl = nexradDevServerBaseUrlState.value
            try {
                var imageBytes = 0L
                var fetchMs = 0L
                var decodeMs = 0L
                val overlay = withContext(Dispatchers.IO) {
                    uiSession.queryNexradOverlay(
                        latestViewport,
                        latestSurfaceSize.width.toDouble(),
                        latestSurfaceSize.height.toDouble(),
                    ) { resource ->
                        val fetchStartMs = SystemClock.elapsedRealtime()
                        fetchNexradCoreResource(context, resource, latestDevServerBaseUrl).also {
                            fetchMs += SystemClock.elapsedRealtime() - fetchStartMs
                        }
                    }
                }
                if (overlay.tiles.isEmpty()) {
                    scheduleNexradAnimation(overlay.animation.nextUpdateDelayMs)
                    nexradFrame = null
                    perfLogInfo(MapLayerLogTag) {
                        "nexrad empty status=${overlay.status.state} animation=${overlay.animation.phase} nextMs=${overlay.animation.nextUpdateDelayMs} elapsedMs=${SystemClock.elapsedRealtime() - effectStartMs}"
                    }
                    continue
                }
                val decodedImagesBySrc = LinkedHashMap<String, androidx.compose.ui.graphics.ImageBitmap>()
                var decodedImageBytes = 0L
                val images = withContext(Dispatchers.IO) {
                    overlay.tiles.map { tile ->
                        val bitmap = decodedImagesBySrc.getOrPut(tile.src) {
                            val bytes = uiSession.nexradTileBytes(tile.src) { resource ->
                                val fetchStartMs = SystemClock.elapsedRealtime()
                                fetchNexradCoreResource(context, resource, latestDevServerBaseUrl).also {
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
                nexradFrame = NexradOverlayFrame(
                    images = images,
                    viewport = latestViewport,
                    surfaceWidthPx = latestSurfaceWidthPx,
                    surfaceHeightPx = latestSurfaceHeightPx,
                    decodedImageCount = decodedImagesBySrc.size,
                    decodedBytes = decodedImageBytes,
                )
                perfLogInfo(MapLayerLogTag) {
                    "nexrad frame-ready pieces=${images.size} decodedImages=${decodedImagesBySrc.size} res=${overlay.stats.res} animation=${overlay.animation.phase} frame=${overlay.animation.selectedFrameIndex}/${overlay.animation.frameCount} nextMs=${overlay.animation.nextUpdateDelayMs} imageBytes=$imageBytes decodedBytes=$decodedImageBytes fetchMs=$fetchMs decodeMs=$decodeMs elapsedMs=${SystemClock.elapsedRealtime() - effectStartMs}"
                }
                scheduleNexradAnimation(overlay.animation.nextUpdateDelayMs)
            } catch (error: CancellationException) {
                throw error
            } catch (error: Throwable) {
                Log.w("AerobagLayers", "nexrad unavailable; retaining previous frame", error)
            }
        }
    }
    LaunchedEffect(uiSession, nexradViewportRefreshRequests) {
        for (ignored in nexradViewportRefreshRequests) {
            delay(NexradViewportRefreshThrottleMs)
            val latestSurfaceSize = nexradSurfaceSizeState.value
            val latestFrame = nexradFrameState.value
            if (
                latestSurfaceSize.width > 0 &&
                latestSurfaceSize.height > 0 &&
                nexradVisibleState.value &&
                nexradEnabledState.value &&
                latestFrame != null &&
                latestFrame.images.isNotEmpty()
            ) {
                nexradRenderRequests.trySend(Unit)
            }
        }
    }
    LaunchedEffect(uiSession, currentViewport, page, mapLayerState.nexrad.visible, mapLayerState.nexrad.enabled) {
        if (
            page == AppPage.Map &&
            mapLayerState.nexrad.visible &&
            mapLayerState.nexrad.enabled &&
            nexradFrameState.value?.images?.isNotEmpty() == true
        ) {
            nexradViewportRefreshRequests.trySend(Unit)
        }
    }
    LaunchedEffect(uiSession, liveFeedGeneration, surfaceSize, mapLayerState.nexrad.visible, mapLayerState.nexrad.enabled, page, devServerBaseUrl) {
        if (surfaceSize.width <= 0 || surfaceSize.height <= 0) {
            nexradFrame = null
            return@LaunchedEffect
        }
        if (page == AppPage.Map && mapLayerState.nexrad.visible && mapLayerState.nexrad.enabled) {
            nexradRenderRequests.trySend(Unit)
        }
    }
    LaunchedEffect(uiSession, terrainRenderRequests, devServerBaseUrl) {
        for (ignored in terrainRenderRequests) {
            while (true) {
                val effectStartMs = SystemClock.elapsedRealtime()
                val latestSurfaceSize = terrainSurfaceSizeState.value
                if (!terrainMapVisibleState.value || latestSurfaceSize.width <= 0 || latestSurfaceSize.height <= 0) {
                    terrainOverlay = emptyList()
                    terrainOverlayError = null
                    perfLogInfo(MapLayerLogTag) { "terrain skipped reason=empty-surface elapsedMs=${SystemClock.elapsedRealtime() - effectStartMs}" }
                    break
                }
                if (!terrainVisibleState.value) {
                    terrainOverlay = emptyList()
                    terrainOverlayError = null
                    perfLogInfo(MapLayerLogTag) { "terrain disabled elapsedMs=${SystemClock.elapsedRealtime() - effectStartMs}" }
                    break
                }
                val latestViewport = terrainViewportState.value
                val latestSurfaceWidthPx = terrainSurfaceWidthPxState.value
                val latestSurfaceHeightPx = terrainSurfaceHeightPxState.value
                val query = try {
                    withContext(Dispatchers.IO) {
                        uiSession.queryTerrainOverlay(
                            latestViewport,
                            latestSurfaceWidthPx.toDouble(),
                            latestSurfaceHeightPx.toDouble(),
                            terrainTileBitmapCache.keys.toList(),
                            terrainTileInFlightKeys.toList(),
                        ) { resource ->
                            fetchTerrainCoreResource(context, resource, devServerBaseUrl)
                        }
                    }
                } catch (error: Throwable) {
                    terrainOverlay = emptyList()
                    terrainOverlayError = error.message ?: error::class.java.simpleName
                    terrainLastQueryDiagnostics = TerrainOverlayDiagnostics(
                        updatedAtMs = SystemClock.elapsedRealtime(),
                        status = "query-error",
                        viewportZoom = latestViewport.zoom,
                        viewportCenterWorldX = latestViewport.centerWorldX,
                        viewportCenterWorldY = latestViewport.centerWorldY,
                        surfaceWidthPx = latestSurfaceSize.width,
                        surfaceHeightPx = latestSurfaceSize.height,
                        error = terrainOverlayError,
                    )
                    Log.w("AerobagLayers", "terrain overlay unavailable", error)
                    break
                }
                val queryMs = SystemClock.elapsedRealtime() - effectStartMs
                terrainLastQueryDiagnostics = query.toDiagnostics(
                    updatedAtMs = SystemClock.elapsedRealtime(),
                    viewport = latestViewport,
                    surfaceSize = latestSurfaceSize,
                )
                if (query.status !is org.aerobag.app.domain.TerrainOverlayStatus.Ready) {
                    terrainOverlay = emptyList()
                    terrainOverlayError = null
                    perfLogInfo(MapLayerLogTag) {
                        "terrain not-ready status=${query.status::class.java.simpleName} queryMs=$queryMs elapsedMs=${SystemClock.elapsedRealtime() - effectStartMs}"
                    }
                    break
                }
                val frameKey = query.frameKey
                val altitudeBucketFt = query.altitudeBucketFt
                if (frameKey == null || altitudeBucketFt == null) {
                    terrainOverlay = emptyList()
                    terrainOverlayError = null
                    perfLogInfo(MapLayerLogTag) {
                        "terrain not-ready status=missing-frame queryMs=$queryMs elapsedMs=${SystemClock.elapsedRealtime() - effectStartMs}"
                    }
                    break
                }
                if (query.schedule.frameComplete) {
                    val images = terrainImagesForCompleteQuery(terrainTileBitmapCache, query)
                    if (images != null) {
                        terrainOverlay = images
                        terrainOverlayError = null
                        if (images.isNotEmpty()) {
                            terrainNoPaintStartedMs = 0L
                            terrainNoPaintLastWarningMs = 0L
                        }
                    }
                    perfLogInfo(MapLayerLogTag) {
                        "terrain frame-ready frame=$frameKey requests=${query.tileRequests.size} images=${images?.size ?: 0} cached=${query.schedule.cachedCount} queryMs=$queryMs elapsedMs=${SystemClock.elapsedRealtime() - effectStartMs}"
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
                for (request in workBatch) {
                    if (!terrainMapVisibleState.value || !terrainVisibleState.value) {
                        break
                    }
                    if (terrainTileBitmapCache.containsKey(request.cacheKey) || terrainTileInFlightKeys.contains(request.cacheKey)) {
                        continue
                    }
                    var fetchMs = 0L
                    var renderMs = 0L
                    var parseMs = 0L
                    var rawBytesTotal = 0L
                    terrainTileInFlightKeys += request.cacheKey
                    try {
                        val rawBytes = withContext(Dispatchers.IO) {
                            val renderStartMs = SystemClock.elapsedRealtime()
                            uiSession.renderTerrainOverlayTile(request, altitudeBucketFt) { resource ->
                                val fetchStartMs = SystemClock.elapsedRealtime()
                                fetchTerrainCoreResource(context, resource, devServerBaseUrl).also {
                                    fetchMs += SystemClock.elapsedRealtime() - fetchStartMs
                                }
                            }.also {
                                renderMs += SystemClock.elapsedRealtime() - renderStartMs
                            }
                        }
                        rawBytesTotal += rawBytes.size
                        val parseStartMs = SystemClock.elapsedRealtime()
                        val bitmap = parseTerrainRawRgba(rawBytes)
                        parseMs += SystemClock.elapsedRealtime() - parseStartMs
                        cacheTerrainBitmap(terrainTileBitmapCache, request, bitmap)
                        batchRendered += 1
                        batchFetchMs += fetchMs
                        batchRenderMs += renderMs
                        batchParseMs += parseMs
                        batchRawBytesTotal += rawBytesTotal
                    } catch (error: CancellationException) {
                        throw error
                    } catch (error: Throwable) {
                        terrainOverlayError = error.message ?: error::class.java.simpleName
                        terrainLastQueryDiagnostics = query.toDiagnostics(
                            updatedAtMs = SystemClock.elapsedRealtime(),
                            viewport = latestViewport,
                            surfaceSize = latestSurfaceSize,
                            error = terrainOverlayError,
                        )
                        Log.w("AerobagLayers", "terrain overlay unavailable", error)
                        break
                    } finally {
                        terrainTileInFlightKeys -= request.cacheKey
                    }
                    yield()
                }
                perfLogInfo(MapLayerLogTag) {
                    "terrain batch-rendered frame=$frameKey requests=${query.tileRequests.size} rendered=$batchRendered batch=${workBatch.size} cached=${query.schedule.cachedCount} missing=${query.schedule.missingCount} rawBytes=$batchRawBytesTotal queryMs=$queryMs fetchMs=$batchFetchMs renderMs=$batchRenderMs parseMs=$batchParseMs batchMs=${SystemClock.elapsedRealtime() - batchStartMs} elapsedMs=${SystemClock.elapsedRealtime() - effectStartMs}"
                }
            }
        }
    }
    LaunchedEffect(uiSession, currentViewport, surfaceSize, mapLayerState.terrainWarning.visible, page, devServerBaseUrl, ownship.terrainAltitudeBucketFt, ownship.position != null) {
        val effectStartMs = SystemClock.elapsedRealtime()
        if (surfaceSize.width <= 0 || surfaceSize.height <= 0 || page != AppPage.Map) {
            terrainOverlay = emptyList()
            terrainOverlayError = null
            perfLogInfo(MapLayerLogTag) { "terrain skipped reason=empty-surface elapsedMs=${SystemClock.elapsedRealtime() - effectStartMs}" }
            return@LaunchedEffect
        }
        if (!mapLayerState.terrainWarning.visible) {
            terrainOverlay = emptyList()
            terrainOverlayError = null
            perfLogInfo(MapLayerLogTag) { "terrain disabled elapsedMs=${SystemClock.elapsedRealtime() - effectStartMs}" }
            return@LaunchedEffect
        }
        terrainRenderRequests.trySend(Unit)
    }
    val displayedMapOverlay = remember(
        committedMapOverlay,
        committedOverlayViewport,
        committedOverlaySurfaceUnits,
        currentViewport,
        surfaceWidthPx,
        surfaceHeightPx,
    ) {
        transformMapOverlayForDisplay(
            overlay = committedMapOverlay,
            fromViewport = committedOverlayViewport,
            fromSurface = committedOverlaySurfaceUnits,
            toViewport = currentViewport,
            toSurface = OverlaySurfaceUnits(surfaceWidthPx, surfaceHeightPx),
        )
    }
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
        val world = screenToWorld(
            viewportState.value,
            ScreenPoint(point.x, point.y),
            surfaceWidthPx,
            surfaceHeightPx,
        )
        val (lat, lon) = worldToLatLon(world.x, world.y)
        sessionWorkRunner.submitMapSelection(
            viewport = viewportState.value,
            widthPx = surfaceWidthPx.toDouble(),
            heightPx = surfaceHeightPx.toDouble(),
            click = LatLonPoint(lat = lat, lon = lon),
            pointDisplayScale = density.density.toDouble(),
            onResult = { result ->
                mapSelection = MapSelectionUiState(point = point, result = result, selectedItem = null)
                chartTrayOpen = false
                layerTrayOpen = false
                dataStatusTrayOpen = false
                situationTrayOpen = false
            },
            onError = { error ->
                Log.w("AerobagSelection", "map selection failed", error)
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

    Box(
        modifier = Modifier
            .fillMaxSize()
            .testTag("parity:map-surface")
            .background(uiTheme.controls.chartSurfaceBg)
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
                updateViewport(
                    zoomAroundPoint(
                        viewport = viewportState.value,
                        minZoom = selectedMap.minZoom,
                        maxZoom = interactiveMaxZoom,
                        anchor = ScreenPoint(surfaceWidthPx / 2f, surfaceHeightPx / 2f),
                        widthPx = surfaceWidthPx,
                        heightPx = surfaceHeightPx,
                        nextZoom = clampZoom(viewportState.value.zoom + delta, selectedMap.minZoom, interactiveMaxZoom),
                    ),
                    syncFollow = false,
                )
                syncFollowStateForViewport(viewportState.value)
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
                                            viewportState.value,
                                            dx = dx,
                                            dy = dy,
                                        )
                                        movedViewportDuringGesture = true
                                        updateViewport(gestureViewport, syncFollow = false)
                                        endingDragChange.consume()
                                    }
                                }
                                break
                            }
                            if (pressed.any { mapInputBlockedAt(it.position) }) {
                                break
                            }
                            mapGestureActive = true
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
                                        gestureViewport,
                                        dx = change.position.x - last.x,
                                        dy = change.position.y - last.y,
                                    )
                                    movedViewportDuringGesture = true
                                    updateViewport(gestureViewport, syncFollow = false)
                                    dragLastPosition = change.position
                                }
                                change.consume()
                            } else {
                                val first = pressed[0]
                                val second = pressed[1]
                                if (pinchSnapshot == null) {
                                    gestureViewport = viewportState.value
                                    pinchSnapshot = createPinchSnapshot(
                                        viewport = gestureViewport,
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
                                updateViewport(gestureViewport, syncFollow = false)
                                first.consume()
                                second.consume()
                            }
                        }
                    } finally {
                        val completedGestureSyncViewport = mapFollowSyncViewportForCompletedGesture(
                            movedViewportDuringGesture = movedViewportDuringGesture,
                            finalGestureViewport = gestureViewport,
                        )
                        if (completedGestureSyncViewport != null) {
                            syncFollowStateForViewport(completedGestureSyncViewport)
                        } else if (loggedGestureSeed && dragLastPosition != null) {
                            val point = dragLastPosition
                            requestMapSelection(point)
                        }
                        mapGestureActive = false
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
                    updateViewport(
                        zoomAroundPoint(
                            viewport = viewportState.value,
                            minZoom = selectedMap.minZoom,
                            maxZoom = interactiveMaxZoom,
                            anchor = ScreenPoint(surfaceWidthPx / 2f, surfaceHeightPx / 2f),
                            widthPx = surfaceWidthPx,
                            heightPx = surfaceHeightPx,
                            nextZoom = clampZoom(viewportState.value.zoom - wheelDelta * 0.28, selectedMap.minZoom, interactiveMaxZoom),
                        ),
                        syncFollow = false,
                    )
                    syncFollowStateForViewport(viewportState.value)
                    true
                } else {
                    false
                }
            },
    ) {
        RasterImageLayers(
            tiles = tiles,
            tileRects = tileRects,
            tileBitmapCache = tileBitmapCache,
            tileLabels = debugState.tileLabels,
            tileLabelPaint = tileLabelPaint,
            tileLabelBackgroundPaint = tileLabelBackgroundPaint,
            nexradFrame = if (mapLayerState.nexrad.visible) nexradFrame else null,
            terrainOverlay = terrainOverlay,
            viewport = currentViewport,
            surfaceWidthPx = surfaceWidthPx,
            surfaceHeightPx = surfaceHeightPx,
        )
        AirspaceOverlayLayer(displayedMapOverlay, density.density, uiTheme)
        MapFeatureOverlayLayer(
            displayedMapOverlay = displayedMapOverlay,
            uiTheme = uiTheme,
            densityScale = density.density,
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
        )
        ObservationOverlayLayer(displayedMapOverlay, density.density, uiTheme)
        OfflineRegionsOverlayLayer(displayedMapOverlay, density.density, uiTheme)
        RouteOverlayLayer(
            flightPlanRoute = flightPlanRoute,
            viewport = currentViewport,
            surfaceWidthPx = surfaceWidthPx,
            surfaceHeightPx = surfaceHeightPx,
            densityScale = density.density,
            uiTheme = uiTheme,
        )
        MapFeatureOverlayLayer(
            displayedMapOverlay = displayedMapOverlay,
            uiTheme = uiTheme,
            densityScale = density.density,
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
            flightPlanOnly = true,
        )
        MapSelectionHighlightLayer(
            selectedItem = mapSelection?.selectedItem,
            displayedMapOverlay = displayedMapOverlay,
            viewport = currentViewport,
            surfaceWidthPx = surfaceWidthPx,
            surfaceHeightPx = surfaceHeightPx,
            densityScale = density.density,
            uiTheme = uiTheme,
        )
        SituationOverlayLayer(
            situationOverlay = situationOverlay,
            densityScale = density.density,
            labelStrokePaint = labelStrokePaint,
            labelFillPaint = labelFillPaint,
            aircraftDrawable = aircraftDrawable,
        )
        mapFollowProbeTag?.let { tag ->
            Box(
                modifier = Modifier
                    .align(Alignment.TopStart)
                    .size(1.dp)
                    .testTag(tag),
            )
        }
        FlightDataBanner(
            banner = flightDataBanner,
            surfaceSize = surfaceSize,
            situationDockTopPadding = situationDockTopPadding,
            uiTheme = uiTheme,
            modifier = Modifier.align(if (surfaceWidthPx > surfaceHeightPx) Alignment.TopEnd else Alignment.TopCenter),
        )
        DataStatusBadge(
            dataStatusState = sessionSnapshot.dataStatusState,
            modifier = Modifier
                .align(Alignment.TopEnd)
                .padding(
                    top = situationDockTopPadding,
                    end = ThumbGap + MenuDockStyle.Situation.buttonWidth + ThumbGap,
                ),
            open = dataStatusTrayOpen,
            onToggle = {
                dataStatusTrayOpen = !dataStatusTrayOpen
                situationTrayOpen = false
                chartTrayOpen = false
                layerTrayOpen = false
            },
            onAction = { actionId ->
                applySessionCommand("performStatusAction") {
                    uiSession.performStatusAction(actionId)
                }
            },
        )
        SituationStatusBadge(
            controls = ownshipControls,
            modifier = Modifier
                .align(Alignment.TopEnd)
                .padding(top = situationDockTopPadding, end = ThumbGap),
            open = situationTrayOpen,
            onToggle = {
                situationTrayOpen = !situationTrayOpen
                dataStatusTrayOpen = false
                chartTrayOpen = false
                layerTrayOpen = false
            },
            onSelectSource = { sourceId ->
                situationTrayOpen = false
                onSelectOwnshipSource(sourceId)
            },
            onSituationControlInput = onSituationControlInput,
        )

        MapTopLeftControls(
            modifier = Modifier.align(Alignment.TopStart),
            currentPage = page,
            onSelectPage = {
                onSelectPage(it)
                chartTrayOpen = false
                layerTrayOpen = false
                dataStatusTrayOpen = false
                situationTrayOpen = false
            },
            selectedLabel = selectedLauncher.launcherLabel,
            chartReferenceFamilyId = chartReferenceAction?.family_id,
            onOpenChartReference = {
                chartReferenceAction?.let { action ->
                    onOpenChartReference(action.family_id, action.suggested_chart_ids)
                }
            },
            trayOptions = trayOptions,
            trayOpen = chartTrayOpen,
            onToggle = {
                chartTrayOpen = !chartTrayOpen
                layerTrayOpen = false
                dataStatusTrayOpen = false
                situationTrayOpen = false
            },
            layerTrayOpen = layerTrayOpen,
            onToggleLayerTray = {
                layerTrayOpen = !layerTrayOpen
                chartTrayOpen = false
                dataStatusTrayOpen = false
                situationTrayOpen = false
            },
            layerOptions = layerTrayOptions,
            chartSearchText = chartSearchText,
            chartSearchOpen = chartSearchOpen,
            chartSearchLoading = chartSearchLoading,
            chartSearchError = chartSearchError,
            chartSearchSuggestions = chartSearchSuggestions,
            onChartSearchTextChange = { value ->
                chartSearchText = value.uppercase().filter { it in 'A'..'Z' || it in '0'..'9' }.take(8)
                chartSearchOpen = true
            },
            onChartSearchFocus = { chartSearchOpen = true },
            onChartSearchSubmit = { submitChartSearch() },
            onChartSearchSuggestionClick = { suggestion -> inspectNavRef(suggestion.navRef) },
        )

        val playbackLeftRoomUnits = surfaceWidthDp / 2f - (ThumbSize.value * 1.5f) - (ThumbGap.value * 2f)
        val playbackBottomPadding =
            if (playbackLeftRoomUnits < ThumbSize.value * 2.8f) {
                ThumbGap + (ThumbSize * 0.67f) + ThumbGap
            } else {
                ThumbGap
            }
        if (playbackPanelState.visible) {
            PlaybackWidget(
                uiSession = uiSession,
                playbackUiState = playbackUiState,
                sourcePath = playbackSourcePath,
                onSourcePathChange = onPlaybackSourcePathChange,
                onSnapshotChange = onSessionSnapshotChange,
                onSessionCommandFailure = onSessionCommandFailure,
                modifier = Modifier
                    .align(Alignment.BottomStart)
                    .padding(start = ThumbGap, bottom = playbackBottomPadding),
            )
        }

        CompactSquareButton(
            label = "CTR",
            modifier = Modifier
                .align(Alignment.BottomEnd)
                .padding(end = ThumbGap, bottom = ThumbGap)
                .size(ThumbSize),
            enabled = mapFollowUiState.canCenterHere || mapFollowUiState.following,
            selected = mapFollowUiState.following,
            selectedColor = Color(0xFF0D6F67),
            onDisabledClick = mapFollowUiState.disabledReason?.let { reason ->
                { showDisabledActionToast(context, reason) }
            },
            onClick = {
                followTargetGate.clear()
                applySessionCommand(if (mapFollowUiState.following) "disengageMapFollow" else "engageMapFollow") {
                    if (mapFollowUiState.following) {
                        uiSession.disengageMapFollow(viewportState.value)
                    } else {
                        uiSession.engageMapFollow(viewportState.value)
                    }
                }
            },
        )

        if (menuTrayOpen) {
            Scrim {
                chartTrayOpen = false
                layerTrayOpen = false
                dataStatusTrayOpen = false
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
                    if (selection.detailModal != null) {
                        selection.detailModal.weatherDetail?.let { weatherDetail ->
                            WeatherDetailModal(
                                detail = weatherDetail,
                                modifier = Modifier
                                    .align(Alignment.Center)
                                    .zIndex(OverlayPlaneModal),
                            )
                        } ?: MapSelectionDetailModal(
                            title = selection.detailModal.title,
                            text = selection.detailModal.text.orEmpty(),
                            modifier = Modifier
                                .align(Alignment.Center)
                                .zIndex(OverlayPlaneModal),
                        )
                    } else {
                        MapSelectionTray(
                            state = selection,
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
                                val weatherDetail = immediateWeatherDetailForMapSelectionItem(item)
                                mapSelection = selection.copy(
                                    selectedItem = item,
                                    detailModal = weatherDetail?.let { detail ->
                                        MapSelectionDetailModalState(
                                            title = "WX ${detail.stationId}",
                                            weatherDetail = detail,
                                        )
                                    },
                                )
                            },
                            onSelectAction = { item, action ->
                                if (!action.enabled) {
                                    action.disabledReason
                                        ?.takeIf { it.isNotBlank() }
                                        ?.let { reason ->
                                            Toast.makeText(context, reason, Toast.LENGTH_SHORT).show()
                                        }
                                    return@MapSelectionTray
                                }
                                action.weatherDetail?.let { detail ->
                                    mapSelection = selection.copy(
                                        selectedItem = item,
                                        detailModal = MapSelectionDetailModalState(
                                            title = "WX ${detail.stationId}",
                                            weatherDetail = detail,
                                        ),
                                    )
                                    return@MapSelectionTray
                                }
                                action.detailText?.let { detail ->
                                    mapSelection = selection.copy(
                                        selectedItem = item,
                                        detailModal = MapSelectionDetailModalState(
                                            title = action.detailTitle ?: action.label,
                                            text = detail,
                                        ),
                                    )
                                    return@MapSelectionTray
                                }
                                action.flightPlanRowAction?.let { rowAction ->
                                    applySessionCommand("performFlightPlanRowAction") {
                                        uiSession.performFlightPlanRowAction(rowAction.rowUid, rowAction.actionUid)
                                    }
                                    mapSelection = null
                                    return@MapSelectionTray
                                }
                                when (val navigation = action.navigation) {
                                    is MapSelectionNavigationAction.OpenPlateTarget -> {
                                        onOpenPlateTarget(navigation.airportId, navigation.target, navigation.chartId)
                                        mapSelection = null
                                        return@MapSelectionTray
                                    }
                                    null -> Unit
                                }
                                action.sessionAction?.let { sessionAction ->
                                    applySessionCommand("performMapSelectionAction") {
                                        uiSession.performMapSelectionAction(sessionAction)
                                    }
                                    mapSelection = null
                                    return@MapSelectionTray
                                }
                            },
                        )
                    }
                }
            }
        }

        NavElementDock(
            navElement = navElement,
            onClick = onOpenPlan,
            modifier = Modifier
                .align(Alignment.BottomCenter)
                .padding(bottom = ThumbGap),
        )

    }
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
) {
    Canvas(modifier = Modifier.fillMaxSize()) {
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
) {
    val center = Offset(feature.screenX.toFloat(), feature.screenY.toFloat())
    val contrastColor = Color.White
    val contrastStrokeWidth = 8f * densityScale
    if (feature.symbolKind == "airport") {
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
            val headingRad = Math.toRadians(headingDeg)
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
    viewport: MapViewportState,
    surfaceWidthPx: Float,
    surfaceHeightPx: Float,
    densityScale: Float,
    uiTheme: UiTheme,
) {
    if (flightPlanRoute.isEmpty() || surfaceWidthPx <= 0f || surfaceHeightPx <= 0f) return
    val visibleSegmentCount = remember(flightPlanRoute, viewport, surfaceWidthPx, surfaceHeightPx) {
        countVisibleRouteSegments(
            flightPlanRoute = flightPlanRoute,
            viewport = viewport,
            surfaceWidthPx = surfaceWidthPx,
            surfaceHeightPx = surfaceHeightPx,
        )
    }
    Canvas(
        modifier = Modifier
            .fillMaxSize()
            .testTag("parity:flight-plan-route-overlay:segments:${flightPlanRoute.size}:visible:$visibleSegmentCount"),
    ) {
        flightPlanRoute.forEach { segment ->
            val path = segment.path.ifEmpty { listOf(segment.from, segment.to) }.map { point ->
                latLonToScreen(point.lat, point.lon, viewport, surfaceWidthPx, surfaceHeightPx)
            }
            val first = path.firstOrNull() ?: return@forEach
            val routePath = Path().apply {
                moveTo(first.x, first.y)
                path.drop(1).forEach { point -> lineTo(point.x, point.y) }
            }
            drawPath(
                path = routePath,
                color = Color(0x8C000000),
                style = Stroke(
                    width = 7f * densityScale,
                    cap = StrokeCap.Round,
                    join = StrokeJoin.Round,
                ),
            )
            drawPath(
                path = routePath,
                color = routeSegmentColor(uiTheme, segment.status),
                style = Stroke(
                    width = 3.5f * densityScale,
                    cap = StrokeCap.Round,
                    join = StrokeJoin.Round,
                ),
            )
        }
    }
}

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
    aircraftDrawable: android.graphics.drawable.Drawable?,
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
        drawContext.canvas.nativeCanvas.apply {
            if (ring != null) {
                val labelPoint = ring.labelPointUnits
                save()
                rotate(ring.labelRotationDeg, labelPoint.x, labelPoint.y)
                labelStrokePaint.textSize = 16f * densityScale
                labelFillPaint.textSize = 16f * densityScale
                drawText(ring.labelText, labelPoint.x, labelPoint.y + labelFillPaint.textSize * 0.33f, labelStrokePaint)
                drawText(ring.labelText, labelPoint.x, labelPoint.y + labelFillPaint.textSize * 0.33f, labelFillPaint)
                restore()
            }
            val iconSizePx = ThumbSize.toPx() * 0.72f
            val left = (center.x - iconSizePx / 2f).roundToInt()
            val top = (center.y - iconSizePx / 2f).roundToInt()
            if (aircraftDrawable != null) {
                save()
                rotate(situationOverlay.headingDeg, center.x, center.y)
                aircraftDrawable.setBounds(left, top, (left + iconSizePx).roundToInt(), (top + iconSizePx).roundToInt())
                aircraftDrawable.draw(this)
                restore()
            }
        }
    }
}

@Composable
@OptIn(ExperimentalComposeUiApi::class)
internal fun MapSelectionTray(
    state: MapSelectionUiState,
    modifier: Modifier,
    onBoundsChange: (Rect?) -> Unit = {},
    onSelectItem: (MapSelectionItem) -> Unit,
    onSelectAction: (MapSelectionItem, MapSelectionAction) -> Unit,
) {
    val uiTheme = LocalAerobagUiTheme.current
    val selectedItem = state.selectedItem
    val actionSlots = selectedItem?.actions.orEmpty()
    val firstActionRow = actionSlots.take(3)
    val secondActionRow = actionSlots.drop(3).take(3)
    Surface(
        modifier = modifier
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
                    MapSelectionActionRow(
                        actions = firstActionRow,
                        selectedItem = selectedItem,
                        onSelectAction = onSelectAction,
                    )
                    if (selectedItem?.detailText != null) {
                        MapSelectionInlineDetailText(selectedItem.detailText)
                    } else {
                        MapSelectionActionRow(
                            actions = secondActionRow,
                            selectedItem = selectedItem,
                            onSelectAction = onSelectAction,
                        )
                    }
                }
            }
        }
    }
}

@Composable
internal fun MapSelectionHeader(selectedItem: MapSelectionItem?) {
    val uiTheme = LocalAerobagUiTheme.current
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .height(ThumbSize * 0.52f),
        verticalArrangement = Arrangement.Center,
    ) {
        Text(
            text = buildAnnotatedString {
                if (selectedItem != null) {
                    withStyle(SpanStyle(fontWeight = FontWeight.Bold)) { append(selectedItem.label) }
                    selectedItem.description?.takeIf { it.isNotBlank() }?.let { append(" · $it") }
                } else {
                    append(" ")
                }
            },
            style = MaterialTheme.typography.labelMedium.copy(lineHeight = 15.sp),
            color = uiTheme.controls.panelFg,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
        )
        Text(
            text = selectedItem?.secondaryDescription?.trim()?.takeIf { it.isNotEmpty() } ?: "\u00a0",
            style = MaterialTheme.typography.labelSmall.copy(
                fontSize = 13.sp,
                fontWeight = FontWeight.Bold,
                lineHeight = 14.sp,
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
            if (action == null) {
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
internal fun MapSelectionDetailModal(
    title: String,
    text: String,
    modifier: Modifier = Modifier,
) {
    val uiTheme = LocalAerobagUiTheme.current
    Surface(
        modifier = modifier
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
internal fun WeatherDetailModal(
    detail: WeatherDetailUiView,
    modifier: Modifier = Modifier,
) {
    val uiTheme = LocalAerobagUiTheme.current
    Surface(
        modifier = modifier
            .widthIn(max = ThumbSize * 9.5f)
            .heightIn(max = ThumbSize * 8.5f),
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
                text = "WX ${detail.stationId}".uppercase(),
                style = MaterialTheme.typography.labelMedium.copy(
                    fontWeight = FontWeight.Black,
                    letterSpacing = 0.4.sp,
                ),
                color = uiTheme.controls.panelFg,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
            WeatherDetailSection(
                label = "METAR",
                ageLabel = detail.metarAgeLabel,
                ageWarning = detail.metarAgeWarning,
                text = detail.metarText,
            )
            WeatherDetailSection(
                label = "TAF",
                ageLabel = detail.tafAgeLabel,
                ageWarning = detail.tafAgeWarning,
                text = detail.tafText,
            )
            AirportNotamSection(notams = detail.notams)
        }
    }
}

@Composable
private fun AirportNotamSection(notams: List<AirportNotamUiView>) {
    val uiTheme = LocalAerobagUiTheme.current
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .heightIn(min = ThumbSize * 1.8f, max = ThumbSize * 4.1f)
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
                text = "NOTAM",
                style = MaterialTheme.typography.labelSmall.copy(
                    fontWeight = FontWeight.Black,
                    letterSpacing = 0.6.sp,
                ),
                color = uiTheme.controls.panelFg,
            )
            Text(
                text = notams.size.toString(),
                style = MaterialTheme.typography.labelSmall.copy(fontWeight = FontWeight.Black),
                color = uiTheme.controls.panelFg,
            )
        }
        if (notams.isEmpty()) {
            Text(
                text = "No airport NOTAMs available.",
                style = MaterialTheme.typography.bodyMedium.copy(fontWeight = FontWeight.Bold),
                color = uiTheme.controls.panelFg.copy(alpha = 0.65f),
            )
        } else {
            LazyColumn(
                modifier = Modifier.fillMaxWidth(),
                verticalArrangement = Arrangement.spacedBy(ThumbGap * 0.65f),
            ) {
                lazyColumnItems(notams, key = { it.id }) { notam ->
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
private fun WeatherDetailSection(
    label: String?,
    ageLabel: String?,
    ageWarning: Boolean,
    text: String?,
    expanded: Boolean = false,
) {
    val uiTheme = LocalAerobagUiTheme.current
    val minHeight = if (expanded) ThumbSize * 6.4f else ThumbSize * 1.2f
    val maxHeight = if (expanded) ThumbSize * 6.8f else ThumbSize * 3.3f
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .heightIn(min = minHeight, max = maxHeight)
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
            text = text ?: "No ${label ?: "text"} available.",
            modifier = Modifier
                .fillMaxWidth()
                .verticalScroll(rememberScrollState()),
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
    if (item.metarFeature == null) {
        return null
    }
    return item.actions.firstOrNull { it.enabled && it.weatherDetail != null }?.weatherDetail
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
    Surface(
        modifier = Modifier
            .size(ThumbSize)
            .testTag(testTag)
            .clickable(onClick = onClick),
        shape = RoundedCornerShape(ThumbRadius),
        color = containerColor,
        contentColor = uiTheme.controls.buttonFg,
        border = BorderStroke(
            if (selected) 2.dp else 1.dp,
            if (selected) uiTheme.controls.buttonFg else lerp(containerColor, Color.Black, 0.22f),
        ),
    ) {
        Column(
            modifier = Modifier.fillMaxSize().padding(3.dp),
            horizontalAlignment = Alignment.CenterHorizontally,
            verticalArrangement = Arrangement.Center,
        ) {
            MapSelectionItemIcon(item, Modifier.weight(1f).fillMaxWidth())
            Text(
                text = item.label,
                style = MaterialTheme.typography.labelSmall.copy(fontSize = 10.sp),
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
                textAlign = TextAlign.Center,
            )
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
            .testTag("parity:map-selection-action:${action.id}")
            .alpha(if (action.label.isBlank()) 0f else 1f)
            .semantics {
                if (!enabled) {
                    disabled()
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
        Box(modifier = Modifier.fillMaxSize().padding(4.dp), contentAlignment = Alignment.Center) {
            if (action.airspaceLimit != null) {
                Canvas(modifier = Modifier.fillMaxSize()) {
                    drawAirspaceLimitGlyph(uiTheme, action.airspaceLimit, Offset(size.width / 2f, size.height / 2f), 1.45f)
                }
            } else {
                Text(
                    text = action.label,
                    style = MaterialTheme.typography.labelSmall.copy(fontSize = 10.sp),
                    textAlign = TextAlign.Center,
                    maxLines = 2,
                    overflow = TextOverflow.Ellipsis,
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
                    keyboardActions = KeyboardActions(onDone = { onSubmit() }),
                    textStyle =
                        MaterialTheme.typography.headlineMedium.copy(
                            color = Color(0xFF132129),
                            fontWeight = FontWeight.ExtraBold,
                            textAlign = TextAlign.Center,
                        ),
                    modifier =
                        Modifier
                            .weight(1f)
                            .height(ThumbSize)
                            .focusRequester(focusRequester)
                            .clip(RoundedCornerShape(ThumbRadius))
                            .background(Color.White)
                            .border(1.dp, Color(0x334E626C), RoundedCornerShape(ThumbRadius))
                            .padding(horizontal = ThumbGap, vertical = ThumbSize * 0.18f),
                )
                CompactSquareButton(label = "Enter", modifier = Modifier.width(ThumbSize * 1.4f).height(ThumbSize), onClick = onSubmit)
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
                    val detail = "${suggestion.kind.uppercase()} ${"%.1f".format(suggestion.distanceFromAnchorNm)}nm"
                    val friendlyName = suggestion.displayName.takeIf { it.isNotBlank() }
                    MenuPanelRow(
                        label = if (friendlyName == null) {
                            "${suggestion.identifier}  $detail"
                        } else {
                            "${suggestion.identifier}  $detail\n$friendlyName"
                        },
                        active = false,
                        enabled = true,
                        width = ThumbSize * 3f,
                        onSelect = { onSuggestionClick(suggestion) },
                    )
                }
            }
        }
    }
}
