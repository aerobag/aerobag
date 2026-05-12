package net.jonh.aerobag.prototype

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
import net.jonh.aerobag.prototype.domain.ChartAirport
import net.jonh.aerobag.prototype.domain.ChartAsset
import net.jonh.aerobag.prototype.domain.ChartPackages
import net.jonh.aerobag.prototype.domain.AppState
import net.jonh.aerobag.prototype.domain.AirwayPresentationPlan
import net.jonh.aerobag.prototype.domain.AirwaySuggestion
import net.jonh.aerobag.prototype.domain.WaypointIdentifierSuggestion
import net.jonh.aerobag.prototype.domain.CoreMapViewport
import net.jonh.aerobag.prototype.domain.DerivedChartPageState
import net.jonh.aerobag.prototype.domain.FlightPlan
import net.jonh.aerobag.prototype.domain.FlightPlanEntryPreview
import net.jonh.aerobag.prototype.domain.FlightPlanUiMutation
import net.jonh.aerobag.prototype.domain.FlightPlanDisplayRowKind
import net.jonh.aerobag.prototype.domain.FlightPlanDisplayRowUiView
import net.jonh.aerobag.prototype.domain.FlightPlanRowActionUiView
import net.jonh.aerobag.prototype.domain.FlightPlanRouteSegment
import net.jonh.aerobag.prototype.domain.FlightPlanUiState
import net.jonh.aerobag.prototype.domain.GuidanceState
import net.jonh.aerobag.prototype.domain.InstalledPackages
import net.jonh.aerobag.prototype.domain.AirspaceDisplayDecoration
import net.jonh.aerobag.prototype.domain.AirspaceDisplayLabel
import net.jonh.aerobag.prototype.domain.AirspaceDisplayPath
import net.jonh.aerobag.prototype.domain.AirspaceDisplaySubpath
import net.jonh.aerobag.prototype.domain.AirspaceLimitGlyph
import net.jonh.aerobag.prototype.domain.AirspaceScreenPoint
import net.jonh.aerobag.prototype.domain.LatLonPoint
import net.jonh.aerobag.prototype.domain.MapChartFamily
import net.jonh.aerobag.prototype.domain.MapLayerId
import net.jonh.aerobag.prototype.domain.MapFollowUiState
import net.jonh.aerobag.prototype.domain.MapOverlayQueryResult
import net.jonh.aerobag.prototype.domain.MapSelectionAction
import net.jonh.aerobag.prototype.domain.MapSelectionHighlight
import net.jonh.aerobag.prototype.domain.MapSelectionItem
import net.jonh.aerobag.prototype.domain.MapSelectionQueryResult
import net.jonh.aerobag.prototype.domain.MapFamilyOption
import net.jonh.aerobag.prototype.domain.MapViewportState
import net.jonh.aerobag.prototype.domain.NativeAppCoreAdapter
import net.jonh.aerobag.prototype.domain.NativeBindings
import net.jonh.aerobag.prototype.domain.NativeUiSession
import net.jonh.aerobag.prototype.domain.NavKvStore
import net.jonh.aerobag.prototype.domain.NavRef
import net.jonh.aerobag.prototype.domain.NavElementUiView
import net.jonh.aerobag.prototype.domain.OwnshipControlModel
import net.jonh.aerobag.prototype.domain.OwnshipMode
import net.jonh.aerobag.prototype.domain.OwnshipRenderState
import net.jonh.aerobag.prototype.domain.OwnshipSelection
import net.jonh.aerobag.prototype.domain.PackageZipStore
import net.jonh.aerobag.prototype.domain.PlaybackStatus
import net.jonh.aerobag.prototype.domain.PlaybackUiState
import net.jonh.aerobag.prototype.domain.ProcedureKind
import net.jonh.aerobag.prototype.domain.ProcedureLoadOption
import net.jonh.aerobag.prototype.domain.ProcedureOptions
import net.jonh.aerobag.prototype.domain.ProcedureSummary
import net.jonh.aerobag.prototype.domain.ResolvedLeg
import net.jonh.aerobag.prototype.domain.ResolvedLegSource
import net.jonh.aerobag.prototype.domain.RenderTile
import net.jonh.aerobag.prototype.domain.RouteSegmentStatus
import net.jonh.aerobag.prototype.domain.RouteComponentViewKind
import net.jonh.aerobag.prototype.domain.RouteComponent
import net.jonh.aerobag.prototype.domain.ScreenPoint
import net.jonh.aerobag.prototype.domain.SectionalPackages
import net.jonh.aerobag.prototype.domain.SequencingMode
import net.jonh.aerobag.prototype.domain.SituationControlInput
import net.jonh.aerobag.prototype.domain.SituationRingCandidate
import net.jonh.aerobag.prototype.domain.TileStorageKind
import net.jonh.aerobag.prototype.domain.UiDebugState
import net.jonh.aerobag.prototype.domain.UiMapLayerToggleState
import net.jonh.aerobag.prototype.domain.UiTheme
import net.jonh.aerobag.prototype.domain.UiThemeLoader
import net.jonh.aerobag.prototype.domain.UiSessionSnapshot
import net.jonh.aerobag.prototype.domain.VisibleMapFeature
import net.jonh.aerobag.prototype.domain.VisibleMetarFeature
import net.jonh.aerobag.prototype.domain.VisiblePirepFeature
import net.jonh.aerobag.prototype.domain.applyPinchGesture
import net.jonh.aerobag.prototype.domain.clampZoom
import net.jonh.aerobag.prototype.domain.createInitialImageViewport
import net.jonh.aerobag.prototype.domain.createPinchSnapshot
import net.jonh.aerobag.prototype.domain.dragImageViewport
import net.jonh.aerobag.prototype.domain.dragViewport
import net.jonh.aerobag.prototype.domain.imageDisplaySize
import net.jonh.aerobag.prototype.domain.latLonToWorld
import net.jonh.aerobag.prototype.domain.preserveViewportForMap
import net.jonh.aerobag.prototype.domain.renderTileKey
import net.jonh.aerobag.prototype.domain.scaleForZoom
import net.jonh.aerobag.prototype.domain.screenToWorld
import net.jonh.aerobag.prototype.domain.viewportCenterLatLon
import net.jonh.aerobag.prototype.domain.worldToLatLon
import net.jonh.aerobag.prototype.domain.zoomAroundPoint
import net.jonh.aerobag.prototype.domain.zoomImageAroundPoint
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.jsonPrimitive
import net.jonh.aerobag.prototype.generated.airportCircleMarkerPath
import net.jonh.aerobag.prototype.generated.airportFuelMarkerPath
import net.jonh.aerobag.prototype.generated.airportOpenMarkerSymbol
import net.jonh.aerobag.prototype.generated.fixTrianglePath
import net.jonh.aerobag.prototype.generated.heliportHPath
import net.jonh.aerobag.prototype.generated.mapSelectionSpotSymbol
import net.jonh.aerobag.prototype.generated.metarBknSymbol
import net.jonh.aerobag.prototype.generated.metarClearSymbol
import net.jonh.aerobag.prototype.generated.metarFewSymbol
import net.jonh.aerobag.prototype.generated.metarMissingSymbol
import net.jonh.aerobag.prototype.generated.metarOvcSymbol
import net.jonh.aerobag.prototype.generated.metarSctSymbol
import net.jonh.aerobag.prototype.generated.NavSymbolLayer
import net.jonh.aerobag.prototype.generated.obstacleDotRadius
import net.jonh.aerobag.prototype.generated.obstacleShortDotY
import net.jonh.aerobag.prototype.generated.obstacleShortPath
import net.jonh.aerobag.prototype.generated.obstacleTallDotY
import net.jonh.aerobag.prototype.generated.obstacleTallPath
import net.jonh.aerobag.prototype.generated.pirepGenericSymbol
import net.jonh.aerobag.prototype.generated.pirepLightIcingSymbol
import net.jonh.aerobag.prototype.generated.pirepLightTurbulenceSymbol
import net.jonh.aerobag.prototype.generated.pirepModerateIcingSymbol
import net.jonh.aerobag.prototype.generated.pirepModerateTurbulenceSymbol
import net.jonh.aerobag.prototype.generated.pirepSevereIcingSymbol
import net.jonh.aerobag.prototype.generated.pirepSevereTurbulenceSymbol
import net.jonh.aerobag.prototype.generated.seaplaneAnchorPath
import net.jonh.aerobag.prototype.generated.vorBandPath
import net.jonh.aerobag.prototype.generated.vorOuterHexPath
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


