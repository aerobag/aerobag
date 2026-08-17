// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

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
import org.aerobag.app.domain.AirwayPresentationPlan
import org.aerobag.app.domain.AirwaySuggestion
import org.aerobag.app.domain.WaypointIdentifierSuggestion
import org.aerobag.app.domain.CoreMapViewport
import org.aerobag.app.domain.DerivedChartPageState
import org.aerobag.app.domain.FlightPlanControlId
import org.aerobag.app.domain.FlightPlanEntryPreview
import org.aerobag.app.domain.FlightPlanDisplayRowKind
import org.aerobag.app.domain.FlightPlanDisplayRowUiView
import org.aerobag.app.domain.FlightPlanRowActionUiView
import org.aerobag.app.domain.FlightPlanRowNavigationAction
import org.aerobag.app.domain.FlightPlanRouteSegment
import org.aerobag.app.domain.FlightPlanUiState
import org.aerobag.app.domain.InstalledPackages
import org.aerobag.app.domain.AirspaceDisplayDecoration
import org.aerobag.app.domain.AirspaceDisplayLabel
import org.aerobag.app.domain.AirspaceDisplayPath
import org.aerobag.app.domain.AirspaceDisplaySubpath
import org.aerobag.app.domain.AirspaceLimitGlyph
import org.aerobag.app.domain.AirspaceScreenPoint
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
import org.aerobag.app.domain.OwnshipSelection
import org.aerobag.app.domain.PackageZipStore
import org.aerobag.app.domain.PlaybackStatus
import org.aerobag.app.domain.PlaybackUiState
import org.aerobag.app.domain.ProcedureKind
import org.aerobag.app.domain.ProcedureLoadOption
import org.aerobag.app.domain.ProcedureOptions
import org.aerobag.app.domain.ProcedureSummary
import org.aerobag.app.domain.RenderTile
import org.aerobag.app.domain.RouteSegmentStatus
import org.aerobag.app.domain.RouteComponentViewKind
import org.aerobag.app.domain.ScreenPoint
import org.aerobag.app.domain.SectionalPackages
import org.aerobag.app.domain.SequencingMode
import org.aerobag.app.domain.SituationControlInput
import org.aerobag.app.domain.SituationRingCandidate
import org.aerobag.app.domain.TileStorageKind
import org.aerobag.app.domain.UiDebugState
import org.aerobag.app.domain.UiMapLayerToggleState
import org.aerobag.app.domain.UiTheme
import org.aerobag.app.domain.UiThemeLoader
import org.aerobag.app.domain.UiSessionSnapshot
import org.aerobag.app.domain.VisibleMapFeature
import org.aerobag.app.domain.VisibleMetarFeature
import org.aerobag.app.domain.VisiblePirepFeature
import org.aerobag.app.domain.WeatherDetailUiView
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
import org.aerobag.app.generated.hasActionSymbol
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
    uiTheme: UiTheme,
    overlayState: FlightPlanOverlayState,
    onOverlayAction: (FlightPlanOverlayAction) -> Unit,
    onSelectPage: (AppPage) -> Unit,
    onOpenRecentChartOrPlate: () -> Unit,
    onOpenCharts: (String, String?) -> Unit,
    onApplySessionSnapshot: (UiSessionSnapshot) -> Unit,
    onSessionCommandFailure: (Throwable) -> Unit,
) {
    val context = LocalContext.current
    val density = LocalDensity.current
    val focusManager = LocalFocusManager.current
    val keyboardController = LocalSoftwareKeyboardController.current
    var selectedWaypointTrayAnchor by remember { mutableStateOf<Dp?>(null) }
    var reorderOpen by remember { mutableStateOf(false) }
    var airwayPicker by remember { mutableStateOf<AndroidAirwayPickerState?>(null) }
    var procedurePicker by remember { mutableStateOf<AndroidProcedurePickerState?>(null) }
    var airportInsert by remember { mutableStateOf<AndroidAirportInsertState?>(null) }
    val airportInfoScope = rememberCoroutineScope()
    var routeEntryText by remember { mutableStateOf("") }
    var routeEntryPreview by remember { mutableStateOf(emptyFlightPlanEntryPreview()) }
    var routeEntryLoading by remember { mutableStateOf(false) }
    var routeEntryError by remember { mutableStateOf<String?>(null) }
    var routeEntrySubmitting by remember { mutableStateOf(false) }
    var routeEntryFocused by remember { mutableStateOf(false) }
    val planDataScrollState = rememberScrollState()
    val routeEntryPreviewController = remember { RouteEntryPreviewController() }
    var routeEntrySuppressNavigationUntilMs by remember { mutableLongStateOf(0L) }
    var trayOpenedAtMs by remember { mutableStateOf(0L) }
    fun applySessionCommand(commandName: String, operation: () -> UiSessionSnapshot): UiSessionSnapshot? =
        try {
            operation().also(onApplySessionSnapshot)
        } catch (error: CancellationException) {
            throw error
        } catch (error: Throwable) {
            Log.w("AerobagSessionCommand", "flight-plan command failed command=$commandName", error)
            onSessionCommandFailure(error)
            null
    }
    val projectedPlanUiState = requireNotNull(planUiState) { "FlightPlanPage requires core-projected FlightPlanUiState" }
    val planStateTestTag = remember(projectedPlanUiState) {
        val activeRows = projectedPlanUiState.displayRows
            .filter { it.active }
            .joinToString(",") { it.uid }
            .ifEmpty { "none" }
        val guidance = projectedPlanUiState.guidance
        "parity:plan-state:rows:${projectedPlanUiState.displayRows.size}" +
            ":active:$activeRows" +
            ":from:${guidance?.activeFromRowUid ?: "none"}" +
            ":to:${guidance?.activeToRowUid ?: "none"}"
    }
    val guidance = projectedPlanUiState.guidance
    val planControls = projectedPlanUiState.controls
    val altitudePlanner = projectedPlanUiState.altitudePlanner
    fun performFlightPlanControl(controlId: FlightPlanControlId) {
        when (controlId) {
            FlightPlanControlId.ActivateNextLeg ->
                applySessionCommand("activateNextLeg") { uiSession.activateNextLeg() }
            FlightPlanControlId.Redo ->
                applySessionCommand("redoFlightPlanEdit") { uiSession.redoFlightPlanEdit() }
            FlightPlanControlId.RestoreDirectTo ->
                applySessionCommand("restoreDirectTo") { uiSession.restoreDirectTo() }
            FlightPlanControlId.SequenceActiveLeg ->
                applySessionCommand("sequenceActiveLeg") { uiSession.sequenceActiveLeg() }
            FlightPlanControlId.StopNavigation ->
                applySessionCommand("stopNavigation") { uiSession.stopNavigation() }
            FlightPlanControlId.SuspendSequencing ->
                applySessionCommand("suspendSequencing") { uiSession.suspendSequencing() }
            FlightPlanControlId.Undo ->
                applySessionCommand("undoFlightPlanEdit") { uiSession.undoFlightPlanEdit() }
            FlightPlanControlId.UnsuspendSequencing ->
                applySessionCommand("unsuspendSequencing") { uiSession.unsuspendSequencing() }
        }
    }
    val rows = remember(projectedPlanUiState.displayRows) {
        buildFlightPlanDisplayRows(projectedPlanUiState)
    }
    val blocks = remember(rows) {
        buildFlightPlanDisplayBlocks(rows)
    }
    val configuration = LocalConfiguration.current
    val narrowPortraitWaypointTray =
        configuration.screenWidthDp <= 720 && configuration.screenHeightDp > configuration.screenWidthDp
    val waypointActionButtonWidth = ThumbSize * 2.5f
    val waypointActionGap = 3.dp
    val waypointTrayWidth = waypointActionButtonWidth * 2f + waypointActionGap + 6.dp
    val planWaypointTrayStart =
        if (narrowPortraitWaypointTray) {
            (configuration.screenWidthDp.dp - waypointTrayWidth - ThumbGap).coerceAtLeast(ThumbGap)
        } else {
            ThumbGap + PlanArrowLane + ThumbSize * 3.15f + PlanGridGap
        }
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
    val overlayPresentation = overlayState.present()
    val selectedWaypointUid = overlayPresentation.selectedRowUid
    val selectedRow = selectedWaypointUid?.let { uid -> rows.find { row -> row.id == uid } }
    val selectedRowBounds = selectedRow?.let { structuredRowBounds[it.id] }
    val waypointTrayStart = planWaypointTrayStart
    val selectedRowActionMatrix = selectedRow?.actionMatrix.orEmpty()
    fun procedurePickerRowCount(picker: AndroidProcedurePickerState): Int =
        when {
            picker.loading || picker.error != null -> 2
            picker.selectedProcedureId == null -> 1 + (picker.procedures.size + 1) / 2
            else -> 1 + (picker.options?.validChoices?.size ?: 0)
        }
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
                            procedurePickerRowCount(picker)
                        }
                        airwayPicker != null -> {
                            val picker = airwayPicker!!
                            when {
                                picker.loading || picker.error != null -> 2
                                picker.selectedAirwayName == null -> 1 + picker.suggestions.size
                                picker.selectedEntryUid == null -> 1 + (picker.presentation?.points?.size ?: 0)
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
                            procedurePickerRowCount(picker)
                        }
                        airwayPicker != null -> {
                            val picker = airwayPicker!!
                            when {
                                picker.loading || picker.error != null -> 2
                                picker.selectedAirwayName == null -> 1 + picker.suggestions.size
                                picker.selectedEntryUid == null -> 1 + (picker.presentation?.points?.size ?: 0)
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
    val procedureChoiceButtonWidth = ThumbSize * 3f
    val procedureTrayWidth = procedureChoiceButtonWidth * 2f + waypointActionGap + 6.dp
    val waypointTrayPaneBottom =
        structuredSurfaceBounds?.let { surfaceBounds ->
            with(density) { (surfaceBounds.bottom - surfaceBounds.top).toDp() } +
                ThumbSize + ThumbGap * 1.25f
        } ?: (configuration.screenHeightDp.dp - ThumbSize * 2.15f)
    val waypointTrayMaxHeight =
        (waypointTrayPaneBottom - waypointTrayTop - ThumbGap)
            .coerceAtLeast(ThumbSize * 2f)
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
        onOverlayAction(FlightPlanOverlayAction.Dismiss)
        selectedWaypointTrayAnchor = null
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
        val snapshot = applySessionCommand("appendFlightPlanEntry") {
            uiSession.appendFlightPlanEntry(input)
        }
        if (snapshot != null) {
            routeEntryText = ""
            routeEntryPreview = emptyFlightPlanEntryPreview()
            keyboardController?.hide()
            focusManager.clearFocus(force = true)
        }
        routeEntrySubmitting = false
    }

    fun performRouteEntryNavigation(action: () -> Unit) {
        if (SystemClock.elapsedRealtime() < routeEntrySuppressNavigationUntilMs) {
            return
        }
        keyboardController?.hide()
        focusManager.clearFocus(force = true)
        action()
    }

    fun currentRouteEntryPreviewState(): RouteEntryPreviewUiState =
        RouteEntryPreviewUiState(
            preview = routeEntryPreview,
            loading = routeEntryLoading,
            error = routeEntryError,
        )

    fun applyRouteEntryPreviewState(next: RouteEntryPreviewUiState) {
        routeEntryPreview = next.preview
        routeEntryLoading = next.loading
        routeEntryError = next.error
    }

    LaunchedEffect(
        projectedPlanUiState.planId,
        projectedPlanUiState.planVersion,
        routeEntryText,
    ) {
        val input = routeEntryText.trim()
        val request = routeEntryPreviewController.begin(input, currentRouteEntryPreviewState())
        applyRouteEntryPreviewState(request.state)
        if (!request.shouldFetch) {
            return@LaunchedEffect
        }
        try {
            val preview =
                withContext(Dispatchers.IO) {
                    uiSession.previewFlightPlanEntry(routeEntryText)
                }
            applyRouteEntryPreviewState(
                routeEntryPreviewController.complete(request.id, preview, currentRouteEntryPreviewState()),
            )
        } catch (error: CancellationException) {
            throw error
        } catch (error: Throwable) {
            applyRouteEntryPreviewState(
                routeEntryPreviewController.fail(request.id, error, currentRouteEntryPreviewState()),
            )
        } finally {
            applyRouteEntryPreviewState(
                routeEntryPreviewController.finish(request.id, currentRouteEntryPreviewState()),
            )
        }
    }

    LaunchedEffect(airportInsert?.rowUid, airportInsert?.before, airportInsert?.airportId) {
        val editor = airportInsert ?: return@LaunchedEffect
        val query = editor.airportId.trim().uppercase()
        if (query.isEmpty()) {
            airportInsert = editor.copy(loading = false, suggestions = emptyList())
            return@LaunchedEffect
        }
        airportInsert = editor.copy(loading = true)
        try {
            val suggestions = withContext(Dispatchers.IO) {
                uiSession.suggestWaypointIdentifiersAtFlightPlanRow(editor.rowUid, editor.before, query, 8)
            }
            airportInsert = airportInsert
                ?.takeIf {
                    it.rowUid == editor.rowUid &&
                        it.before == editor.before &&
                        it.airportId.trim().uppercase() == query
                }
                ?.copy(loading = false, suggestions = suggestions)
        } catch (error: CancellationException) {
            throw error
        } catch (error: Throwable) {
            airportInsert = airportInsert
                ?.takeIf {
                    it.rowUid == editor.rowUid &&
                        it.before == editor.before &&
                        it.airportId.trim().uppercase() == query
                }
                ?.copy(
                    loading = false,
                    suggestions = emptyList(),
                    error = error.message ?: error.toString(),
                )
        }
    }

    LaunchedEffect(rows, selectedWaypointUid) {
        if (selectedWaypointUid != null && selectedRow == null) {
            closePanels()
        }
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
        Column(
            modifier = Modifier
                .fillMaxSize()
                .padding(
                    top = ThumbGap,
                    start = ThumbGap,
                    end = ThumbGap,
                    bottom = ThumbSize * 2f + ThumbGap * 2f,
                ),
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
                        .padding(start = PlanArrowLane)
                        .testTag(planStateTestTag),
                    verticalArrangement = Arrangement.spacedBy(PlanGridGap),
                ) {
                    PlanHeaderRow(
                        columns = planUiState.dataColumns,
                        dataScrollState = planDataScrollState,
                        onDataColumnAction = { actionId ->
                            applySessionCommand("performFlightPlanColumnAction") {
                                uiSession.performFlightPlanColumnAction(actionId)
                            }
                        },
                    )
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
                                        selected = selectedWaypointUid == block.row.id,
                                        dataScrollState = planDataScrollState,
                                        structuredRowBounds = structuredRowBounds,
                                        onWaypointClick = {
                                            trayOpenedAtMs = SystemClock.elapsedRealtime()
                                            selectedWaypointTrayAnchor =
                                                structuredSurfaceBounds?.let { surface ->
                                                    structuredRowBounds[block.row.id]?.top?.let { top ->
                                                        with(density) { (top - surface.top).toDp() } + (ThumbSize + ThumbGap * 1.25f)
                                                    }
                                                }
                                            onOverlayAction(FlightPlanOverlayAction.SelectRow(block.row.id))
                                            airwayPicker = null
                                            procedurePicker = null
                                            airportInsert = null
                                        },
                                        onDataCellAction = { actionId ->
                                            applySessionCommand("performTimeDisplayAction") {
                                                uiSession.performTimeDisplayAction(actionId)
                                            }
                                        },
                                    )
                                }

                                is FlightPlanDisplayBlock.Group -> {
                                    FlightPlanGroupBlock(
                                        header = block.header,
                                        headerSelected = selectedWaypointUid == block.header.id,
                                        dataScrollState = planDataScrollState,
                                        structuredRowBounds = structuredRowBounds,
                                        onHeaderClick = {
                                            trayOpenedAtMs = SystemClock.elapsedRealtime()
                                            selectedWaypointTrayAnchor =
                                                structuredSurfaceBounds?.let { surface ->
                                                    structuredRowBounds[block.header.id]?.top?.let { top ->
                                                        with(density) { (top - surface.top).toDp() } + (ThumbSize + ThumbGap * 1.25f)
                                                    }
                                                }
                                            onOverlayAction(FlightPlanOverlayAction.SelectRow(block.header.id))
                                            airwayPicker = null
                                            procedurePicker = null
                                            airportInsert = null
                                        },
                                        onDataCellAction = { actionId ->
                                            applySessionCommand("performTimeDisplayAction") {
                                                uiSession.performTimeDisplayAction(actionId)
                                            }
                                        },
                                        children = block.children,
                                        selectedWaypointUid = selectedWaypointUid,
                                        onChildClick = { childRow ->
                                            trayOpenedAtMs = SystemClock.elapsedRealtime()
                                            selectedWaypointTrayAnchor =
                                                structuredSurfaceBounds?.let { surface ->
                                                    structuredRowBounds[childRow.id]?.top?.let { top ->
                                                            with(density) { (top - surface.top).toDp() } + (ThumbSize + ThumbGap * 1.25f)
                                                    }
                                                }
                                            onOverlayAction(FlightPlanOverlayAction.SelectRow(childRow.id))
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
                            color = uiTheme.flightPlanRoute.guidanceArrow,
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
                            drawPath(head, color = uiTheme.flightPlanRoute.guidanceArrow)
                        }
                    }
                }
            }
        }

        Box(
            modifier = Modifier
                .align(Alignment.BottomCenter)
                .fillMaxWidth()
                .height(ThumbSize * 2f + ThumbGap * 2f)
                .padding(start = ThumbGap, end = ThumbGap, bottom = ThumbGap),
        ) {
            Row(
                modifier = Modifier.align(Alignment.TopCenter),
                horizontalArrangement = Arrangement.spacedBy(ThumbGap),
            ) {
                planControls.forEach { control ->
                    CompactSquareButton(
                        label = control.label,
                        modifier = Modifier.size(ThumbSize),
                        maxLines = 2,
                        enabled = control.enabled,
                        testTag = "parity:plan-control:${control.id.coreId()}",
                        onDisabledClick = control.disabledReason?.let { reason ->
                            { showDisabledActionToast(context, reason) }
                        },
                        onClick = { performFlightPlanControl(control.id) },
                    )
                }
                Surface(
                    modifier = Modifier
                        .width(ThumbSize * 3.2f)
                        .height(ThumbSize)
                        .testTag("parity:plan-estimate-mode")
                        .clickable { onSelectPage(AppPage.AltitudePlanner) },
                    color = uiTheme.controls.panelBg,
                    shape = RoundedCornerShape(ThumbRadius),
                    border = BorderStroke(1.dp, uiTheme.controls.panelBorder),
                ) {
                    Box(contentAlignment = Alignment.Center) {
                        Text(
                            text = altitudePlanner.estimateSummary.label,
                            modifier = Modifier.padding(ThumbSize * 0.12f),
                            color = if (altitudePlanner.estimateSummary.estimateKind == "modeled") {
                                uiTheme.controls.flightDataModeledValue
                            } else {
                                uiTheme.controls.panelFg
                            },
                            style = MaterialTheme.typography.labelMedium,
                            fontWeight = FontWeight.Bold,
                            lineHeight = 13.sp,
                            textAlign = TextAlign.Center,
                            maxLines = 3,
                            overflow = TextOverflow.Ellipsis,
                        )
                    }
                }
            }
            PrimaryNavigationDock(
                currentPage = page,
                navElement = navElement,
                chartPlateTargetPage = mostRecentChartOrPlatePage,
                onHomeClick = {
                    performRouteEntryNavigation {
                        onSelectPage(AppPage.Home)
                    }
                },
                onOpenPlan = null,
                onSelectPage = onSelectPage,
                onOpenChartOrPlate = {
                    performRouteEntryNavigation {
                        onOpenRecentChartOrPlate()
                    }
                },
                modifier = Modifier.align(Alignment.BottomCenter),
            )
        }

        if (selectedWaypointUid != null && selectedRow != null) {
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
                                appCore.resolveNavRefIdentifier(airportId)
                            }.onSuccess { waypoint ->
                                if (applySessionCommand("insertWaypointAtFlightPlanRow") {
                                        uiSession.insertWaypointAtFlightPlanRow(editor.rowUid, editor.before, waypoint)
                                    } != null) {
                                    closePanels()
                                }
                            }.onFailure { error ->
                                airportInsert = editor.copy(error = error.message ?: error.toString())
                            }
                        },
                        onSuggestionClick = { suggestion ->
                            if (applySessionCommand("insertWaypointAtFlightPlanRow") {
                                uiSession.insertWaypointAtFlightPlanRow(editor.rowUid, editor.before, suggestion.navRef)
                            } != null) {
                                closePanels()
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
                    width = if (picker.selectedProcedureId == null) procedureTrayWidth else waypointTrayWidth,
                ) {
                    MenuPanelRow(label = "${picker.kind.title().uppercase()} ${picker.airportId}", active = false, enabled = false, onSelect = {})
                    if (picker.error != null) {
                        MenuPanelRow(label = picker.error, active = false, enabled = false, onSelect = {})
                    }
                    if (picker.loading) {
                        MenuPanelRow(label = "Loading…", active = false, enabled = false, onSelect = {})
                    } else if (picker.selectedProcedureId == null) {
                        if (picker.procedures.isEmpty()) {
                            MenuPanelRow(label = picker.kind.noPublishedProceduresLabel(), active = false, enabled = false, onSelect = {})
                        } else {
                            picker.procedures.chunked(2).forEach { rowProcedures ->
                                Row(horizontalArrangement = Arrangement.spacedBy(waypointActionGap)) {
                                    rowProcedures.forEach { procedure ->
                                        MenuPanelRow(
                                            label = procedure.displayLabel,
                                            active = false,
                                            enabled = procedure.enabled,
                                            disabledReason = procedure.disabledReason,
                                            accentColor = plateFolderColor(uiTheme, procedure.accentCategory),
                                            width = procedureChoiceButtonWidth,
                                            maxLines = 1,
                                            onSelect = {
                                                procedurePicker = picker.copy(loading = true, error = null)
                                                runCatching {
                                                    appCore.describeProcedureOptions(
                                                        picker.airportId,
                                                        procedure.procedureId,
                                                        picker.kind,
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
                                }
                            }
                        }
                    } else {
                        val choices = picker.options?.validChoices.orEmpty()
                        if (choices.isEmpty()) {
                            MenuPanelRow(label = "No published routes are available.", active = false, enabled = false, onSelect = {})
                        }
                        choices.forEach { choice ->
                            MenuPanelRow(
                                label = procedureChoiceLabel(
                                    picker.kind,
                                    choice.runwayTransition,
                                    choice.enrouteTransition,
                                ),
                                active = false,
                                enabled = true,
                                onSelect = {
                                    procedurePicker = picker.copy(loading = true, error = null)
                                    val snapshot = applySessionCommand("selectProcedureAtFlightPlanRow") {
                                        uiSession.selectProcedureAtFlightPlanRow(
                                            picker.rowUid,
                                            picker.airportId,
                                            picker.selectedProcedureId,
                                            picker.kind,
                                            choice.runwayTransition,
                                            choice.enrouteTransition,
                                        )
                                    }
                                    if (snapshot != null) {
                                        closePanels()
                                    } else {
                                        procedurePicker = picker.copy(loading = false)
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
                        .heightIn(max = waypointTrayMaxHeight)
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
                        LazyColumn(
                            modifier = Modifier
                                .fillMaxWidth()
                                .weight(1f, fill = false),
                            verticalArrangement = Arrangement.spacedBy(waypointActionGap),
                        ) {
                            items(picker.suggestions.size) { index ->
                                val suggestion = picker.suggestions[index]
                                MenuPanelRow(
                                    label = suggestion.airwayName,
                                    active = false,
                                    enabled = true,
                                    onSelect = {
                                        airwayPicker = picker.copy(loading = true, error = null)
                                        runCatching {
                                            uiSession.prepareAirwayPresentationAtFlightPlanRow(
                                                picker.rowUid,
                                                suggestion.airwayName,
                                            )
                                        }.onSuccess { presentation ->
                                            airwayPicker =
                                                picker.copy(
                                                    loading = false,
                                                    selectedAirwayName = suggestion.airwayName,
                                                    presentation = presentation,
                                                    selectedEntryUid = null,
                                                )
                                        }.onFailure { error ->
                                            airwayPicker = picker.copy(loading = false, error = error.message ?: error.toString())
                                        }
                                    },
                                )
                            }
                        }
                    } else if (picker.selectedEntryUid == null) {
                        val presentation = requireNotNull(picker.presentation)
                        LazyColumn(
                            modifier = Modifier
                                .fillMaxWidth()
                                .weight(1f, fill = false),
                            verticalArrangement = Arrangement.spacedBy(waypointActionGap),
                        ) {
                            items(presentation.points.size, key = { presentation.points[it].uid }) { index ->
                                val point = presentation.points[index]
                                MenuPanelRow(
                                    label = navRefLabel(point.navRef),
                                    active = point.uid == presentation.suggestedEntryUid,
                                    enabled = true,
                                    onSelect = {
                                        airwayPicker = picker.copy(selectedEntryUid = point.uid)
                                    },
                                )
                            }
                        }
                        MenuPanelRow(label = "Back", active = false, enabled = true, onSelect = { airwayPicker = picker.copy(selectedAirwayName = null, presentation = null) })
                    } else {
                        val presentation = requireNotNull(picker.presentation)
                        LazyColumn(
                            modifier = Modifier
                                .fillMaxWidth()
                                .weight(1f, fill = false),
                            verticalArrangement = Arrangement.spacedBy(waypointActionGap),
                        ) {
                            items(presentation.points.size, key = { presentation.points[it].uid }) { exitIndex ->
                                val point = presentation.points[exitIndex]
                                val isEntry = point.uid == picker.selectedEntryUid
                                MenuPanelRow(
                                    label = navRefLabel(point.navRef),
                                    active = point.uid == presentation.suggestedExitUid,
                                    enabled = !isEntry,
                                    disabledReason = if (isEntry) "That fix is the airway entry; choose an exit." else null,
                                    onSelect = {
                                        if (isEntry) return@MenuPanelRow
                                        airwayPicker = picker.copy(loading = true, error = null)
                                        val snapshot = applySessionCommand("insertAirwayAtFlightPlanRow") {
                                            uiSession.insertAirwayAtFlightPlanRow(
                                                picker.rowUid,
                                                presentation,
                                                requireNotNull(picker.selectedEntryUid),
                                                point.uid,
                                            )
                                        }
                                        if (snapshot != null) {
                                            closePanels()
                                        } else {
                                            airwayPicker = picker.copy(loading = false)
                                        }
                                    },
                                )
                            }
                        }
                        MenuPanelRow(label = "Back", active = false, enabled = true, onSelect = { airwayPicker = picker.copy(selectedEntryUid = null) })
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
                            (0..1).forEach { menuColumn ->
                                val action = actionRow.find { it.menuColumn == menuColumn }
                                if (action == null) {
                                    Box(modifier = Modifier.width(waypointActionButtonWidth))
                                    return@forEach
                                }
                                MenuPanelRow(
                                    label = action.label,
                                    active = false,
                                    enabled = action.enabled,
                                    disabledReason = action.disabledReason,
                                    testTag = "parity:plan-row-action:${action.id}",
                                    width = waypointActionButtonWidth,
                                    trailingContent = if (hasActionSymbol(action.id)) {
                                        {
                                            ActionIcon(
                                                actionId = action.id,
                                                enabled = action.enabled,
                                                modifier = Modifier.size(ThumbSize * 0.62f),
                                            )
                                        }
                                    } else {
                                        null
                                    },
                                    onSelect = {
                                        if (!action.enabled) {
                                            return@MenuPanelRow
                                        }
                                        action.weatherDetail?.let { detail ->
                                            selectedWaypointTrayAnchor = null
                                            airwayPicker = null
                                            procedurePicker = null
                                            airportInsert = null
                                            onOverlayAction(FlightPlanOverlayAction.ShowWeather(detail))
                                            return@MenuPanelRow
                                        }
                                        action.airportInfoAirportId?.let { airportId ->
                                            selectedWaypointTrayAnchor = null
                                            airwayPicker = null
                                            procedurePicker = null
                                            airportInsert = null
                                            onOverlayAction(
                                                FlightPlanOverlayAction.ShowAirportInfo(airportId),
                                            )
                                            airportInfoScope.launch {
                                                runCatching {
                                                    withContext(Dispatchers.IO) {
                                                        uiSession.airportInfo(airportId)
                                                    }
                                                }.onSuccess { detail ->
                                                    onOverlayAction(
                                                        FlightPlanOverlayAction.ResolveAirportInfo(
                                                            airportId = airportId,
                                                            detail = detail,
                                                        ),
                                                    )
                                                }.onFailure { error ->
                                                    onOverlayAction(
                                                        FlightPlanOverlayAction.FailAirportInfo(
                                                            airportId = airportId,
                                                            error = error.message
                                                                ?: error.toString(),
                                                        ),
                                                    )
                                                }
                                            }
                                            return@MenuPanelRow
                                        }
                                        if (action.execution == "core_session") {
                                            val snapshot = applySessionCommand("performFlightPlanRowAction") {
                                                uiSession.performFlightPlanRowAction(selectedRow.id, action.uid)
                                            }
                                            if (snapshot != null && action.dismissTrayOnSuccess) {
                                                closePanels()
                                            }
                                            return@MenuPanelRow
                                        }
                                        when (val navigation = action.navigation) {
                                            is FlightPlanRowNavigationAction.OpenAirportCharts -> {
                                                onOpenCharts(navigation.airportId, null)
                                                closePanels()
                                                return@MenuPanelRow
                                            }
                                            is FlightPlanRowNavigationAction.OpenPlateTarget -> {
                                                onOpenCharts(navigation.airportId, navigation.target)
                                                closePanels()
                                                return@MenuPanelRow
                                            }
                                            null -> Unit
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
                                                        selectedEntryUid = null,
                                                    )
                                                runCatching {
                                                    appCore.suggestAirwaysNear(selectedRow.originAnchor!!)
                                                }.onSuccess { suggestions ->
                                                    airwayPicker = airwayPicker?.copy(loading = false, suggestions = suggestions)
                                                }.onFailure { error ->
                                                    airwayPicker = airwayPicker?.copy(loading = false, error = error.message ?: error.toString())
                                                }
                                            }
                                            "select_departure",
                                            "select_arrival",
                                            "select_approach",
                                            -> {
                                                val airportId = selectedRow.chartAirportId ?: return@MenuPanelRow
                                                val procedureKind = action.procedureKind ?: return@MenuPanelRow
                                                procedurePicker =
                                                    AndroidProcedurePickerState(
                                                        loading = true,
                                                        error = null,
                                                        rowUid = selectedRow.id,
                                                        airportId = airportId,
                                                        kind = procedureKind,
                                                        procedures = emptyList(),
                                                        selectedProcedureId = null,
                                                        options = null,
                                                    )
                                                runCatching {
                                                    appCore.listProcedures(airportId, procedureKind)
                                                }.onSuccess { procedures ->
                                                    procedurePicker = procedurePicker?.copy(loading = false, procedures = procedures)
                                                }.onFailure { error ->
                                                    Log.e("AerobagProcedure", "listProcedures failed airport=$airportId", error)
                                                    procedurePicker = procedurePicker?.copy(loading = false, error = error.message ?: error.toString())
                                                }
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

private fun ProcedureKind.title() = when (this) {
    ProcedureKind.Sid -> "Departure"
    ProcedureKind.Star -> "Arrival"
    ProcedureKind.Approach -> "Approach"
}

private fun ProcedureKind.noPublishedProceduresLabel() = when (this) {
    ProcedureKind.Sid -> "No published departures are available."
    ProcedureKind.Star -> "No published arrivals are available."
    ProcedureKind.Approach -> "No published approaches are available."
}

private fun procedureChoiceLabel(
    kind: ProcedureKind,
    runwayTransition: String?,
    enrouteTransition: String?,
): String = when (kind) {
    ProcedureKind.Sid -> when {
        runwayTransition != null && enrouteTransition != null ->
            "via $runwayTransition to $enrouteTransition"
        runwayTransition != null -> "via $runwayTransition"
        enrouteTransition != null -> "to $enrouteTransition"
        else -> "Published route"
    }
    ProcedureKind.Star -> when {
        runwayTransition != null && enrouteTransition != null ->
            "from $enrouteTransition to $runwayTransition"
        runwayTransition != null -> "to $runwayTransition"
        enrouteTransition != null -> "from $enrouteTransition"
        else -> "Published route"
    }
    ProcedureKind.Approach -> when {
        enrouteTransition != null -> "from $enrouteTransition"
        runwayTransition != null -> "from $runwayTransition"
        else -> "Published route"
    }
}

private fun FlightPlanControlId.coreId() = when (this) {
    FlightPlanControlId.ActivateNextLeg -> "activate_next_leg"
    FlightPlanControlId.Redo -> "redo"
    FlightPlanControlId.RestoreDirectTo -> "restore_direct_to"
    FlightPlanControlId.SequenceActiveLeg -> "sequence_active_leg"
    FlightPlanControlId.StopNavigation -> "stop_navigation"
    FlightPlanControlId.SuspendSequencing -> "suspend_sequencing"
    FlightPlanControlId.Undo -> "undo"
    FlightPlanControlId.UnsuspendSequencing -> "unsuspend_sequencing"
}

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
    val submitAction = rememberCurrentAction(onSubmit)
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
            keyboardActions = KeyboardActions(onDone = { submitAction() }),
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
                            submitAction()
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
