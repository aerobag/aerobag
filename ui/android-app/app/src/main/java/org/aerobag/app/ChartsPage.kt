package org.aerobag.app

import android.Manifest
import android.content.Context
import android.content.Intent
import android.content.SharedPreferences
import android.content.pm.PackageManager
import android.graphics.BitmapFactory
import android.graphics.Paint
import android.graphics.Typeface
import android.os.Build
import android.os.Bundle
import android.os.SystemClock
import android.util.Log
import android.view.KeyEvent as AndroidKeyEvent
import android.view.MotionEvent
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
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.delay
import kotlinx.coroutines.ensureActive
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext
import org.aerobag.app.domain.ChartAirport
import org.aerobag.app.domain.ChartAsset
import org.aerobag.app.domain.AppState
import org.aerobag.app.domain.AirwayPresentationPlan
import org.aerobag.app.domain.AirwaySuggestion
import org.aerobag.app.domain.WaypointIdentifierSuggestion
import org.aerobag.app.domain.CoreMapViewport
import org.aerobag.app.domain.CoreResourceRequest
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
import org.aerobag.app.domain.InstalledPackages
import org.aerobag.app.domain.AirspaceDisplayDecoration
import org.aerobag.app.domain.AirspaceDisplayLabel
import org.aerobag.app.domain.AirspaceDisplayPath
import org.aerobag.app.domain.AirspaceDisplaySubpath
import org.aerobag.app.domain.AirspaceLimitGlyph
import org.aerobag.app.domain.AirspaceScreenPoint
import org.aerobag.app.domain.ImageDisplaySize
import org.aerobag.app.domain.ImageViewportState
import org.aerobag.app.domain.LatLonPoint
import org.aerobag.app.domain.MapLayerId
import org.aerobag.app.domain.MapFollowUiState
import org.aerobag.app.domain.MapOverlayQueryResult
import org.aerobag.app.domain.MapSelectionAction
import org.aerobag.app.domain.MapSelectionHighlight
import org.aerobag.app.domain.MapSelectionItem
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
import org.aerobag.app.domain.PlateGeoref
import org.aerobag.app.domain.describeForLog
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
import org.aerobag.app.domain.RouteSegmentStatus
import org.aerobag.app.domain.RouteComponentViewKind
import org.aerobag.app.domain.RouteComponent
import org.aerobag.app.domain.ScreenPoint
import org.aerobag.app.domain.SectionalPackages
import org.aerobag.app.domain.SequencingMode
import org.aerobag.app.domain.SituationControlInput
import org.aerobag.app.domain.SituationRingCandidate
import org.aerobag.app.domain.TileStorageKind
import org.aerobag.app.domain.UiDataStatusState
import org.aerobag.app.domain.UiDebugState
import org.aerobag.app.domain.UiMapLayerToggleState
import org.aerobag.app.domain.UiTheme
import org.aerobag.app.domain.UiThemeLoader
import org.aerobag.app.domain.UiSessionSnapshot
import org.aerobag.app.domain.VisibleMapFeature
import org.aerobag.app.domain.VisibleMetarFeature
import org.aerobag.app.domain.VisiblePirepFeature
import org.aerobag.app.domain.applyPinchGesture
import org.aerobag.app.domain.clampZoom
import org.aerobag.app.domain.createInitialImageViewport
import org.aerobag.app.domain.createPinchSnapshot
import org.aerobag.app.domain.dragImageViewport
import org.aerobag.app.domain.dragViewport
import org.aerobag.app.domain.imageDisplaySize
import org.aerobag.app.domain.latLonToWorld
import org.aerobag.app.domain.preserveViewportForMap
import org.aerobag.app.domain.renderTileKey
import org.aerobag.app.domain.scaleForZoom
import org.aerobag.app.domain.screenToWorld
import org.aerobag.app.domain.viewportCenterLatLon
import org.aerobag.app.domain.worldToLatLon
import org.aerobag.app.domain.zoomAroundPoint
import org.aerobag.app.domain.zoomImageAroundPoint
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
import java.net.URL
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