@Composable
internal fun FlightPlanPage(
    appCore: NativeAppCoreAdapter,
    uiSession: NativeUiSession,
    page: AppPage,
    pageHistory: List<AppViewSnapshot>,
    mostRecentChartOrPlatePage: AppPage,
    uptimeLabel: String,
    navElement: NavElementUiView?,
    planUiState: FlightPlanUiState?,
    planListState: LazyListState,
    plan: FlightPlan,
    uiTheme: UiTheme,
    onSelectPage: (AppPage) -> Unit,
    onOpenRecentChartOrPlate: () -> Unit,
    onOpenCharts: (String?) -> Unit,
    onApplySessionSnapshot: (UiSessionSnapshot) -> Unit,
) {
    val density = LocalDensity.current
    val focusManager = LocalFocusManager.current
    val keyboardController = LocalSoftwareKeyboardController.current
    var selectedWaypointIndex by remember { mutableStateOf<Int?>(null) }
    var selectedWaypointTrayAnchor by remember { mutableStateOf<Dp?>(null) }
    var pendingSelectedRowKey by remember { mutableStateOf<String?>(null) }
    var reorderOpen by remember { mutableStateOf(false) }
    var airwayPicker by remember { mutableStateOf<AndroidAirwayPickerState?>(null) }
    var procedurePicker by remember { mutableStateOf<AndroidProcedurePickerState?>(null) }
    var airportInsert by remember { mutableStateOf<AndroidAirportInsertState?>(null) }
    var routeEntryText by remember { mutableStateOf("") }
    var routeEntryPreview by remember { mutableStateOf(emptyFlightPlanEntryPreview()) }
    var routeEntryLoading by remember { mutableStateOf(false) }
    var routeEntryError by remember { mutableStateOf<String?>(null) }
    var routeEntrySubmitting by remember { mutableStateOf(false) }
    var routeEntryFocused by remember { mutableStateOf(false) }
    var routeEntrySuppressNavigationUntilMs by remember { mutableLongStateOf(0L) }
    var trayOpenedAtMs by remember { mutableStateOf(0L) }
    val projectedPlanUiState = requireNotNull(planUiState) { "FlightPlanPage requires core-projected FlightPlanUiState" }
    val guidance = projectedPlanUiState.guidance
    val rows = remember(projectedPlanUiState.displayRows) {
        buildFlightPlanDisplayRows(projectedPlanUiState)
    }
    val blocks = remember(rows) {
        buildFlightPlanDisplayBlocks(rows)
    }
    val configuration = LocalConfiguration.current
    val narrowPortraitWaypointTray =
        configuration.screenWidthDp <= 720 && configuration.screenHeightDp > configuration.screenWidthDp
    val waypointActionButtonWidth =
        if (narrowPortraitWaypointTray) {
            ThumbSize * 1.5f
        } else {
            ThumbSize * 2f
        }
    val planWaypointTrayStart =
        ThumbGap + PlanArrowLane +
            if (narrowPortraitWaypointTray) {
                ThumbSize * 2.5f
            } else {
                ThumbSize * 3.15f
            } + PlanGridGap
    val imeBottomPadding = with(density) { WindowInsets.ime.getBottom(this).toDp() }
    val fallbackKeyboardPadding = (configuration.screenHeightDp * 0.38f).dp
    val keyboardAvoidancePadding =
        if (imeBottomPadding > 0.dp) {
            imeBottomPadding
        } else {
            fallbackKeyboardPadding
        }
    val planListBottomPadding =
        if (routeEntryFocused) {
            keyboardAvoidancePadding + ThumbSize + ThumbGap * 2f
        } else {
            0.dp
        }
    var structuredSurfaceBounds by remember { mutableStateOf<Rect?>(null) }
    val structuredRowBounds = remember { mutableStateMapOf<String, Rect>() }
    val selectedRow = selectedWaypointIndex?.let(rows::getOrNull)
    val selectedRowBounds = selectedRow?.let { structuredRowBounds[it.id] }
    val waypointTrayStart = planWaypointTrayStart
    val selectedRowActionMatrix = selectedRow?.actionMatrix.orEmpty()
    fun estimateTrayHeightDp(rowCount: Int): Dp =
        ThumbGap * 2 + (ThumbSize + 3.dp) * rowCount
    val waypointTrayTop =
        run {
            val defaultTop = ThumbSize + ThumbGap * 1.25f
            val anchoredTop = selectedWaypointTrayAnchor
            if (anchoredTop != null) {
                val paneTop = defaultTop + ThumbSize * 0.1f
                val estimatedRows =
                    when {
                        airportInsert != null -> 5
                        procedurePicker != null -> {
                            val picker = procedurePicker!!
                            when {
                                picker.loading || picker.error != null -> 2
                                picker.selectedProcedureId == null -> 1 + picker.procedures.size
                                else -> 1 + (picker.options?.validChoices?.size ?: 0)
                            }
                        }
                        airwayPicker != null -> {
                            val picker = airwayPicker!!
                            when {
                                picker.loading || picker.error != null -> 2
                                picker.selectedAirwayName == null -> 1 + picker.suggestions.size
                                picker.selectedEntryIndex == null -> 1 + (picker.presentation?.points?.size ?: 0)
                                else -> 1 + (picker.presentation?.points?.size ?: 0)
                            }
                        }
                        else -> selectedRowActionMatrix.size
                    }.coerceAtLeast(1)
                val estimatedHeight = estimateTrayHeightDp(estimatedRows)
                val paneBottom = with(density) { (structuredSurfaceBounds?.height ?: 0f).toDp() } + defaultTop
                val maxTop = (paneBottom - estimatedHeight - ThumbSize * 0.1f).coerceAtLeast(paneTop)
                return@run anchoredTop.coerceIn(paneTop, maxTop)
            }
            val surfaceBounds = structuredSurfaceBounds
            val rowBounds = selectedRowBounds
            if (surfaceBounds == null || rowBounds == null) {
                defaultTop
            } else {
                val desiredTop = with(density) { (rowBounds.top - surfaceBounds.top).toDp() } + defaultTop
                val paneTop = defaultTop + ThumbSize * 0.1f
                val estimatedRows =
                    when {
                        airportInsert != null -> 5
                        procedurePicker != null -> {
                            val picker = procedurePicker!!
                            when {
                                picker.loading || picker.error != null -> 2
                                picker.selectedProcedureId == null -> 1 + picker.procedures.size
                                else -> 1 + (picker.options?.validChoices?.size ?: 0)
                            }
                        }
                        airwayPicker != null -> {
                            val picker = airwayPicker!!
                            when {
                                picker.loading || picker.error != null -> 2
                                picker.selectedAirwayName == null -> 1 + picker.suggestions.size
                                picker.selectedEntryIndex == null -> 1 + (picker.presentation?.points?.size ?: 0)
                                else -> 1 + (picker.presentation?.points?.size ?: 0)
                            }
                        }
                        else -> selectedRowActionMatrix.size
                    }.coerceAtLeast(1)
                val estimatedHeight = estimateTrayHeightDp(estimatedRows)
                val paneBottom = with(density) { (surfaceBounds.bottom - surfaceBounds.top).toDp() } + defaultTop
                val maxTop = (paneBottom - estimatedHeight - ThumbSize * 0.1f).coerceAtLeast(paneTop)
                desiredTop.coerceIn(paneTop, maxTop)
            }
        }
    val waypointActionGap = 3.dp
    val waypointTrayWidth = waypointActionButtonWidth * 2f + waypointActionGap + 6.dp
    val structuredArrow =
        remember(rows, guidance?.activeFromRowUid, guidance?.activeToRowUid, structuredSurfaceBounds, structuredRowBounds.toMap(), density) {
            val surfaceBounds = structuredSurfaceBounds ?: return@remember null
            val guidanceView = guidance ?: return@remember null
            val visibleIndices =
                rows.mapIndexedNotNull { index, row ->
                    if (structuredRowBounds.containsKey(row.id)) index else null
                }
            val firstVisibleIndex = visibleIndices.minOrNull()
            val lastVisibleIndex = visibleIndices.maxOrNull()
            val toRowUid = guidanceView.activeToRowUid ?: return@remember null
            val toIndex = rows.indexOfFirst { row -> row.id == toRowUid }
            if (toIndex < 0) {
                return@remember null
            }
            val fromIndex =
                guidanceView.activeFromRowUid
                    ?.let { uid -> rows.indexOfFirst { row -> row.id == uid } }
                    ?: -1
            val lanePx = with(density) { PlanArrowLane.toPx() }
            val headLength = with(density) { 12.dp.toPx() }
            val textInsetPx = with(density) { PlanArrowButtonInset.toPx() }
            val surfaceHeight = surfaceBounds.height
            val elbowX = lanePx * 0.25f
            fun rowPoint(index: Int, preferBelow: Boolean): StructuredArrowEndpoint? {
                val row = rows[index]
                val indentPx = with(density) { (row.depth * 18).dp.toPx() }
                val x = lanePx + indentPx + textInsetPx
                val bounds = structuredRowBounds[row.id]
                if (bounds != null) {
                    val centerY = bounds.top - surfaceBounds.top + bounds.height / 2f
                    val clippedAbove = bounds.bottom < surfaceBounds.top
                    val clippedBelow = bounds.top > surfaceBounds.bottom
                    val clampedY =
                        when {
                            clippedAbove -> if (preferBelow) surfaceHeight else 0f
                            clippedBelow -> if (preferBelow) surfaceHeight else 0f
                            else -> centerY.coerceIn(0f, surfaceHeight)
                        }
                    return StructuredArrowEndpoint(
                        point =
                            Offset(
                                x = x,
                                y = clampedY,
                            ),
                        clipped = clampedY != centerY,
                        clippedAbove = clippedAbove || (clampedY == 0f && centerY < 0f),
                        clippedBelow = clippedBelow || (clampedY == surfaceHeight && centerY > surfaceHeight),
                    )
                }
                return when {
                    firstVisibleIndex != null && index < firstVisibleIndex ->
                        StructuredArrowEndpoint(
                            point = Offset(x = x, y = 0f),
                            clipped = true,
                            clippedAbove = true,
                            clippedBelow = false,
                        )
                    lastVisibleIndex != null && index > lastVisibleIndex ->
                        StructuredArrowEndpoint(
                            point = Offset(x = x, y = surfaceHeight),
                            clipped = true,
                            clippedAbove = false,
                            clippedBelow = true,
                        )
                    else -> null
                }
            }
            val toEndpoint = rowPoint(toIndex, preferBelow = fromIndex >= 0 && toIndex > fromIndex) ?: return@remember null
            if (toEndpoint.clippedAbove) {
                return@remember null
            }
            val fromEndpoint = if (fromIndex >= 0) rowPoint(fromIndex, preferBelow = false) else null
            if (fromEndpoint?.clipped == true && toEndpoint.clipped) {
                return@remember null
            }
            val fromPoint = fromEndpoint?.point ?: Offset(x = elbowX, y = toEndpoint.point.y)
            val toPoint = toEndpoint.point
            StructuredArrowSpec(
                fromPoint = fromPoint,
                toPoint = toPoint,
                toClipped = toEndpoint.clipped,
                fromClippedAbove = fromEndpoint?.clippedAbove ?: false,
                elbowX = elbowX,
                shaftEndX = maxOf(elbowX, toPoint.x - headLength + with(density) { 1.5.dp.toPx() }),
                headLength = headLength,
            )
        }

    fun closePanels() {
        selectedWaypointIndex = null
        selectedWaypointTrayAnchor = null
        pendingSelectedRowKey = null
        airwayPicker = null
        procedurePicker = null
        airportInsert = null
    }

    fun submitRouteEntry() {
        val input = routeEntryText.trim()
        if (input.isEmpty() || !routeEntryPreview.canCommit || routeEntrySubmitting) {
            return
        }
        routeEntrySuppressNavigationUntilMs = SystemClock.elapsedRealtime() + 800L
        routeEntrySubmitting = true
        routeEntryError = null
        runCatching {
            val mutation = appCore.appendFlightPlanEntry(plan, input)
            uiSession.replaceFlightPlan(mutation.plan)
        }.onSuccess { snapshot ->
            onApplySessionSnapshot(snapshot)
            routeEntryText = ""
            routeEntryPreview = emptyFlightPlanEntryPreview()
            keyboardController?.hide()
            focusManager.clearFocus(force = true)
        }.onFailure { error ->
            routeEntryError = error.message ?: error.toString()
        }
        routeEntrySubmitting = false
    }

    fun routeEntryNavigationSuppressed(): Boolean =
        routeEntryFocused || SystemClock.elapsedRealtime() < routeEntrySuppressNavigationUntilMs

    LaunchedEffect(plan, routeEntryText) {
        val input = routeEntryText.trim()
        if (input.isEmpty()) {
            routeEntryLoading = false
            routeEntryPreview = emptyFlightPlanEntryPreview()
            return@LaunchedEffect
        }
        routeEntryLoading = true
        runCatching {
            withContext(Dispatchers.IO) {
                appCore.previewFlightPlanEntry(plan, routeEntryText)
            }
        }.onSuccess { preview ->
            routeEntryPreview = preview
        }.onFailure { error ->
            routeEntryPreview = emptyFlightPlanEntryPreview()
            routeEntryError = error.message ?: error.toString()
        }
        routeEntryLoading = false
    }

    LaunchedEffect(airportInsert?.rowUid, airportInsert?.before, airportInsert?.airportId) {
        val editor = airportInsert ?: return@LaunchedEffect
        val prefix = editor.airportId.trim().uppercase()
        if (prefix.isEmpty()) {
            airportInsert = editor.copy(loading = false, suggestions = emptyList())
            return@LaunchedEffect
        }
        airportInsert = editor.copy(loading = true)
        runCatching {
            withContext(Dispatchers.IO) {
                uiSession.suggestWaypointIdentifiersAtFlightPlanRow(editor.rowUid, editor.before, prefix, 8)
            }
        }.onSuccess { suggestions ->
            airportInsert = airportInsert?.copy(loading = false, suggestions = suggestions)
        }.onFailure { error ->
            airportInsert = airportInsert?.copy(loading = false, suggestions = emptyList(), error = error.message ?: error.toString())
        }
    }

    LaunchedEffect(rows, pendingSelectedRowKey) {
        val selectionKey = pendingSelectedRowKey ?: return@LaunchedEffect
        Log.d("AerobagReorder", "resolveSelection pendingKey=$selectionKey rows=${rows.joinToString(" | ") { "${it.selectionKey}:${it.rowKind}:${it.label}" }}")
        val nextIndex =
            rows.indexOfFirst { row ->
                row.selectionKey == selectionKey
            }
        if (nextIndex >= 0) {
            selectedWaypointIndex = nextIndex
        } else {
            selectedWaypointIndex = null
        }
        pendingSelectedRowKey = null
    }

    LaunchedEffect(routeEntryFocused, keyboardAvoidancePadding, blocks.size) {
        if (routeEntryFocused) {
            delay(250)
            planListState.animateScrollToItem(blocks.size)
        }
    }

    Box(
        modifier = Modifier
            .fillMaxSize()
            .background(uiTheme.controls.chartSurfaceBg),
    ) {
        HomeReturnDock(
            modifier = Modifier.align(Alignment.TopStart),
            currentPage = page,
            chartPlateTargetPage = mostRecentChartOrPlatePage,
            onHomeClick = {
                if (!routeEntryNavigationSuppressed()) {
                    onSelectPage(AppPage.Home)
                }
            },
            onOpenChartOrPlate = {
                if (!routeEntryNavigationSuppressed()) {
                    onOpenRecentChartOrPlate()
                }
            },
        )

        Column(
            modifier = Modifier
                .fillMaxSize()
                .padding(top = ThumbSize + ThumbGap * 2, start = ThumbGap, end = ThumbGap, bottom = ThumbSize * 2.15f),
            verticalArrangement = Arrangement.spacedBy(PlanGridGap),
        ) {
            Box(
                modifier = Modifier
                    .weight(1f)
                    .fillMaxWidth()
                    .onGloballyPositioned { coordinates ->
                        structuredSurfaceBounds = coordinates.boundsInWindow()
                    },
            ) {
                Column(
                    modifier = Modifier
                        .fillMaxSize()
                        .padding(start = PlanArrowLane),
                    verticalArrangement = Arrangement.spacedBy(PlanGridGap),
                ) {
                    PlanHeaderRow()
                    LazyColumn(
                        state = planListState,
                        modifier = Modifier
                            .weight(1f)
                            .testTag("parity:plan-list"),
                        contentPadding = PaddingValues(bottom = planListBottomPadding),
                        verticalArrangement = Arrangement.spacedBy(PlanGridGap),
                    ) {
                        items(blocks.size) { blockIndex ->
                            when (val block = blocks[blockIndex]) {
                                is FlightPlanDisplayBlock.Single -> {
                                    FlightPlanDataRow(
                                        row = block.row,
                                        selected = selectedWaypointIndex == block.index,
                                        structuredRowBounds = structuredRowBounds,
                                        onWaypointClick = {
                                            trayOpenedAtMs = SystemClock.elapsedRealtime()
                                            selectedWaypointTrayAnchor =
                                                structuredSurfaceBounds?.let { surface ->
                                                    structuredRowBounds[block.row.id]?.top?.let { top ->
                                                        with(density) { (top - surface.top).toDp() } + (ThumbSize + ThumbGap * 1.25f)
                                                    }
                                                }
                                            selectedWaypointIndex = block.index
                                            pendingSelectedRowKey = block.row.selectionKey
                                            airwayPicker = null
                                            procedurePicker = null
                                            airportInsert = null
                                        },
                                    )
                                }

                                is FlightPlanDisplayBlock.Group -> {
                                    FlightPlanGroupBlock(
                                        header = block.header,
                                        headerSelected = selectedWaypointIndex == block.headerIndex,
                                        structuredRowBounds = structuredRowBounds,
                                        onHeaderClick = {
                                            trayOpenedAtMs = SystemClock.elapsedRealtime()
                                            selectedWaypointTrayAnchor =
                                                structuredSurfaceBounds?.let { surface ->
                                                    structuredRowBounds[block.header.id]?.top?.let { top ->
                                                        with(density) { (top - surface.top).toDp() } + (ThumbSize + ThumbGap * 1.25f)
                                                    }
                                                }
                                            selectedWaypointIndex = block.headerIndex
                                            pendingSelectedRowKey = block.header.selectionKey
                                            airwayPicker = null
                                            procedurePicker = null
                                            airportInsert = null
                                        },
                                        children = block.children,
                                        selectedWaypointIndex = selectedWaypointIndex,
                                        onChildClick = { childIndex ->
                                            trayOpenedAtMs = SystemClock.elapsedRealtime()
                                            selectedWaypointTrayAnchor =
                                                structuredSurfaceBounds?.let { surface ->
                                                    block.children.firstOrNull { it.first == childIndex }?.second?.id?.let { rowId ->
                                                        structuredRowBounds[rowId]?.top?.let { top ->
                                                            with(density) { (top - surface.top).toDp() } + (ThumbSize + ThumbGap * 1.25f)
                                                        }
                                                    }
                                                }
                                            selectedWaypointIndex = childIndex
                                            pendingSelectedRowKey = block.children.firstOrNull { it.first == childIndex }?.second?.selectionKey
                                            airwayPicker = null
                                            procedurePicker = null
                                            airportInsert = null
                                        },
                                    )
                                }
                            }
                        }
                        item {
                            FlightPlanRouteEntryRow(
                                text = routeEntryText,
                                preview = routeEntryPreview,
                                loading = routeEntryLoading,
                                error = routeEntryError,
                                submitting = routeEntrySubmitting,
                                onTextChange = { value ->
                                    routeEntryText = value.uppercase()
                                    routeEntryError = null
                                },
                                onFocusChange = { focused -> routeEntryFocused = focused },
                                onSubmit = { submitRouteEntry() },
                            )
                        }
                    }
                }

                if (structuredArrow != null) {
                    Canvas(
                        modifier =
                            Modifier
                                .matchParentSize()
                                .zIndex(2f),
                    ) {
                        val path =
                            Path().apply {
                                moveTo(
                                    if (structuredArrow.fromClippedAbove) structuredArrow.elbowX else structuredArrow.fromPoint.x,
                                    structuredArrow.fromPoint.y,
                                )
                                if (!structuredArrow.fromClippedAbove) {
                                    lineTo(structuredArrow.elbowX, structuredArrow.fromPoint.y)
                                }
                                lineTo(structuredArrow.elbowX, structuredArrow.toPoint.y)
                                if (!structuredArrow.toClipped) {
                                    lineTo(structuredArrow.shaftEndX, structuredArrow.toPoint.y)
                                }
                            }
                        drawPath(
                            path = path,
                            color = Color(0xFFD45A7A),
                            style =
                                Stroke(
                                    width = with(density) { 3.dp.toPx() },
                                    cap = StrokeCap.Round,
                                ),
                        )
                        if (!structuredArrow.toClipped) {
                            val head =
                                Path().apply {
                                    moveTo(structuredArrow.toPoint.x, structuredArrow.toPoint.y)
                                    lineTo(
                                        structuredArrow.toPoint.x - structuredArrow.headLength,
                                        structuredArrow.toPoint.y - structuredArrow.headLength * 0.55f,
                                    )
                                    lineTo(
                                        structuredArrow.toPoint.x - structuredArrow.headLength,
                                        structuredArrow.toPoint.y + structuredArrow.headLength * 0.55f,
                                    )
                                    close()
                                }
                            drawPath(head, color = Color(0xFFD45A7A))
                        }
                    }
                }
            }
        }

        Box(
            modifier = Modifier
                .align(Alignment.BottomCenter)
                .fillMaxWidth()
                .height(ThumbSize * 2.05f)
                .padding(start = ThumbGap, end = ThumbGap, bottom = ThumbGap),
        ) {
            Row(
                modifier = Modifier.align(Alignment.TopCenter),
                horizontalArrangement = Arrangement.spacedBy(ThumbGap),
            ) {
                CompactSquareButton(
                    label = "Next Leg",
                    modifier = Modifier.width(ThumbSize * 1.8f).height(ThumbSize),
                    enabled = guidance?.canActivateNextLeg == true,
                    onClick = { onApplySessionSnapshot(uiSession.activateNextLeg()) },
                )
                CompactSquareButton(
                    label = "Sequence",
                    modifier = Modifier.width(ThumbSize * 1.8f).height(ThumbSize),
                    enabled = guidance?.canSequenceActiveLeg == true,
                    onClick = { onApplySessionSnapshot(uiSession.sequenceActiveLeg()) },
                )
                CompactSquareButton(
                    label = "Suspend",
                    modifier = Modifier.width(ThumbSize * 1.8f).height(ThumbSize),
                    enabled = guidance?.canSuspend == true,
                    onClick = { onApplySessionSnapshot(uiSession.suspendSequencing()) },
                )
                CompactSquareButton(
                    label = "Unsusp",
                    modifier = Modifier.width(ThumbSize * 1.8f).height(ThumbSize),
                    enabled = guidance?.canUnsuspend == true,
                    onClick = { onApplySessionSnapshot(uiSession.unsuspendSequencing()) },
                )
            }
            NavElementDock(
                navElement = navElement,
                onClick = {
                    if (!routeEntryNavigationSuppressed()) {
                        onOpenRecentChartOrPlate()
                    }
                },
                modifier = Modifier.align(Alignment.BottomCenter),
            )
        }

        if (selectedWaypointIndex != null && selectedRow != null) {
            Scrim {
                if (SystemClock.elapsedRealtime() - trayOpenedAtMs >= 150L) {
                    closePanels()
                }
            }
            if (airportInsert != null) {
                val editor = airportInsert!!
                BoxWithConstraints(
                    modifier =
                        Modifier
                            .fillMaxSize()
                            .zIndex(5f),
                ) {
                    AirportInsertPanel(
                        state = editor,
                        modifier =
                            Modifier
                                .align(Alignment.TopCenter)
                                .fillMaxWidth()
                                .padding(start = ThumbGap, top = ThumbSize, end = ThumbGap)
                                .heightIn(min = ThumbSize * 1.45f),
                        onTextChange = { value ->
                            airportInsert =
                                editor.copy(
                                    airportId = value.uppercase().filter { it in 'A'..'Z' || it in '0'..'9' }.take(8),
                                    error = null,
                                )
                        },
                        onSubmit = {
                            val airportId = editor.airportId.trim().uppercase()
                            if (airportId.isEmpty()) {
                                airportInsert = editor.copy(error = "Enter airport id")
                                return@AirportInsertPanel
                            }
                            runCatching {
                                val waypoint = appCore.resolveNavRefIdentifier(airportId)
                                uiSession.insertWaypointAtFlightPlanRow(editor.rowUid, editor.before, waypoint)
                            }.onSuccess { snapshot ->
                                onApplySessionSnapshot(snapshot)
                                closePanels()
                            }.onFailure { error ->
                                airportInsert = editor.copy(error = error.message ?: error.toString())
                            }
                        },
                        onSuggestionClick = { suggestion ->
                            runCatching {
                                uiSession.insertWaypointAtFlightPlanRow(editor.rowUid, editor.before, suggestion.navRef)
                            }.onSuccess { snapshot ->
                                onApplySessionSnapshot(snapshot)
                                closePanels()
                            }.onFailure { error ->
                                airportInsert = editor.copy(error = error.message ?: error.toString())
                            }
                        },
                    )
                }
            } else if (procedurePicker != null) {
                val picker = procedurePicker!!
                MenuPanel(
                    modifier = Modifier
                        .align(Alignment.TopStart)
                        .padding(top = waypointTrayTop, start = waypointTrayStart, end = ThumbGap)
                        .zIndex(5f),
                    width = waypointTrayWidth,
                ) {
                    MenuPanelRow(label = "APPROACH ${picker.airportId}", active = false, enabled = false, onSelect = {})
                    if (picker.error != null) {
                        MenuPanelRow(label = picker.error, active = false, enabled = false, onSelect = {})
                    }
                    if (picker.loading) {
                        MenuPanelRow(label = "Loading…", active = false, enabled = false, onSelect = {})
                    } else if (picker.selectedProcedureId == null) {
                        picker.procedures.forEach { procedure ->
                            MenuPanelRow(
                                label = procedure.procedureId,
                                active = false,
                                enabled = true,
                                onSelect = {
                                    procedurePicker = picker.copy(loading = true, error = null)
                                    runCatching {
                                        appCore.describeProcedureOptions(
                                            picker.airportId,
                                            procedure.procedureId,
                                            ProcedureKind.Approach,
                                        )
                                    }.onSuccess { options ->
                                        procedurePicker =
                                            picker.copy(
                                                loading = false,
                                                selectedProcedureId = procedure.procedureId,
                                                options = options,
                                            )
                                    }.onFailure { error ->
                                        Log.e("AerobagProcedure", "describeProcedureOptions failed airport=${picker.airportId} procedure=${procedure.procedureId}", error)
                                        procedurePicker = picker.copy(loading = false, error = error.message ?: error.toString())
                                    }
                                },
                            )
                        }
                    } else {
                        picker.options?.validChoices?.forEach { choice ->
                            MenuPanelRow(
                                label = choice.enrouteTransition ?: "No Transition",
                                active = false,
                                enabled = true,
                                onSelect = {
                                    procedurePicker = picker.copy(loading = true, error = null)
                                    runCatching {
                                        uiSession.selectProcedureAtFlightPlanRow(
                                            picker.rowUid,
                                            picker.airportId,
                                            picker.selectedProcedureId,
                                            ProcedureKind.Approach,
                                            null,
                                            choice.enrouteTransition,
                                        )
                                    }.onSuccess { snapshot ->
                                        onApplySessionSnapshot(snapshot)
                                        closePanels()
                                    }.onFailure { error ->
                                        Log.e(
                                            "AerobagProcedure",
                                            "select procedure failed row=${picker.rowUid} airport=${picker.airportId} procedure=${picker.selectedProcedureId} enroute=${choice.enrouteTransition}",
                                            error,
                                        )
                                        procedurePicker = picker.copy(loading = false, error = error.message ?: error.toString())
                                    }
                                },
                            )
                        }
                        MenuPanelRow(
                            label = "Back",
                            active = false,
                            enabled = true,
                            onSelect = { procedurePicker = picker.copy(selectedProcedureId = null, options = null) },
                        )
                    }
                }
            } else if (airwayPicker != null) {
                val picker = airwayPicker!!
                MenuPanel(
                    modifier = Modifier
                        .align(Alignment.TopStart)
                        .padding(top = waypointTrayTop, start = waypointTrayStart, end = ThumbGap)
                        .zIndex(5f),
                    width = waypointTrayWidth,
                ) {
                    MenuPanelRow(
                        label = buildString {
                            append("AIRWAY ")
                            append(navRefLabel(picker.originAnchor))
                            picker.destinationAnchor?.let {
                                append(" -> ")
                                append(navRefLabel(it))
                            }
                        },
                        active = false,
                        enabled = false,
                        onSelect = {},
                    )
                    if (picker.error != null) {
                        MenuPanelRow(label = picker.error, active = false, enabled = false, onSelect = {})
                    }
                    if (picker.loading) {
                        MenuPanelRow(label = "Loading…", active = false, enabled = false, onSelect = {})
                    } else if (picker.selectedAirwayName == null) {
                        picker.suggestions.forEach { suggestion ->
                            MenuPanelRow(
                                label = suggestion.airwayName,
                                active = false,
                                enabled = true,
                                onSelect = {
                                    airwayPicker = picker.copy(loading = true, error = null)
                                    runCatching {
                                        appCore.prepareAirwayPresentationForAnchors(
                                            suggestion.airwayName,
                                            picker.originAnchor,
                                            picker.destinationAnchor,
                                        )
                                    }.onSuccess { presentation ->
                                        airwayPicker =
                                            picker.copy(
                                                loading = false,
                                                selectedAirwayName = suggestion.airwayName,
                                                presentation = presentation,
                                                selectedEntryIndex = null,
                                            )
                                    }.onFailure { error ->
                                        airwayPicker = picker.copy(loading = false, error = error.message ?: error.toString())
                                    }
                                },
                            )
                        }
                    } else if (picker.selectedEntryIndex == null) {
                        picker.presentation?.points?.forEachIndexed { index, point ->
                            MenuPanelRow(
                                label = navRefLabel(point.navRef),
                                active = index == picker.presentation.suggestedEntryIndex,
                                enabled = true,
                                onSelect = {
                                    airwayPicker = picker.copy(selectedEntryIndex = index)
                                },
                            )
                        }
                        MenuPanelRow(label = "Back", active = false, enabled = true, onSelect = { airwayPicker = picker.copy(selectedAirwayName = null, presentation = null) })
                    } else {
                        val presentation = picker.presentation
                        presentation?.points?.forEachIndexed { exitIndex, point ->
                            val isEntry = exitIndex == picker.selectedEntryIndex
                            MenuPanelRow(
                                label = navRefLabel(point.navRef),
                                active = exitIndex == presentation.suggestedExitIndex,
                                enabled = !isEntry,
                                onSelect = {
                                    if (isEntry) return@MenuPanelRow
                                    airwayPicker = picker.copy(loading = true, error = null)
                                    runCatching {
                                        uiSession.insertAirwayAtFlightPlanRow(
                                            picker.rowUid,
                                            presentation,
                                            picker.selectedEntryIndex,
                                            exitIndex,
                                        )
                                    }.onSuccess { snapshot ->
                                        onApplySessionSnapshot(snapshot)
                                        closePanels()
                                    }.onFailure { error ->
                                        airwayPicker = picker.copy(error = error.message ?: error.toString())
                                    }
                                },
                            )
                        }
                        MenuPanelRow(label = "Back", active = false, enabled = true, onSelect = { airwayPicker = picker.copy(selectedEntryIndex = null) })
                    }
                }
            } else {
                MenuPanel(
                    modifier = Modifier
                        .align(Alignment.TopStart)
                        .padding(top = waypointTrayTop, start = waypointTrayStart, end = ThumbGap)
                        .zIndex(5f),
                    width = waypointTrayWidth,
                ) {
                    selectedRowActionMatrix.forEach { actionRow ->
                        Row(
                            modifier = Modifier.fillMaxWidth(),
                            horizontalArrangement = Arrangement.spacedBy(waypointActionGap),
                        ) {
                            actionRow.forEach { action ->
                                MenuPanelRow(
                                    label = action.label,
                                    active = false,
                                    enabled = action.enabled,
                                    testTag = "parity:plan-row-action:${action.id}",
                                    width = waypointActionButtonWidth,
                                    onSelect = {
                                        if (!action.enabled) {
                                            return@MenuPanelRow
                                        }
                                        if (action.execution == "core_session") {
                                            runCatching {
                                                uiSession.performFlightPlanRowAction(selectedRow.id, action.uid)
                                            }.onSuccess { snapshot ->
                                                onApplySessionSnapshot(snapshot)
                                            }.onFailure { error ->
                                                Log.e(
                                                    "AerobagPlan",
                                                    "core row action failed row=${selectedRow.id} action=${action.uid}",
                                                    error,
                                                )
                                            }
                                            if (action.dismissTrayOnSuccess) {
                                                closePanels()
                                            }
                                            return@MenuPanelRow
                                        }
                                        when (action.id) {
                                            "insert_before",
                                            "insert_after",
                                            -> {
                                                airportInsert =
                                                    AndroidAirportInsertState(
                                                        rowUid = selectedRow.id,
                                                        before = action.id == "insert_before",
                                                        airportId = "",
                                                        error = null,
                                                        loading = false,
                                                        suggestions = emptyList(),
                                                    )
                                            }
                                            "add_airway" -> {
                                                airwayPicker =
                                                    AndroidAirwayPickerState(
                                                        loading = true,
                                                        error = null,
                                                        rowUid = selectedRow.id,
                                                        originAnchor = selectedRow.originAnchor!!,
                                                        destinationAnchor = selectedRow.destinationAnchor,
                                                        suggestions = emptyList(),
                                                        selectedAirwayName = null,
                                                        presentation = null,
                                                        selectedEntryIndex = null,
                                                    )
                                                runCatching {
                                                    appCore.suggestAirwaysNear(selectedRow.originAnchor!!)
                                                }.onSuccess { suggestions ->
                                                    airwayPicker = airwayPicker?.copy(loading = false, suggestions = suggestions)
                                                }.onFailure { error ->
                                                    airwayPicker = airwayPicker?.copy(loading = false, error = error.message ?: error.toString())
                                                }
                                            }
                                            "select_procedure" -> {
                                                val airportId = selectedRow.chartAirportId ?: return@MenuPanelRow
                                                procedurePicker =
                                                    AndroidProcedurePickerState(
                                                        loading = true,
                                                        error = null,
                                                        rowUid = selectedRow.id,
                                                        airportId = airportId,
                                                        procedures = emptyList(),
                                                        selectedProcedureId = null,
                                                        options = null,
                                                    )
                                                runCatching {
                                                    appCore.listProcedures(airportId, ProcedureKind.Approach)
                                                }.onSuccess { procedures ->
                                                    procedurePicker = procedurePicker?.copy(loading = false, procedures = procedures)
                                                }.onFailure { error ->
                                                    Log.e("AerobagProcedure", "listProcedures failed airport=$airportId", error)
                                                    procedurePicker = procedurePicker?.copy(loading = false, error = error.message ?: error.toString())
                                                }
                                            }
                                            "plates" -> {
                                                onOpenCharts(selectedRow.chartAirportId)
                                                closePanels()
                                            }
                                            "waypoint_info" -> {}
                                            else -> Unit
                                        }
                                    },
                                )
                            }
                        }
                    }
                }
        }
    }
}
}

internal fun emptyFlightPlanEntryPreview(): FlightPlanEntryPreview =
    FlightPlanEntryPreview(
        canCommit = false,
        tokens = emptyList(),
        issues = emptyList(),
    )

internal fun routeEntryVisualTransformation(
    preview: FlightPlanEntryPreview,
    neutralColor: Color,
    recognizedColor: Color,
    invalidColor: Color,
): VisualTransformation =
    VisualTransformation { text ->
        val annotated =
            buildAnnotatedString {
                append(text.text)
                preview.tokens.forEach { token ->
                    val start = token.start.coerceIn(0, text.text.length)
                    val end = token.end.coerceIn(start, text.text.length)
                    if (start == end) {
                        return@forEach
                    }
                    val color =
                        when (token.state) {
                            "recognized" -> recognizedColor
                            "invalid" -> invalidColor
                            else -> neutralColor
                        }
                    addStyle(SpanStyle(color = color), start, end)
                }
                preview.issues.forEach { issue ->
                    val start = issue.start.coerceIn(0, text.text.length)
                    val end = issue.end.coerceIn(start, text.text.length)
                    if (start != end) {
                        addStyle(
                            SpanStyle(color = invalidColor, textDecoration = TextDecoration.Underline),
                            start,
                            end,
                        )
                    }
                }
            }
        TransformedText(annotated, OffsetMapping.Identity)
    }