@OptIn(ExperimentalComposeUiApi::class)
@Composable
internal fun ChartsPage(
    page: AppPage,
    pageHistory: List<AppViewSnapshot>,
    uptimeLabel: String,
    airports: List<ChartAirport>,
    selectedAirport: ChartAirport?,
    selectedChart: ChartAsset?,
    plan: FlightPlan,
    uiTheme: UiTheme,
    ownship: OwnshipRenderState,
    ownshipControls: OwnshipControlModel,
    dataStatusState: UiDataStatusState,
    flightDataBanner: FlightDataBannerModel,
    uiSession: NativeUiSession,
    navElement: NavElementUiView?,
    folderOpen: Boolean,
    viewport: ImageViewportState?,
    onViewportChange: (ImageViewportState?) -> Unit,
    onSessionSnapshotChange: (UiSessionSnapshot) -> Unit,
    onSessionCommandFailure: (Throwable) -> Unit,
    onFolderOpenChange: (Boolean) -> Unit,
    onSelectPage: (AppPage) -> Unit,
    onOpenPlan: () -> Unit,
    onStatusAction: (String) -> Unit,
    onSelectAirport: (String) -> Unit,
    onSelectChart: (String) -> Unit,
    onSelectOwnshipSource: (String) -> Unit,
) {
    val context = LocalContext.current
    val activity = context as? MainActivity
    val aircraftDrawable = remember(context) { AppCompatResources.getDrawable(context, R.drawable.plan_view_icon)?.mutate() }
    val density = LocalDensity.current
    val focusRequester = remember { FocusRequester() }
    val devServerBaseUrl = remember(context) { loadAndroidDevServerBaseUrl(context.applicationContext) }
    val chartLabelsById = remember(airports) {
        airports.flatMap { airport -> airport.charts }.associate { chart -> chart.id to chart.label }
    }
    var airportTrayOpen by remember { mutableStateOf(false) }
    var chartTrayOpen by remember { mutableStateOf(false) }
    var loadTrayOpen by remember { mutableStateOf(false) }
    var dataStatusTrayOpen by remember { mutableStateOf(false) }
    var situationTrayOpen by remember { mutableStateOf(false) }
    var surfaceSize by remember { mutableStateOf(IntSize.Zero) }
    fun applySessionCommand(operation: () -> UiSessionSnapshot) {
        try {
            onSessionSnapshotChange(operation())
        } catch (error: CancellationException) {
            throw error
        } catch (error: Throwable) {
            onSessionCommandFailure(error)
        }
    }
    val surfaceWidthDp = remember(surfaceSize, density) { with(density) { surfaceSize.width.toDp().value } }
    val situationDockLowered = surfaceWidthDp.dp < SituationDockOverlapWidth
    val situationDockTopPadding =
        if (situationDockLowered) ThumbSize + (ThumbGap * 2f) else ThumbGap
    val sortedCharts = selectedAirport?.charts ?: emptyList()
    val overscrollPx = with(density) { ThumbSize.toPx() }
    val bitmap by produceState<androidx.compose.ui.graphics.ImageBitmap?>(initialValue = null, selectedChart?.id, uiSession, devServerBaseUrl) {
        val chartId = selectedChart?.id
        value = if (chartId == null) {
            null
        } else {
            withContext(Dispatchers.IO) {
                var attemptedResource: CoreResourceRequest? = null
                runCatching {
                    val bytes = uiSession.chartAssetBytes(chartId, "asset") { resource ->
                        attemptedResource = resource
                        fetchCoreResource(context, resource, devServerBaseUrl)
                    }
                    BitmapFactory.decodeByteArray(bytes, 0, bytes.size)?.asImageBitmap()
                        ?: error(
                            "failed to decode plate asset bytes for $chartId " +
                                "source=${attemptedResource?.source?.describeForLog() ?: "unresolved"}",
                        )
                }.onFailure { error ->
                    Log.w(
                        "AerobagCharts",
                        "plate asset unavailable chart=$chartId " +
                            "source=${attemptedResource?.source?.describeForLog() ?: "unresolved"}",
                        error,
                    )
                }.getOrNull()
            }
        }
    }
    val viewportState = rememberUpdatedState(viewport)
    val imageWidthPx = bitmap?.width?.toFloat() ?: 0f
    val imageHeightPx = bitmap?.height?.toFloat() ?: 0f
    val trayOpen = airportTrayOpen || chartTrayOpen || loadTrayOpen || dataStatusTrayOpen || situationTrayOpen
    val plateProcedureLoads by produceState<List<ProcedureLoadOption>>(initialValue = emptyList(), plan.version, selectedChart?.id) {
        val chart = selectedChart
        value = if (chart == null) {
            emptyList()
        } else {
            withContext(Dispatchers.IO) {
                runCatching { uiSession.describePlateProcedureLoads(plan, chart.id) }
                    .onFailure { Log.w("AerobagCharts", "plate procedure loads unavailable chart=${chart.id}", it) }
                    .getOrDefault(emptyList())
            }
        }
    }

    LaunchedEffect(bitmap, surfaceSize) {
        val currentBitmap = bitmap
        if (currentBitmap != null && surfaceSize.width > 0 && surfaceSize.height > 0 && viewport == null) {
            onViewportChange(createInitialImageViewport(
                imageWidthPx = currentBitmap.width.toFloat(),
                imageHeightPx = currentBitmap.height.toFloat(),
                viewportWidthPx = surfaceSize.width.toFloat(),
                viewportHeightPx = surfaceSize.height.toFloat(),
            ))
        }
    }
    LaunchedEffect(selectedChart?.id, trayOpen, folderOpen) {
        if (!trayOpen && !folderOpen) {
            withFrameNanos { }
            focusRequester.requestFocus()
        }
    }
    DisposableEffect(activity, selectedChart?.id, surfaceSize, bitmap, viewportState.value, trayOpen, folderOpen) {
        if (activity != null) {
            activity.onHardwareZoomDelta = { delta ->
                val currentState = viewportState.value
                if (bitmap == null || currentState == null || trayOpen || folderOpen) {
                    false
                } else {
                    onViewportChange(
                        zoomImageAroundPoint(
                            state = currentState,
                            anchorX = surfaceSize.width / 2f,
                            anchorY = surfaceSize.height / 2f,
                            nextZoom = currentState.zoom + delta.toFloat(),
                            imageWidthPx = imageWidthPx,
                            imageHeightPx = imageHeightPx,
                            viewportWidthPx = surfaceSize.width.toFloat(),
                            viewportHeightPx = surfaceSize.height.toFloat(),
                            overscrollPx = overscrollPx,
                        ),
                    )
                    true
                }
            }
        }
        onDispose {
            if (activity != null && activity.onHardwareZoomDelta != null) {
                activity.onHardwareZoomDelta = null
            }
        }
    }

    Box(
        modifier = Modifier
            .fillMaxSize()
            .background(uiTheme.controls.chartSurfaceBg)
            .onSizeChanged { surfaceSize = it }
            .focusRequester(focusRequester)
            .focusable()
            .onPreviewKeyEvent { event ->
                if (bitmap == null || viewportState.value == null || trayOpen || folderOpen || event.nativeKeyEvent.action != AndroidKeyEvent.ACTION_DOWN) {
                    return@onPreviewKeyEvent false
                }
                val nextZoom = when (event.nativeKeyEvent.keyCode) {
                    AndroidKeyEvent.KEYCODE_PLUS,
                    AndroidKeyEvent.KEYCODE_EQUALS,
                    AndroidKeyEvent.KEYCODE_NUMPAD_ADD -> (viewportState.value?.zoom ?: 1f) + 0.3f
                    AndroidKeyEvent.KEYCODE_MINUS,
                    AndroidKeyEvent.KEYCODE_NUMPAD_SUBTRACT -> (viewportState.value?.zoom ?: 1f) - 0.3f
                    else -> return@onPreviewKeyEvent false
                }
                onViewportChange(zoomImageAroundPoint(
                    state = viewportState.value ?: return@onPreviewKeyEvent false,
                    anchorX = surfaceSize.width / 2f,
                    anchorY = surfaceSize.height / 2f,
                    nextZoom = nextZoom,
                    imageWidthPx = imageWidthPx,
                    imageHeightPx = imageHeightPx,
                    viewportWidthPx = surfaceSize.width.toFloat(),
                    viewportHeightPx = surfaceSize.height.toFloat(),
                    overscrollPx = overscrollPx,
                ))
                true
            }
            .pointerInput(bitmap, surfaceSize, trayOpen) {
                if (bitmap == null || viewportState.value == null || trayOpen || folderOpen) {
                    return@pointerInput
                }
                detectTapGestures(
                    onDoubleTap = { tap ->
                        onViewportChange(zoomImageAroundPoint(
                            state = viewportState.value ?: return@detectTapGestures,
                            anchorX = tap.x,
                            anchorY = tap.y,
                            nextZoom = (viewportState.value?.zoom ?: 1f) + 0.75f,
                            imageWidthPx = imageWidthPx,
                            imageHeightPx = imageHeightPx,
                            viewportWidthPx = surfaceSize.width.toFloat(),
                            viewportHeightPx = surfaceSize.height.toFloat(),
                            overscrollPx = overscrollPx,
                        ))
                    },
                )
            }
            .pointerInput(bitmap, surfaceSize, trayOpen) {
                if (bitmap == null || viewportState.value == null || trayOpen || folderOpen) {
                    return@pointerInput
                }
                detectTransformGestures { centroid, pan, zoom, _ ->
                    val current = viewportState.value ?: return@detectTransformGestures
                    var next = zoomImageAroundPoint(
                        state = current,
                        anchorX = centroid.x,
                        anchorY = centroid.y,
                        nextZoom = current.zoom * zoom,
                        imageWidthPx = imageWidthPx,
                        imageHeightPx = imageHeightPx,
                        viewportWidthPx = surfaceSize.width.toFloat(),
                        viewportHeightPx = surfaceSize.height.toFloat(),
                        overscrollPx = overscrollPx,
                    )
                    next = dragImageViewport(
                        state = next,
                        dxPx = pan.x,
                        dyPx = pan.y,
                        imageWidthPx = imageWidthPx,
                        imageHeightPx = imageHeightPx,
                        viewportWidthPx = surfaceSize.width.toFloat(),
                        viewportHeightPx = surfaceSize.height.toFloat(),
                        overscrollPx = overscrollPx,
                    )
                    onViewportChange(next)
                }
            }
            .pointerInteropFilter { event ->
                if (bitmap == null || viewportState.value == null || trayOpen || folderOpen) {
                    return@pointerInteropFilter false
                }
                if (event.actionMasked == MotionEvent.ACTION_DOWN) {
                    focusRequester.requestFocus()
                }
                if (event.action == MotionEvent.ACTION_SCROLL) {
                    val wheelDelta = event.getAxisValue(MotionEvent.AXIS_VSCROLL).takeIf { it != 0f }
                        ?: event.getAxisValue(MotionEvent.AXIS_SCROLL)
                    onViewportChange(zoomImageAroundPoint(
                        state = viewportState.value ?: return@pointerInteropFilter false,
                        anchorX = surfaceSize.width / 2f,
                        anchorY = surfaceSize.height / 2f,
                        nextZoom = (viewportState.value?.zoom ?: 1f) - wheelDelta * 0.18f,
                        imageWidthPx = imageWidthPx,
                        imageHeightPx = imageHeightPx,
                        viewportWidthPx = surfaceSize.width.toFloat(),
                        viewportHeightPx = surfaceSize.height.toFloat(),
                        overscrollPx = overscrollPx,
                    ))
                    true
                } else {
                    false
                }
            },
    ) {
        if (folderOpen) {
            PlateFolderGrid(
                modifier = Modifier.fillMaxSize(),
                charts = sortedCharts,
                selectedChartId = selectedChart?.id,
                uiSession = uiSession,
                uiTheme = uiTheme,
                devServerBaseUrl = devServerBaseUrl,
                onSelectChart = {
                    onSelectChart(it)
                },
            )
        } else {
            val currentViewport = viewport
            val currentBitmap = bitmap
            val currentDisplaySize = if (currentViewport != null && currentBitmap != null) {
                imageDisplaySize(
                    imageWidthPx = currentBitmap.width.toFloat(),
                    imageHeightPx = currentBitmap.height.toFloat(),
                    viewportWidthPx = surfaceSize.width.toFloat(),
                    viewportHeightPx = surfaceSize.height.toFloat(),
                    zoom = currentViewport.zoom,
                )
            } else {
                null
            }
            val plateOwnshipOverlay = if (currentViewport != null && currentBitmap != null && currentDisplaySize != null) {
                resolvePlateOwnshipOverlay(
                    ownship = ownship,
                    georef = selectedChart?.georef,
                    imageWidthPx = currentBitmap.width.toFloat(),
                    imageHeightPx = currentBitmap.height.toFloat(),
                    viewport = currentViewport,
                    displaySize = currentDisplaySize,
                )
            } else {
                null
            }
            Canvas(modifier = Modifier.fillMaxSize()) {
                if (currentViewport != null && currentBitmap != null && currentDisplaySize != null) {
                    drawImage(
                        image = currentBitmap,
                        dstOffset = IntOffset(currentViewport.leftPx.roundToInt(), currentViewport.topPx.roundToInt()),
                        dstSize = IntSize(currentDisplaySize.widthPx.roundToInt(), currentDisplaySize.heightPx.roundToInt()),
                    )
                    drawRect(
                        color = Color(0x14000000),
                        topLeft = Offset(currentViewport.leftPx, currentViewport.topPx),
                        size = Size(currentDisplaySize.widthPx, currentDisplaySize.heightPx),
                        style = Stroke(width = 1.dp.toPx()),
                    )
                }
            }
            if (plateOwnshipOverlay != null) {
                Canvas(
                    modifier = Modifier
                        .fillMaxSize()
                        .testTag("parity:plate-ownship-overlay"),
                ) {
                    drawPlateOwnshipOverlay(plateOwnshipOverlay, aircraftDrawable)
                }
            }
        }

        if (page == AppPage.Map) {
            FlightDataBanner(
                banner = flightDataBanner,
                surfaceSize = surfaceSize,
                situationDockTopPadding = situationDockTopPadding,
                uiTheme = uiTheme,
                modifier = Modifier.align(if (surfaceSize.width > surfaceSize.height) Alignment.TopEnd else Alignment.TopCenter),
            )
        }

        DataStatusBadge(
            dataStatusState = dataStatusState,
            modifier = Modifier
                .align(Alignment.TopEnd)
                .padding(
                    top = situationDockTopPadding,
                    end = ThumbGap + MenuDockStyle.Situation.buttonWidth + ThumbGap,
                ),
            open = dataStatusTrayOpen,
            onToggle = {
                dataStatusTrayOpen = !dataStatusTrayOpen
                airportTrayOpen = false
                chartTrayOpen = false
                loadTrayOpen = false
                situationTrayOpen = false
            },
            onAction = onStatusAction,
        )

        SituationStatusBadge(
            controls = ownshipControls,
            modifier = Modifier
                .align(Alignment.TopEnd)
                .padding(top = situationDockTopPadding, end = ThumbGap),
            open = situationTrayOpen,
            onToggle = {
                situationTrayOpen = !situationTrayOpen
                airportTrayOpen = false
                chartTrayOpen = false
                loadTrayOpen = false
                dataStatusTrayOpen = false
            },
            onSelectSource = { sourceId ->
                situationTrayOpen = false
                onSelectOwnshipSource(sourceId)
            },
            onSituationControlInput = { input ->
                applySessionCommand {
                    uiSession.applySituationControlInput(input, System.currentTimeMillis().toDouble())
                }
            },
        )

        ChartViewerSelectors(
            modifier = Modifier.align(Alignment.TopStart),
            currentPage = page,
            airports = airports,
            selectedAirport = selectedAirport,
            selectedChart = selectedChart,
            folderOpen = folderOpen,
            airportTrayOpen = airportTrayOpen,
            chartTrayOpen = chartTrayOpen,
            loadTrayOpen = loadTrayOpen,
            plateProcedureLoads = plateProcedureLoads,
            onSelectPage = {
                onSelectPage(it)
                airportTrayOpen = false
                chartTrayOpen = false
                loadTrayOpen = false
                dataStatusTrayOpen = false
                situationTrayOpen = false
            },
            onToggleAirportTray = {
                airportTrayOpen = !airportTrayOpen
                chartTrayOpen = false
                loadTrayOpen = false
                dataStatusTrayOpen = false
                situationTrayOpen = false
            },
            onToggleChartTray = {
                chartTrayOpen = !chartTrayOpen
                airportTrayOpen = false
                loadTrayOpen = false
                dataStatusTrayOpen = false
                situationTrayOpen = false
            },
            onToggleLoadTray = {
                loadTrayOpen = !loadTrayOpen
                airportTrayOpen = false
                chartTrayOpen = false
                dataStatusTrayOpen = false
                situationTrayOpen = false
            },
            onToggleFolder = {
                onFolderOpenChange(!folderOpen)
                airportTrayOpen = false
                chartTrayOpen = false
                loadTrayOpen = false
                dataStatusTrayOpen = false
                situationTrayOpen = false
            },
            onSelectAirport = {
                onSelectAirport(it)
                airportTrayOpen = false
                loadTrayOpen = false
                dataStatusTrayOpen = false
                situationTrayOpen = false
            },
            onSelectChart = {
                onSelectChart(it)
                chartTrayOpen = false
                loadTrayOpen = false
                dataStatusTrayOpen = false
                situationTrayOpen = false
            },
            onSelectProcedureLoad = { loadId ->
                runCatching { uiSession.loadPlateProcedure(loadId) }
                    .onSuccess(onSessionSnapshotChange)
                    .onFailure { error ->
                        if (error is org.aerobag.app.domain.NativeSessionCommandRejectedException) {
                            onSessionCommandFailure(error)
                        } else {
                            Log.w("AerobagCharts", "plate procedure load failed", error)
                        }
                    }
                loadTrayOpen = false
                dataStatusTrayOpen = false
            },
        )

        if (trayOpen) {
            Scrim {
                airportTrayOpen = false
                chartTrayOpen = false
                loadTrayOpen = false
                dataStatusTrayOpen = false
                situationTrayOpen = false
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

internal data class PlateOwnshipOverlay(
    val screenX: Float,
    val screenY: Float,
    val headingDeg: Float,
)

private data class PlateImagePoint(
    val x: Double,
    val y: Double,
)

internal fun resolvePlateOwnshipOverlay(
    ownship: OwnshipRenderState,
    georef: PlateGeoref?,
    imageWidthPx: Float,
    imageHeightPx: Float,
    viewport: ImageViewportState,
    displaySize: ImageDisplaySize,
): PlateOwnshipOverlay? {
    if (imageWidthPx <= 0f || imageHeightPx <= 0f) return null
    if (!ownship.drawAircraft) return null
    val position = ownship.position ?: return null
    val chartGeoref = georef ?: return null
    val imagePoint = plateImagePoint(position, chartGeoref)
    if (imagePoint.x.isNaN() || imagePoint.x.isInfinite()) return null
    if (imagePoint.y.isNaN() || imagePoint.y.isInfinite()) return null
    if (imagePoint.x < 0.0 || imagePoint.x > imageWidthPx.toDouble()) return null
    if (imagePoint.y < 0.0 || imagePoint.y > imageHeightPx.toDouble()) return null

    val scaleX = displaySize.widthPx.toDouble() / imageWidthPx.toDouble()
    val scaleY = displaySize.heightPx.toDouble() / imageHeightPx.toDouble()
    return PlateOwnshipOverlay(
        screenX = (viewport.leftPx.toDouble() + imagePoint.x * scaleX).toFloat(),
        screenY = (viewport.topPx.toDouble() + imagePoint.y * scaleY).toFloat(),
        headingDeg = (ownship.orientationDeg ?: 0.0).toFloat(),
    )
}

private fun plateImagePoint(position: LatLonPoint, georef: PlateGeoref): PlateImagePoint =
    when (georef) {
        is PlateGeoref.PlateTransformV1 -> PlateImagePoint(
            x = (position.lon - georef.topLeftLon) * georef.pixelsPerLongitude,
            y = (position.lat - georef.topLeftLat) * georef.pixelsPerLatitude,
        )
        is PlateGeoref.AirportDiagramTransformV1 -> PlateImagePoint(
            x = position.lon * georef.pixelXFromLon +
                position.lat * georef.pixelXFromLat +
                georef.pixelXOffset,
            y = position.lon * georef.pixelYFromLon +
                position.lat * georef.pixelYFromLat +
                georef.pixelYOffset,
        )
    }

private fun androidx.compose.ui.graphics.drawscope.DrawScope.drawPlateOwnshipOverlay(
    overlay: PlateOwnshipOverlay,
    aircraftDrawable: android.graphics.drawable.Drawable?,
) {
    val center = Offset(overlay.screenX, overlay.screenY)
    val iconSizePx = ThumbSize.toPx() * 0.72f
    if (aircraftDrawable == null) {
        drawCircle(Color.White, radius = iconSizePx * 0.24f, center = center)
        drawCircle(Color(0x66000000), radius = iconSizePx * 0.24f, center = center, style = Stroke(width = 2f))
        return
    }
    val left = (center.x - iconSizePx / 2f).roundToInt()
    val top = (center.y - iconSizePx / 2f).roundToInt()
    drawContext.canvas.nativeCanvas.apply {
        save()
        rotate(overlay.headingDeg, center.x, center.y)
        aircraftDrawable.setBounds(left, top, (left + iconSizePx).roundToInt(), (top + iconSizePx).roundToInt())
        aircraftDrawable.draw(this)
        restore()
    }
}

@Composable
internal fun ChartPlateToggleButton(
    currentPage: AppPage,
    onSelectPage: (AppPage) -> Unit,
) {
    val targetPage = if (currentPage == AppPage.Map) AppPage.Charts else AppPage.Map
    val option = PageOptions.firstOrNull { it.page == currentPage }
        ?: PageOptions.first { it.page == AppPage.Map }
    Box(modifier = Modifier.size(ThumbSize)) {
        CompactSquareButton(
            label = option.launcherLabel,
            modifier = Modifier.matchParentSize(),
            selected = currentPage == AppPage.Map || currentPage == AppPage.Charts,
            iconResId = option.iconResId,
            onClick = { onSelectPage(targetPage) },
        )
        PageToggleIndicator(
            chartSelected = currentPage == AppPage.Map,
            modifier = Modifier
                .align(Alignment.TopCenter)
                .padding(top = 3.dp),
        )
    }
}

@Composable
internal fun ChartPlateReturnButton(
    targetPage: AppPage,
    onClick: () -> Unit,
) {
    val chartPage = if (targetPage == AppPage.Charts) AppPage.Charts else AppPage.Map
    val option = PageOptions.firstOrNull { it.page == chartPage }
        ?: PageOptions.first { it.page == AppPage.Map }
    CompactSquareButton(
        label = option.launcherLabel,
        modifier = Modifier.size(ThumbSize),
        iconResId = option.iconResId,
        onClick = onClick,
    )
}

@Composable
internal fun HomePageButton(
    currentPage: AppPage,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val homeOption = PageOptions.first { it.page == AppPage.Home }
    CompactSquareButton(
        label = homeOption.launcherLabel,
        modifier = modifier.size(ThumbSize),
        selected = currentPage == AppPage.Home,
        iconResId = homeOption.iconResId,
        onClick = onClick,
    )
}

@Composable
internal fun HomeReturnDock(
    modifier: Modifier = Modifier,
    currentPage: AppPage,
    chartPlateTargetPage: AppPage,
    onHomeClick: () -> Unit,
    onOpenChartOrPlate: () -> Unit,
) {
    Row(
        modifier = modifier.padding(ThumbGap),
        horizontalArrangement = Arrangement.spacedBy(ThumbGap),
        verticalAlignment = Alignment.Top,
    ) {
        HomePageButton(
            currentPage = currentPage,
            onClick = onHomeClick,
        )
        ChartPlateReturnButton(
            targetPage = chartPlateTargetPage,
            onClick = onOpenChartOrPlate,
        )
    }
}

@Composable
internal fun PageToggleIndicator(
    chartSelected: Boolean,
    modifier: Modifier = Modifier,
) {
    val knobOffset by animateDpAsState(
        targetValue = if (chartSelected) 0.dp else ThumbSize * 0.30f,
        label = "pageToggleOffset",
    )
    Box(
        modifier = modifier
            .width(ThumbSize * 0.48f)
            .height(ThumbSize * 0.18f)
            .clip(RoundedCornerShape(999.dp))
            .background(Color.Black.copy(alpha = 0.55f))
            .border(1.dp, Color.White.copy(alpha = 0.62f), RoundedCornerShape(999.dp)),
    ) {
        Box(
            modifier = Modifier
                .padding(1.dp)
                .offset(x = knobOffset)
                .align(Alignment.CenterStart)
                .size(ThumbSize * 0.14f)
                .clip(CircleShape)
                .background(Color.White),
        )
    }
}

@Composable
internal fun MapTopLeftControls(
    modifier: Modifier = Modifier,
    currentPage: AppPage,
    onSelectPage: (AppPage) -> Unit,
    selectedLabel: String,
    trayOptions: List<ChartTrayOption>,
    trayOpen: Boolean,
    onToggle: () -> Unit,
    layerTrayOpen: Boolean,
    onToggleLayerTray: () -> Unit,
    layerOptions: List<MenuDockOption>,
    chartSearchText: String,
    chartSearchOpen: Boolean,
    chartSearchLoading: Boolean,
    chartSearchError: String?,
    chartSearchSuggestions: List<WaypointIdentifierSuggestion>,
    onChartSearchTextChange: (String) -> Unit,
    onChartSearchFocus: () -> Unit,
    onChartSearchSubmit: () -> Unit,
    onChartSearchSuggestionClick: (WaypointIdentifierSuggestion) -> Unit,
) {
    Row(
        modifier = modifier.padding(ThumbGap),
        horizontalArrangement = Arrangement.spacedBy(ThumbGap),
        verticalAlignment = Alignment.Top,
    ) {
        HomePageButton(
            currentPage = currentPage,
            onClick = { onSelectPage(AppPage.Home) },
        )
        ChartPlateToggleButton(
            currentPage = currentPage,
            onSelectPage = onSelectPage,
        )
        MenuDock(
            launcherLabel = selectedLabel,
            launcherIconResId = trayOptions.firstOrNull { it.launcherLabel == selectedLabel }?.iconResId,
            launcherTestTag = "parity:chart-family-button",
            optionTestTagPrefix = "parity:tray-option",
            open = trayOpen,
            onToggle = onToggle,
            style = MenuDockStyle.Compact,
            options = trayOptions.map { option ->
                MenuDockOption(option.id, option.label, active = option.launcherLabel == selectedLabel, enabled = option.available, iconResId = option.iconResId) { option.select?.invoke() }
            },
        )
        MenuDock(
            launcherLabel = "LAYERS",
            launcherIconResId = mapLayerIconResId(MapLayerId.Vectors),
            launcherTestTag = "parity:layers-button",
            optionTestTagPrefix = "parity:tray-option",
            open = layerTrayOpen,
            onToggle = onToggleLayerTray,
            style = MenuDockStyle.Layers,
            options = layerOptions,
        )
        AndroidChartSearchBox(
            text = chartSearchText,
            open = chartSearchOpen,
            loading = chartSearchLoading,
            error = chartSearchError,
            suggestions = chartSearchSuggestions,
            onTextChange = onChartSearchTextChange,
            onFocus = onChartSearchFocus,
            onSubmit = onChartSearchSubmit,
            onSuggestionClick = onChartSearchSuggestionClick,
        )
    }
}

@Composable
internal fun AndroidChartSearchBox(
    text: String,
    open: Boolean,
    loading: Boolean,
    error: String?,
    suggestions: List<WaypointIdentifierSuggestion>,
    onTextChange: (String) -> Unit,
    onFocus: () -> Unit,
    onSubmit: () -> Unit,
    onSuggestionClick: (WaypointIdentifierSuggestion) -> Unit,
) {
    val uiTheme = LocalAerobagUiTheme.current
    val keyboardController = LocalSoftwareKeyboardController.current
    val showTray = open && (text.isNotBlank() || loading || error != null || suggestions.isNotEmpty())
    Box {
        BasicTextField(
            value = text,
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
            keyboardActions =
                KeyboardActions(
                    onDone = {
                        keyboardController?.hide()
                        onSubmit()
                    },
                ),
            textStyle =
                MaterialTheme.typography.titleMedium.copy(
                    color = uiTheme.controls.panelFg,
                    fontWeight = FontWeight.ExtraBold,
                    textAlign = TextAlign.Center,
                ),
            modifier =
                Modifier
                    .testTag("chart-search-input")
                    .width(ThumbSize * 2f)
                    .height(ThumbSize)
                    .clip(RoundedCornerShape(ThumbRadius))
                    .background(Color.White.copy(alpha = 0.96f))
                    .border(1.5.dp, uiTheme.controls.panelBorder, RoundedCornerShape(ThumbRadius))
                    .onFocusChanged { state -> if (state.isFocused) onFocus() }
                    .onPreviewKeyEvent { event ->
                        if (event.nativeKeyEvent.action == AndroidKeyEvent.ACTION_DOWN &&
                            event.nativeKeyEvent.keyCode == AndroidKeyEvent.KEYCODE_ENTER
                        ) {
                            keyboardController?.hide()
                            onSubmit()
                            true
                        } else {
                            false
                        }
                    }
                    .padding(horizontal = ThumbGap, vertical = ThumbSize * 0.22f),
            decorationBox = { innerTextField ->
                Box(modifier = Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
                    if (text.isBlank()) {
                        Text(
                            text = "SEARCH",
                            style = MaterialTheme.typography.titleMedium.copy(fontWeight = FontWeight.ExtraBold),
                            color = Color(0x884E626C),
                        )
                    }
                    innerTextField()
                }
            },
        )
        if (showTray) {
            Surface(
                modifier =
                    Modifier
                        .align(Alignment.TopStart)
                        .padding(top = ThumbSize + ThumbGap)
                        .width(ThumbSize * 3.4f),
                shape = RoundedCornerShape(ThumbRadius),
                color = Color(0xF7FCF8F1),
                contentColor = uiTheme.controls.panelFg,
                border = BorderStroke(1.dp, uiTheme.controls.panelBorder),
                shadowElevation = 8.dp,
            ) {
                Column(modifier = Modifier.padding(ThumbGap), verticalArrangement = Arrangement.spacedBy(ThumbGap)) {
                    if (loading) {
                        Text("Searching...", style = MaterialTheme.typography.labelSmall, color = uiTheme.controls.panelMuted)
                    }
                    error?.let {
                        Text(it, style = MaterialTheme.typography.labelSmall, color = Color(0xFFAA2233), maxLines = 2, overflow = TextOverflow.Ellipsis)
                    }
                    if (!loading && error == null && text.isNotBlank() && suggestions.isEmpty()) {
                        Text("No matches", style = MaterialTheme.typography.labelSmall, color = uiTheme.controls.panelMuted)
                    }
                    suggestions.forEach { suggestion ->
                        Surface(
                            modifier =
                                Modifier
                                    .fillMaxWidth()
                                    .height(ThumbSize)
                                    .testTag("chart-search-suggestion-${suggestion.identifier}")
                                    .clickable {
                                        keyboardController?.hide()
                                        onSuggestionClick(suggestion)
                                    },
                            shape = RoundedCornerShape(ThumbRadius),
                            color = uiTheme.controls.buttonUnchecked,
                            contentColor = uiTheme.controls.buttonFg,
                            border = BorderStroke(1.dp, lerp(uiTheme.controls.buttonUnchecked, Color.Black, 0.22f)),
                        ) {
                            Row(
                                modifier = Modifier.fillMaxSize().padding(horizontal = ThumbGap),
                                verticalAlignment = Alignment.CenterVertically,
                                horizontalArrangement = Arrangement.SpaceBetween,
                            ) {
                                Column(modifier = Modifier.weight(1f)) {
                                    Text(suggestion.identifier, style = MaterialTheme.typography.labelLarge, fontWeight = FontWeight.ExtraBold)
                                    if (suggestion.displayName.isNotBlank()) {
                                        Text(
                                            suggestion.displayName,
                                            style = MaterialTheme.typography.labelSmall,
                                            maxLines = 1,
                                            overflow = TextOverflow.Ellipsis,
                                        )
                                    }
                                }
                                Text(
                                    "${suggestion.kind.uppercase()} ${"%.1f".format(suggestion.distanceFromAnchorNm)}nm",
                                    style = MaterialTheme.typography.labelSmall,
                                )
                            }
                        }
                    }
                }
            }
        }
    }
}

@Composable
internal fun ChartViewerSelectors(
    modifier: Modifier = Modifier,
    currentPage: AppPage,
    airports: List<ChartAirport>,
    selectedAirport: ChartAirport?,
    selectedChart: ChartAsset?,
    folderOpen: Boolean,
    airportTrayOpen: Boolean,
    chartTrayOpen: Boolean,
    loadTrayOpen: Boolean,
    plateProcedureLoads: List<ProcedureLoadOption>,
    onSelectPage: (AppPage) -> Unit,
    onToggleAirportTray: () -> Unit,
    onToggleChartTray: () -> Unit,
    onToggleLoadTray: () -> Unit,
    onToggleFolder: () -> Unit,
    onSelectAirport: (String) -> Unit,
    onSelectChart: (String) -> Unit,
    onSelectProcedureLoad: (String) -> Unit,
) {
    val uiTheme = LocalAerobagUiTheme.current
    val trayOpen = airportTrayOpen || chartTrayOpen || loadTrayOpen
    BoxWithConstraints(modifier = modifier) {
        val rowHorizontalPadding = ThumbGap * 2f
        val rowGaps = ThumbGap * 5f
        val fixedButtonsWidth = ThumbSize * 5f
        val chartButtonWidth = (maxWidth - rowHorizontalPadding - rowGaps - fixedButtonsWidth)
            .coerceIn(ThumbSize, MenuDockStyle.PlateWide.buttonWidth)
        Row(
            modifier = Modifier.padding(ThumbGap),
            horizontalArrangement = Arrangement.spacedBy(ThumbGap),
            verticalAlignment = Alignment.Top,
        ) {
            HomePageButton(
                currentPage = currentPage,
                onClick = { onSelectPage(AppPage.Home) },
            )

            ChartPlateToggleButton(
                currentPage = currentPage,
                onSelectPage = onSelectPage,
            )

            MenuDock(
                launcherLabel = selectedAirport?.id ?: "---",
                launcherTestTag = "parity:plate-airport-button",
                optionTestTagPrefix = "parity:tray-option",
                open = airportTrayOpen,
                onToggle = onToggleAirportTray,
                style = MenuDockStyle.PlateAirport,
                options = airports.map { airport ->
                    MenuDockOption(airport.id, airport.id, active = airport.id == selectedAirport?.id) { onSelectAirport(airport.id) }
                },
            )

            MenuDock(
                launcherLabel = selectedChart?.label ?: "---",
                launcherTestTag = "parity:plate-chart-button",
                optionTestTagPrefix = "parity:tray-option",
                open = chartTrayOpen,
                onToggle = onToggleChartTray,
                style = MenuDockStyle.PlateWide,
                buttonWidthOverride = chartButtonWidth,
                options = (selectedAirport?.charts ?: emptyList()).map { chart ->
                    MenuDockOption(
                        chart.id,
                        chart.label,
                        active = chart.id == selectedChart?.id,
                        accentColor = plateFolderColor(uiTheme, chart.folderCategory),
                    ) { onSelectChart(chart.id) }
                },
            )

            MenuDock(
                launcherLabel = "LOAD\nAPPCH",
                launcherTestTag = "parity:plate-load-button",
                optionTestTagPrefix = "parity:tray-option",
                open = loadTrayOpen,
                onToggle = onToggleLoadTray,
                style = MenuDockStyle.Compact,
                disabled = plateProcedureLoads.isEmpty(),
                options = plateProcedureLoads.map { load ->
                    MenuDockOption(load.loadId, load.label) { onSelectProcedureLoad(load.loadId) }
                },
            )

            CompactSquareButton(
                label = "FLDR",
                modifier = Modifier.size(ThumbSize),
                testTag = "parity:plate-folder-button",
                enabled = !trayOpen,
                selected = folderOpen,
                onClick = onToggleFolder,
            )
        }
    }
}

@Composable
internal fun PlateFolderGrid(
    modifier: Modifier = Modifier,
    charts: List<ChartAsset>,
    selectedChartId: String?,
    uiSession: NativeUiSession,
    uiTheme: UiTheme,
    devServerBaseUrl: String,
    onSelectChart: (String) -> Unit,
) {
    val context = LocalContext.current
    LazyVerticalGrid(
        columns = GridCells.Adaptive(minSize = PlateFolderTileWidth),
        modifier = modifier.padding(top = ThumbSize + (ThumbGap * 2f), start = FolderThumbGutter, end = FolderThumbGutter, bottom = FolderThumbGutter),
        horizontalArrangement = Arrangement.spacedBy(FolderThumbGutter),
        verticalArrangement = Arrangement.spacedBy(FolderThumbGutter),
    ) {
        lazyGridItems(charts, key = { it.id }) { chart ->
            val thumbnail by produceState<androidx.compose.ui.graphics.ImageBitmap?>(initialValue = null, chart.id, chart.hasThumbnail, uiSession, devServerBaseUrl) {
                value = if (chart.hasThumbnail) {
                    withContext(Dispatchers.IO) {
                        var attemptedResource: CoreResourceRequest? = null
                        runCatching {
                            val bytes = uiSession.chartAssetBytes(chart.id, "thumbnail") { resource ->
                                attemptedResource = resource
                                fetchCoreResource(context, resource, devServerBaseUrl)
                            }
                            BitmapFactory.decodeByteArray(bytes, 0, bytes.size)?.asImageBitmap()
                                ?: error(
                                    "failed to decode plate thumbnail bytes for ${chart.id} " +
                                        "source=${attemptedResource?.source?.describeForLog() ?: "unresolved"}",
                                )
                        }.onFailure { error ->
                            Log.w(
                                "AerobagCharts",
                                "plate thumbnail unavailable chart=${chart.id} " +
                                    "source=${attemptedResource?.source?.describeForLog() ?: "unresolved"}",
                                error,
                            )
                        }.getOrNull()
                    }
                } else {
                    null
                }
            }
            Surface(
                modifier = Modifier
                    .width(PlateFolderTileWidth)
                    .height(PlateFolderTileHeight)
                    .border(
                        width = if (chart.id == selectedChartId) 2.dp else 1.dp,
                        color = if (chart.id == selectedChartId) MaterialTheme.colorScheme.primary else Color(0x26132129),
                        shape = RoundedCornerShape(ThumbRadius),
                    )
                    .clickable { onSelectChart(chart.id) },
                shape = RoundedCornerShape(ThumbRadius),
                color = uiTheme.plateFolder.thumbnailBg,
                shadowElevation = 2.dp,
            ) {
                Box {
                    if (thumbnail != null) {
                        androidx.compose.foundation.Image(
                            bitmap = thumbnail!!,
                            contentDescription = null,
                            modifier = Modifier.fillMaxSize(),
                        )
                    }
                    Box(
                        modifier = Modifier
                            .align(Alignment.BottomStart)
                            .fillMaxWidth()
                            .background(plateFolderColor(uiTheme, chart.folderCategory))
                            .padding(horizontal = 6.dp, vertical = 5.dp),
                    ) {
                        Text(
                            text = chart.label,
                            style = MaterialTheme.typography.labelSmall,
                            color = Color(0xFFFFF7EF),
                            maxLines = 3,
                            overflow = TextOverflow.Ellipsis,
                        )
                    }
                }
            }
        }
    }
}

@Composable
@OptIn(ExperimentalComposeUiApi::class)
internal fun MenuDock(
    modifier: Modifier = Modifier,
    launcherLabel: String,
    @DrawableRes launcherIconResId: Int? = null,
    launcherTestTag: String? = null,
    optionTestTagPrefix: String? = null,
    open: Boolean,
    onToggle: () -> Unit,
    style: MenuDockStyle,
    buttonWidthOverride: Dp? = null,
    trayWidthOverride: Dp? = null,
    launcherForegroundColor: Color? = null,
    disabled: Boolean = false,
    options: List<MenuDockOption>,
    body: (@Composable ColumnScope.() -> Unit)? = null,
    footer: (@Composable ColumnScope.() -> Unit)? = null,
) {
    val density = LocalDensity.current
    val configuration = LocalConfiguration.current
    var anchorTopPx by remember { mutableStateOf(0f) }
    val screenHeightPx = with(density) { configuration.screenHeightDp.dp.toPx() }
    val launcherAccentColor = options.firstOrNull { it.active }?.accentColor
    val buttonWidth = buttonWidthOverride ?: style.buttonWidth
    val buttonHeight = style.buttonHeight
    val trayWidth = trayWidthOverride ?: style.trayWidth
    val trayOffsetPx = with(density) { (buttonHeight + ThumbGap).toPx() }
    val trayBottomMarginPx = with(density) { ThumbGap.toPx() }
    val trayMaxHeight = with(density) {
        ((screenHeightPx - anchorTopPx - trayOffsetPx - trayBottomMarginPx).coerceAtLeast(ThumbSize.toPx())).toDp()
    }
    val uiTheme = LocalAerobagUiTheme.current
    val situationLauncher = style == MenuDockStyle.Situation
    Box(
        modifier = modifier
            .width(buttonWidth)
            .height(buttonHeight)
            .wrapContentSize(unbounded = true, align = Alignment.TopStart),
    ) {
        CompactSquareButton(
            label = launcherLabel,
            iconResId = launcherIconResId,
            maxLines = style.launcherMaxLines,
            enabled = !disabled,
            selected = open && !situationLauncher,
            backgroundColor = if (situationLauncher) uiTheme.controls.situationStatusBg else null,
            foregroundColor = if (situationLauncher) launcherForegroundColor ?: uiTheme.controls.situationStatusFg else null,
            accentColor = launcherAccentColor,
            wide = style != MenuDockStyle.Compact,
            testTag = launcherTestTag,
            modifier = Modifier
                .width(buttonWidth)
                .height(buttonHeight)
                .align(Alignment.TopStart)
                .onGloballyPositioned { coordinates ->
                    anchorTopPx = coordinates.boundsInWindow().top
                },
            onClick = onToggle,
        )
        if (open) {
            Popup(
                offset = IntOffset(0, trayOffsetPx.roundToInt()),
                onDismissRequest = onToggle,
                properties = PopupProperties(focusable = true),
            ) {
                MenuPanel(
                    modifier = Modifier
                        .semantics { testTagsAsResourceId = true }
                        .width(trayWidth)
                        .heightIn(max = trayMaxHeight),
                ) {
                    if (body != null) {
                        body()
                    } else {
                        LazyColumn(verticalArrangement = Arrangement.spacedBy(3.dp)) {
                            lazyColumnItems(options) { option ->
                                MenuPanelRow(
                                    label = option.label,
                                    active = option.active,
                                    enabled = option.enabled,
                                    accentColor = option.accentColor,
                                    toggleState = option.toggleState,
                                    iconResId = option.iconResId,
                                    testTag = optionTestTagPrefix?.let { "$it:${option.key}" },
                                    width = trayWidth,
                                    onSelect = option.onSelect,
                                )
                            }
                        }
                    }
                    footer?.invoke(this)
                }
            }
        }
    }
}

@Composable
internal fun MenuPanel(
    modifier: Modifier = Modifier,
    width: Dp = Dp.Unspecified,
    content: @Composable ColumnScope.() -> Unit,
) {
    val uiTheme = LocalAerobagUiTheme.current
    Card(
        modifier = modifier.then(if (width != Dp.Unspecified) Modifier.width(width) else Modifier),
        shape = RoundedCornerShape(ThumbRadius + 2.dp),
        colors = CardDefaults.cardColors(
            containerColor = uiTheme.controls.panelBg,
            contentColor = uiTheme.controls.panelFg,
        ),
        border = BorderStroke(2.dp, uiTheme.controls.panelBorder),
    ) {
        Column(
            modifier = Modifier.padding(3.dp),
            verticalArrangement = Arrangement.spacedBy(3.dp),
            content = content,
        )
    }
}

@Composable
internal fun MenuPanelRow(
    label: String,
    active: Boolean,
    enabled: Boolean,
    accentColor: Color? = null,
    toggleState: UiMapLayerToggleState? = null,
    @DrawableRes iconResId: Int? = null,
    testTag: String? = null,
    modifier: Modifier = Modifier,
    width: Dp = Dp.Unspecified,
    maxLines: Int = 2,
    onSelect: () -> Unit,
) {
    val uiTheme = LocalAerobagUiTheme.current
    val rowShape = RoundedCornerShape(ThumbRadius)
    val isOn = toggleState?.enabled == true && toggleState.visible
    val isOff = toggleState?.enabled == true && !toggleState.visible
    val rowBackground = when {
        !enabled -> uiTheme.controls.buttonDisabled
        isOn -> uiTheme.controls.buttonChecked
        isOff -> uiTheme.controls.buttonUnchecked
        active -> uiTheme.controls.buttonChecked
        else -> uiTheme.controls.buttonUnchecked
    }
    val rowTextColor = uiTheme.controls.buttonFg
    val renderedLabel = buttonLabel(label)
    val renderedLabelStyle = buttonLabelStyle()
    Box(
        modifier = modifier
            .then(if (width != Dp.Unspecified) Modifier.width(width) else Modifier.fillMaxWidth())
            .height(ThumbSize)
            .then(if (testTag != null) Modifier.testTag(testTag) else Modifier)
            .semantics { selected = active || isOn }
            .clip(rowShape)
            .background(rowBackground)
            .clickable(
                enabled = enabled,
                indication = null,
                interactionSource = remember { MutableInteractionSource() },
            ) {
                onSelect()
            },
        contentAlignment = Alignment.CenterStart,
    ) {
        if (accentColor != null) {
            Box(
                modifier = Modifier
                    .fillMaxWidth()
                    .height(ThumbSize / 2f)
                    .align(Alignment.BottomStart)
                    .background(accentColor.copy(alpha = if (enabled) 1f else 0.45f)),
            )
        }
        if (iconResId != null || toggleState != null) {
            Row(
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(horizontal = 8.dp),
                horizontalArrangement = Arrangement.spacedBy(8.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                if (iconResId != null) {
                    IconFrame(
                        iconResId = iconResId,
                        modifier = Modifier.size(ThumbSize * 0.72f),
                    )
                }
                Text(
                    text = renderedLabel,
                    modifier = Modifier.weight(1f),
                    style = renderedLabelStyle,
                    maxLines = maxLines,
                    overflow = TextOverflow.Ellipsis,
                    color = rowTextColor,
                )
                if (toggleState != null) {
                    LayerToggle(
                        visible = toggleState.visible,
                        enabled = toggleState.enabled,
                        modifier = Modifier.padding(start = 4.dp),
                    )
                }
            }
        } else {
            Text(
                text = renderedLabel,
                modifier = Modifier.padding(horizontal = 12.dp),
                style = renderedLabelStyle,
                maxLines = maxLines,
                overflow = TextOverflow.Ellipsis,
                color = rowTextColor,
            )
        }
    }
}

@Composable
internal fun NavElementDock(
    navElement: NavElementUiView?,
    modifier: Modifier = Modifier,
    onClick: (() -> Unit)? = null,
) {
    val uiTheme = LocalAerobagUiTheme.current
    val displayedActiveLegSummary = navElement?.activeLegSummary ?: "NO ACTIVE LEG"
    val displayedTextColor = if (navElement == null) {
        Color.White.copy(alpha = 0.72f)
    } else {
        Color.White
    }
    val shape = RoundedCornerShape(ThumbRadius * 0.9f)
    Surface(
        modifier =
            modifier
                .width(ThumbSize * 3f)
                .height(ThumbSize * 0.67f)
                .testTag("parity:nav-cdi")
                .then(
                    if (onClick != null) {
                        Modifier.clickable(
                            indication = null,
                            interactionSource = remember { MutableInteractionSource() },
                        ) { onClick() }
                    } else {
                        Modifier
                    },
                ),
        shape = shape,
        color = uiTheme.controls.panelFg,
        contentColor = Color.White,
        border = BorderStroke(1.dp, uiTheme.controls.panelBorder),
        shadowElevation = 6.dp,
    ) {
        Column(
            modifier = Modifier.fillMaxSize().padding(horizontal = ThumbSize * 0.14f),
            horizontalAlignment = Alignment.CenterHorizontally,
            verticalArrangement = Arrangement.Center,
        ) {
            Box(
                modifier = Modifier.weight(1f).fillMaxWidth(),
                contentAlignment = Alignment.Center,
            ) {
                Text(
                    text = displayedActiveLegSummary,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                    style = MaterialTheme.typography.labelSmall,
                    fontWeight = FontWeight.Bold,
                    color = displayedTextColor,
                )
            }
            Canvas(
                modifier = Modifier
                    .weight(1f)
                    .fillMaxWidth(),
            ) {
                val unit = size.width / 4.5f
                val centerX = 2.25f * unit
                val baselineY = size.height * 0.5f
                val dotXs = listOf(0.25f, 1.25f, 3.25f, 4.25f).map { it * unit }
                val dotRadius = unit * 0.04375f
                val pointerPosition = navElement?.cdiIndicatorDots
                val fullScaleDots = 2f
                val offscaleDots = 2.1f
                val clampedPointerPosition = pointerPosition?.coerceIn(-fullScaleDots, fullScaleDots)
                val pointerX = clampedPointerPosition?.let { (it + 2.25f) * unit }
                val offscaleDirection = when {
                    pointerPosition == null || abs(pointerPosition) <= offscaleDots -> null
                    pointerPosition > 0f -> 1
                    else -> -1
                }
                val offscaleReadout = navElement?.cdiOffscaleReadout
                val offscaleReadoutDotIndex = when {
                    offscaleReadout == null -> null
                    offscaleDirection == 1 -> 2
                    offscaleDirection == -1 -> 1
                    else -> null
                }
                val triangleHalfWidth = unit * 0.25f
                drawPath(
                    path =
                        Path().apply {
                            moveTo(centerX - triangleHalfWidth, size.height)
                            lineTo(centerX + triangleHalfWidth, size.height)
                            lineTo(centerX, 0f)
                            close()
                        },
                    color = Color.White,
                )
                dotXs.forEachIndexed { index, x ->
                    if (index == offscaleReadoutDotIndex) {
                        return@forEachIndexed
                    }
                    drawCircle(
                        color = Color.White,
                        radius = dotRadius,
                        center = Offset(x, baselineY),
                        style = Stroke(width = unit * 0.05f),
                    )
                }
                if (offscaleDirection != null) {
                    val baseX = (2.25f + offscaleDirection * offscaleDots) * unit
                    val tipX = if (offscaleDirection > 0) size.width else 0f
                    drawPath(
                        path =
                            Path().apply {
                                moveTo(baseX, size.height * 0.18f)
                                lineTo(baseX, size.height * 0.82f)
                                lineTo(tipX, baselineY)
                                close()
                            },
                        color = uiTheme.controls.cdiPointer,
                    )
                    if (offscaleReadout != null && offscaleReadoutDotIndex != null) {
                        val textPaint = Paint(Paint.ANTI_ALIAS_FLAG).apply {
                            color = uiTheme.controls.cdiPointer.toArgb()
                            textAlign = Paint.Align.CENTER
                            textSize = size.height * 0.8f
                            typeface = Typeface.create(Typeface.DEFAULT, Typeface.BOLD)
                        }
                        val strokePaint = Paint(textPaint).apply {
                            style = Paint.Style.STROKE
                            strokeWidth = size.height * 0.14f
                            strokeJoin = Paint.Join.ROUND
                            color = Color(0xD1081218).toArgb()
                        }
                        val textY = baselineY - (textPaint.descent() + textPaint.ascent()) / 2f
                        drawContext.canvas.nativeCanvas.drawText(
                            offscaleReadout,
                            dotXs[offscaleReadoutDotIndex],
                            textY,
                            strokePaint,
                        )
                        drawContext.canvas.nativeCanvas.drawText(
                            offscaleReadout,
                            dotXs[offscaleReadoutDotIndex],
                            textY,
                            textPaint,
                        )
                    }
                } else if (pointerX != null) {
                    drawLine(
                        color = uiTheme.controls.cdiPointer,
                        start = Offset(pointerX, 0f),
                        end = Offset(pointerX, size.height),
                        strokeWidth = unit * 0.14f,
                        cap = StrokeCap.Round,
                    )
                }
            }
        }
    }
}