@Composable
internal fun FlightPlanRouteEntryRow(
    text: String,
    preview: FlightPlanEntryPreview,
    loading: Boolean,
    error: String?,
    submitting: Boolean,
    onTextChange: (String) -> Unit,
    onFocusChange: (Boolean) -> Unit,
    onSubmit: () -> Unit,
) {
    val uiTheme = LocalAerobagUiTheme.current
    val fieldShape = RoundedCornerShape(ThumbRadius * 0.82f)
    val neutralTextColor = uiTheme.controls.panelFg
    val recognizedTextColor = Color(0xFF12683C)
    val invalidTextColor = Color(0xFFC23A2C)
    val borderColor =
        when {
            error != null || preview.issues.isNotEmpty() -> invalidTextColor
            preview.canCommit -> recognizedTextColor
            else -> Color(0x554E626C)
        }
    val feedback =
        error
            ?: preview.issues.firstOrNull()?.message
            ?: if (loading) "Checking..." else null
    Column(
        modifier = Modifier.fillMaxWidth(),
        verticalArrangement = Arrangement.spacedBy(ThumbGap * 0.35f),
    ) {
        BasicTextField(
            value = text,
            onValueChange = onTextChange,
            singleLine = true,
            enabled = !submitting,
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
                MaterialTheme.typography.titleMedium.copy(
                    color = neutralTextColor,
                    fontWeight = FontWeight.ExtraBold,
                ),
            visualTransformation = routeEntryVisualTransformation(
                preview = preview,
                neutralColor = neutralTextColor,
                recognizedColor = recognizedTextColor,
                invalidColor = invalidTextColor,
            ),
            modifier =
                Modifier
                    .testTag("parity:plan-append-route-input")
                    .onFocusChanged { state -> onFocusChange(state.isFocused) }
                    .fillMaxWidth()
                    .height(ThumbSize)
                    .clip(fieldShape)
                    .background(Color.White.copy(alpha = 0.96f))
                    .border(1.5.dp, borderColor, fieldShape)
                    .onPreviewKeyEvent { event ->
                        if (event.nativeKeyEvent.keyCode != AndroidKeyEvent.KEYCODE_ENTER) {
                            return@onPreviewKeyEvent false
                        }
                        if (event.nativeKeyEvent.action == AndroidKeyEvent.ACTION_DOWN) {
                            onSubmit()
                        }
                        true
                    }
                    .padding(horizontal = ThumbGap, vertical = ThumbSize * 0.22f),
            decorationBox = { innerTextField ->
                Box(modifier = Modifier.fillMaxSize(), contentAlignment = Alignment.CenterStart) {
                    if (text.isBlank()) {
                        Text(
                            text = "Append route...",
                            style = MaterialTheme.typography.titleMedium.copy(fontWeight = FontWeight.ExtraBold),
                            color = Color(0x884E626C),
                        )
                    }
                    innerTextField()
                }
            },
        )
        if (feedback != null) {
            Text(
                text = feedback,
                modifier = Modifier.testTag("parity:plan-append-route-feedback"),
                style = MaterialTheme.typography.labelSmall,
                color = if (error != null || preview.issues.isNotEmpty()) invalidTextColor else uiTheme.controls.panelFg,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
        }
    }
}
