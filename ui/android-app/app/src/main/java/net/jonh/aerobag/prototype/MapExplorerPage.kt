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
import net.jonh.aerobag.prototype.domain.InstalledPackageKind
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
import net.jonh.aerobag.prototype.domain.MapView
import net.jonh.aerobag.prototype.domain.MapViewOption
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
import net.jonh.aerobag.prototype.domain.SampleData
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
import net.jonh.aerobag.prototype.domain.tileRelativePath
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


@OptIn(ExperimentalComposeUiApi::class)
@Composable
internal fun MapExplorerPage(
    appCore: NativeAppCoreAdapter,
    page: AppPage,
    pageHistory: List<AppViewSnapshot>,
    uptimeLabel: String,
    fixture: net.jonh.aerobag.prototype.domain.ContentFixture,
    uiSession: NativeUiSession,
    sessionSnapshot: UiSessionSnapshot,
    uiTheme: UiTheme,
    ownship: OwnshipRenderState,
    playbackUiState: PlaybackUiState,
    playbackSourcePath: String,
    mapFollowUiState: MapFollowUiState,
    mapFollowTargetViewport: CoreMapViewport?,
    situationRingCandidates: List<SituationRingCandidate>,
    selectedMap: MapViewOption,
    mapFamilyOptions: List<MapFamilyOption>,
    viewport: MapViewportState,
    decodedTileBitmapCache: DecodedTileBitmapCache,
    debugState: UiDebugState,
    pageTilePaintTiming: PageTilePaintTiming?,
    ownshipControls: OwnshipControlModel,
    onPageTilePaintTimingComplete: (Long) -> Unit,
    onViewportChange: (MapViewportState) -> Unit,
    onSessionSnapshotChange: (UiSessionSnapshot) -> Unit,
    onSelectOwnshipSource: (String) -> Unit,
    onSituationControlInput: (SituationControlInput) -> Unit,
    onPlaybackSourcePathChange: (String) -> Unit,
    onSelectMapFamily: (MapChartFamily) -> Unit,
    onSelectPage: (AppPage) -> Unit,
    onOpenPlan: () -> Unit,
    navElement: NavElementUiView?,
    plan: net.jonh.aerobag.prototype.domain.FlightPlan,
    planUiState: FlightPlanUiState?,
) {
    val context = LocalContext.current
    val activity = context as? MainActivity
    val density = LocalDensity.current
    val json = remember { Json { ignoreUnknownKeys = true } }
    val devServerBaseUrl = remember(context) {
        loadAndroidDevServerBaseUrl(context.applicationContext)
    }
    val focusRequester = remember { FocusRequester() }
    var chartTrayOpen by remember { mutableStateOf(false) }
    var layerTrayOpen by remember { mutableStateOf(false) }
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
                neededPointTiles = emptyList(),
                neededMetarTiles = emptyList(),
                neededAirspaceRefTiles = emptyList(),
                neededAirspaceFeatures = emptyList(),
                neededAirspaceLabelTiles = emptyList(),
                neededMetars = false,
                neededTfrs = false,
                visibleFeatures = emptyList(),
                visibleMetars = emptyList(),
                visiblePireps = emptyList(),
                airspacePaths = emptyList(),
                tfrPaths = emptyList(),
                airspaceLabels = emptyList(),
                offlineRegions = emptyList(),
                warnings = emptyList(),
            ),
        )
    }
    var committedOverlayViewport by remember(uiSession) { mutableStateOf<MapViewportState?>(null) }
    var committedOverlaySurfaceUnits by remember(uiSession) { mutableStateOf<OverlaySurfaceUnits?>(null) }
    var mapOverlayError by remember(uiSession) { mutableStateOf<String?>(null) }
    var nexradFrames by remember(uiSession) { mutableStateOf<List<NexradOverlayFrame>>(emptyList()) }
    var nexradFrameSourceUrl by remember(uiSession) { mutableStateOf<String?>(null) }
    var nexradFrameIndex by remember(uiSession) { mutableStateOf(0) }
    var terrainOverlay by remember(uiSession) { mutableStateOf<List<TerrainOverlayImage>>(emptyList()) }
    var terrainOverlayError by remember(uiSession) { mutableStateOf<String?>(null) }
    var flightPlanRoute by remember(plan.id, plan.version) { mutableStateOf<List<FlightPlanRouteSegment>>(emptyList()) }
    var mapGestureActive by remember { mutableStateOf(false) }
    var installingPackage by remember { mutableStateOf<String?>(null) }
    var installRevision by remember { mutableStateOf(0) }
    val selectedFamilyMapViews = remember(selectedMap, fixture.mapViews) {
        val chartFamilies =
            when (selectedMap.mapView.chartFamily) {
                MapChartFamily.Tac -> setOf(MapChartFamily.Sec, MapChartFamily.Tac)
                else -> setOf(selectedMap.mapView.chartFamily)
            }
        fixture.mapViews.filter { it.mapView.chartFamily in chartFamilies }
    }
    val viewportState = remember(selectedMap.id) { mutableStateOf(viewport) }
    var viewportSyncPending by remember(selectedMap.id) { mutableStateOf(false) }
    LaunchedEffect(viewport, selectedMap.id) {
        val parentMatchesLocal = sameMapViewport(viewport, viewportState.value)
        Log.i(
            MapViewportLogTag,
            "prop-sync map=${selectedMap.id} parentZoom=${"%.2f".format(viewport.zoom)} localZoom=${"%.2f".format(viewportState.value.zoom)} parentCenter=${"%.3f".format(viewport.centerWorldX)},${"%.3f".format(viewport.centerWorldY)} localCenter=${"%.3f".format(viewportState.value.centerWorldX)},${"%.3f".format(viewportState.value.centerWorldY)} pending=$viewportSyncPending matches=$parentMatchesLocal",
        )
        when {
            !viewportSyncPending -> {
                viewportState.value = viewport
            }
            parentMatchesLocal -> {
                viewportSyncPending = false
            }
            else -> {
                Log.i(
                    MapViewportLogTag,
                    "prop-sync ignored stale parent map=${selectedMap.id} parentCenter=${"%.3f".format(viewport.centerWorldX)},${"%.3f".format(viewport.centerWorldY)} localCenter=${"%.3f".format(viewportState.value.centerWorldX)},${"%.3f".format(viewportState.value.centerWorldY)}",
                )
            }
        }
    }
    val currentViewport = viewportState.value
    val surfaceWidthPx = surfaceSize.width.toFloat()
    val surfaceHeightPx = surfaceSize.height.toFloat()
    val surfaceWidthDp = remember(surfaceSize, density) { with(density) { surfaceSize.width.toDp().value } }
    val surfaceHeightDp = remember(surfaceSize, density) { with(density) { surfaceSize.height.toDp().value } }
    val situationDockTopPadding =
        if (surfaceWidthDp.dp < SituationDockOverlapWidth) ThumbSize + (ThumbGap * 2f) else ThumbGap
    val tiles = remember(selectedMap.id, currentViewport, surfaceSize, fixture.mapViews, uiSession, debugState.fastTiles) {
        if (surfaceSize.width == 0 || surfaceSize.height == 0) {
            emptyList()
        } else {
            val planStartMs = SystemClock.elapsedRealtime()
            val mapViewsById = fixture.mapViews.associateBy { it.id }
            val plan = json.decodeFromString<WireRasterTilePlan>(
                uiSession.queryRasterTilePlanJson(
                    currentViewport,
                    surfaceWidthPx.toDouble(),
                    surfaceHeightPx.toDouble(),
                ),
            )
            val planMs = SystemClock.elapsedRealtime() - planStartMs
            pageTilePaintTiming?.let { timing ->
                Log.i(
                    TileBudgetLogTag,
                    "page-to-map-plan id=${timing.id} from=${timing.fromPage} elapsedMs=${SystemClock.elapsedRealtime() - timing.startedMs} planMs=$planMs tiles=${plan.tiles.size} fastTiles=${debugState.fastTiles}",
                )
            }
            plan.tiles.mapNotNull { tile ->
                val primaryOption = mapViewsById[tile.primary.map_view_id] ?: return@mapNotNull null
                val candidateMapViews = (listOf(tile.primary) + tile.fallbacks)
                    .mapNotNull { source -> mapViewsById[source.map_view_id]?.mapView }
                    .ifEmpty { listOf(primaryOption.mapView) }
                RenderTile(
                    x = tile.x,
                    yTms = tile.y_tms,
                    leftPx = tile.left_px.toFloat(),
                    topPx = tile.top_px.toFloat(),
                    sizePx = tile.size_px.toFloat(),
                    zoom = tile.source_zoom,
                    mapViewId = primaryOption.id,
                    mapView = primaryOption.mapView,
                    candidateMapViews = candidateMapViews,
                )
            }
        }
    }
    val selectedPackageName = selectedMap.mapView.packageName
    val mapLayerState = sessionSnapshot.mapLayerState
    val topLeftTrayOpen = chartTrayOpen || layerTrayOpen
    val selectedPackageInstalled = remember(selectedPackageName, installRevision) {
        selectedPackageName?.let { SectionalPackages.isInstalled(context, it) } ?: true
    }
    val familyPackageNames = remember(selectedFamilyMapViews) {
        selectedFamilyMapViews.mapNotNull { it.mapView.packageName }.distinct()
    }
    val installedFamilyPackageCount = remember(familyPackageNames, installRevision) {
        familyPackageNames.count { SectionalPackages.isInstalled(context, it) }
    }
    val sourceZooms = tiles.map { it.zoom }.distinct().sorted()
    val renderedPackages = tiles.mapNotNull { it.mapView.packageName }.distinct().sorted()
    val familyStatus = remember(installingPackage, installedFamilyPackageCount, familyPackageNames, selectedMap.mapView.chartFamily) {
        when {
            installingPackage != null -> "Installing ${installingPackage}..."
            installedFamilyPackageCount == familyPackageNames.size -> "Local ${selectedMap.mapView.chartFamily.name}"
            installedFamilyPackageCount > 0 -> "Partial ${selectedMap.mapView.chartFamily.name}"
            else -> "Package missing"
        }
    }
    val trayOptions = remember(mapFamilyOptions) {
        mapFamilyOptions.map { option ->
            ChartTrayOption(
                id = chartFamilyId(option.id),
                label = option.label,
                launcherLabel = option.launcherLabel,
                available = option.enabled,
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
                toggleState = mapLayerState.metars,
                iconResId = mapLayerIconResId(MapLayerId.Metars),
            ) {
                val visible = !mapLayerState.metars.visible
                val startMs = SystemClock.elapsedRealtime()
                val snapshot = uiSession.setMapLayerVisibility(MapLayerId.Metars, visible)
                Log.i(MapLayerLogTag, "toggle layer=metars visible=$visible coreMs=${SystemClock.elapsedRealtime() - startMs}")
                onSessionSnapshotChange(snapshot)
            },
            MenuDockOption(
                key = "vectors",
                label = "Vectors",
                enabled = mapLayerState.vectors.enabled,
                toggleState = mapLayerState.vectors,
                iconResId = mapLayerIconResId(MapLayerId.Vectors),
            ) {
                val visible = !mapLayerState.vectors.visible
                val startMs = SystemClock.elapsedRealtime()
                val snapshot = uiSession.setMapLayerVisibility(MapLayerId.Vectors, visible)
                Log.i(MapLayerLogTag, "toggle layer=vectors visible=$visible coreMs=${SystemClock.elapsedRealtime() - startMs}")
                onSessionSnapshotChange(snapshot)
            },
            MenuDockOption(
                key = "nexrad",
                label = "NEXRAD",
                enabled = mapLayerState.nexrad.enabled,
                toggleState = mapLayerState.nexrad,
                iconResId = mapLayerIconResId(MapLayerId.Nexrad),
            ) {
                val visible = !mapLayerState.nexrad.visible
                val startMs = SystemClock.elapsedRealtime()
                val snapshot = uiSession.setMapLayerVisibility(MapLayerId.Nexrad, visible)
                Log.i(MapLayerLogTag, "toggle layer=nexrad visible=$visible coreMs=${SystemClock.elapsedRealtime() - startMs}")
                onSessionSnapshotChange(snapshot)
            },
            MenuDockOption(
                key = "terrain_warning",
                label = "Terrain Warning",
                enabled = mapLayerState.terrainWarning.enabled,
                toggleState = mapLayerState.terrainWarning,
                iconResId = mapLayerIconResId(MapLayerId.TerrainWarning),
            ) {
                val visible = !mapLayerState.terrainWarning.visible
                val startMs = SystemClock.elapsedRealtime()
                val snapshot = uiSession.setMapLayerVisibility(MapLayerId.TerrainWarning, visible)
                Log.i(MapLayerLogTag, "toggle layer=terrain_warning visible=$visible coreMs=${SystemClock.elapsedRealtime() - startMs}")
                onSessionSnapshotChange(snapshot)
            },
            MenuDockOption(
                key = "world_basemap",
                label = "World Map",
                enabled = mapLayerState.worldBasemap.enabled,
                toggleState = mapLayerState.worldBasemap,
                iconResId = mapLayerIconResId(MapLayerId.WorldBasemap),
            ) {
                val visible = !mapLayerState.worldBasemap.visible
                val startMs = SystemClock.elapsedRealtime()
                val snapshot = uiSession.setMapLayerVisibility(MapLayerId.WorldBasemap, visible)
                Log.i(MapLayerLogTag, "toggle layer=world_basemap visible=$visible coreMs=${SystemClock.elapsedRealtime() - startMs}")
                onSessionSnapshotChange(snapshot)
            },
            MenuDockOption(
                key = "offline_regions",
                label = "Offline Regions",
                enabled = mapLayerState.offlineRegions.enabled,
                toggleState = mapLayerState.offlineRegions,
                iconResId = mapLayerIconResId(MapLayerId.OfflineRegions),
            ) {
                val visible = !mapLayerState.offlineRegions.visible
                val startMs = SystemClock.elapsedRealtime()
                val snapshot = uiSession.setMapLayerVisibility(MapLayerId.OfflineRegions, visible)
                Log.i(MapLayerLogTag, "toggle layer=offline_regions visible=$visible coreMs=${SystemClock.elapsedRealtime() - startMs}")
                onSessionSnapshotChange(snapshot)
            },
        )
    }
    val selectedLauncher = trayOptions.firstOrNull { option ->
        when (option.id) {
            "sec" -> selectedMap.mapView.chartFamily == MapChartFamily.Sec
            "tac" -> selectedMap.mapView.chartFamily == MapChartFamily.Tac
            "enr-l" -> selectedMap.mapView.chartFamily == MapChartFamily.EnrL
            "enr-h" -> selectedMap.mapView.chartFamily == MapChartFamily.EnrH
            "shaded-relief" -> selectedMap.mapView.chartFamily == MapChartFamily.ShadedRelief
            else -> false
        }
    } ?: trayOptions.first()
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
    val routeScreenSegments = remember(flightPlanRoute, currentViewport, surfaceWidthPx, surfaceHeightPx) {
        if (surfaceWidthPx <= 0f || surfaceHeightPx <= 0f) {
            emptyList()
        } else {
            flightPlanRoute.map { segment ->
                Pair(
                    (segment.path.ifEmpty { listOf(segment.from, segment.to) }).map { point ->
                        latLonToScreenPoint(currentViewport, point, surfaceWidthPx, surfaceHeightPx)
                    },
                    segment,
                )
            }
        }
    }

    fun syncFollowStateForViewport(nextViewport: MapViewportState) {
        if (!mapFollowUiState.following || surfaceWidthPx <= 0f || surfaceHeightPx <= 0f) {
            return
        }
        val overlay = resolveSituationOverlay(
            ownship = ownship,
            viewport = nextViewport,
            widthUnits = surfaceWidthPx,
            heightUnits = surfaceHeightPx,
            ringCandidates = situationRingCandidates,
        )
        if (overlay == null) {
            runCatching { uiSession.disengageMapFollow(nextViewport) }.onSuccess(onSessionSnapshotChange)
            return
        }
        val point = overlay.pointUnits
        if (point.x < 0f || point.x > surfaceWidthPx || point.y < 0f || point.y > surfaceHeightPx) {
            runCatching { uiSession.disengageMapFollow(nextViewport) }.onSuccess(onSessionSnapshotChange)
            return
        }
        runCatching {
            uiSession.setMapFollowOffset(
                nextViewport,
                (point.x - surfaceWidthPx / 2f).toDouble(),
                (point.y - surfaceHeightPx / 2f).toDouble(),
            )
        }.onSuccess(onSessionSnapshotChange)
    }

    fun updateViewport(nextViewport: MapViewportState, syncFollow: Boolean = true) {
        Log.i(
            MapViewportLogTag,
            "update map=${selectedMap.id} from=${"%.2f".format(viewportState.value.zoom)} to=${"%.2f".format(nextViewport.zoom)} fromCenter=${"%.3f".format(viewportState.value.centerWorldX)},${"%.3f".format(viewportState.value.centerWorldY)} toCenter=${"%.3f".format(nextViewport.centerWorldX)},${"%.3f".format(nextViewport.centerWorldY)} syncFollow=$syncFollow",
        )
        viewportState.value = nextViewport
        viewportSyncPending = true
        onViewportChange(nextViewport)
        if (syncFollow) {
            syncFollowStateForViewport(nextViewport)
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

    fun submitChartSearch() {
        val query = chartSearchText.trim().uppercase()
        if (query.isBlank()) {
            return
        }
        chartSearchLoading = true
        chartSearchError = null
        runCatching {
            chartSearchSuggestions.firstOrNull()?.navRef ?: appCore.resolveNavRefIdentifier(query)
        }.onSuccess { navRef ->
            recenterOnNavRef(navRef)
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
    val tileBitmapCache = remember(selectedMap.id, installRevision, debugState.fastTiles) {
        mutableStateMapOf<net.jonh.aerobag.prototype.domain.RenderTileKey, androidx.compose.ui.graphics.ImageBitmap?>()
    }
    val rasterTileBitmapLoaderScope = rememberCoroutineScope()
    val rasterTileBitmapLoader = remember(context.applicationContext, rasterTileBitmapLoaderScope) {
        RasterTileBitmapLoader(context.applicationContext, rasterTileBitmapLoaderScope)
    }
    DisposableEffect(rasterTileBitmapLoader) {
        onDispose {
            rasterTileBitmapLoader.close()
        }
    }
    LaunchedEffect(tiles, selectedMap.id, installRevision, debugState.fastTiles) {
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
        val decodedCacheStats = decodedTileBitmapCache.stats()
        Log.i(
            TileBudgetLogTag,
            "visible map=${selectedMap.id} total=${tiles.size} missing=${missingTiles.size} localCache=${tileBitmapCache.size} decodedLru=${decodedCacheStats.entries}/${decodedCacheStats.bytes}B lruHits=$decodedCacheHits fastTiles=${debugState.fastTiles} groups=[${formatTileBudgetSummary(tiles)}]",
        )
        if (missingTiles.isEmpty()) {
            pageTilePaintTiming?.takeIf { tiles.isNotEmpty() }?.let { timing ->
                withFrameNanos { }
                Log.i(
                    TileBudgetLogTag,
                    "page-to-map-frame id=${timing.id} from=${timing.fromPage} elapsedMs=${SystemClock.elapsedRealtime() - timing.startedMs} cacheOnly=true",
                )
                onPageTilePaintTimingComplete(timing.id)
            }
            return@LaunchedEffect
        }
        val loadStartMs = SystemClock.elapsedRealtime()
        val generationId = TileLoadGenerationIds.incrementAndGet()
        val (viewportLat, viewportLon) = viewportCenterLatLon(currentViewport)
        Log.i(
            TileBudgetLogTag,
            "generation-start gen=$generationId map=${selectedMap.id} zoom=${"%.2f".format(currentViewport.zoom)} center=${"%.3f".format(viewportLat)},${"%.3f".format(viewportLon)} total=${tiles.size} missing=${missingTiles.size} cache=${tileBitmapCache.size}",
        )
        var loadedThisPassCount = 0
        val loadedTiles = try {
            rasterTileBitmapLoader.loadVisibleTileBitmaps(
                selectedMap.id,
                generationId,
                missingTiles,
            ) { loaded ->
                tileBitmapCache[loaded.result.key] = loaded.result.bitmap
                val bitmap = loaded.result.bitmap
                if (bitmap != null) {
                    loadedThisPassCount += 1
                    decodedTileBitmapCache.put(decodedTileCacheKey(loaded.tile), bitmap, loaded.result.decodedBytes)
                } else {
                    Log.w(
                        TileBudgetLogTag,
                        "generation-empty gen=$generationId key=${loaded.result.key} ${formatTileRef(loaded.tile)}",
                    )
                }
            }
        } catch (error: CancellationException) {
            Log.w(
                TileBudgetLogTag,
                "generation-cancel gen=$generationId map=${selectedMap.id} loaded=$loadedThisPassCount/${missingTiles.size} elapsedMs=${SystemClock.elapsedRealtime() - loadStartMs}",
            )
            throw error
        }
        val tileResults = loadedTiles.map { it.result }
        val readElapsedMs = tileResults.sumOf { it.readMs }
        val decodeElapsedMs = tileResults.sumOf { it.decodeMs }
        val loadedBytes = tileResults.sumOf { it.bytes.toLong() }
        val loadedDecodedBytes = tileResults.sumOf { it.decodedBytes }
        Log.i(
            TileBudgetLogTag,
            "generation-finish gen=$generationId map=${selectedMap.id} loaded=$loadedThisPassCount/${missingTiles.size} bytes=$loadedBytes decodedBytes=$loadedDecodedBytes elapsedMs=${SystemClock.elapsedRealtime() - loadStartMs} readMs=$readElapsedMs decodeMs=$decodeElapsedMs",
        )
        Log.i(
            TileBudgetLogTag,
            "batch map=${selectedMap.id} loaded=$loadedThisPassCount/${missingTiles.size} bytes=$loadedBytes decodedBytes=$loadedDecodedBytes elapsedMs=${SystemClock.elapsedRealtime() - loadStartMs}",
        )
        pageTilePaintTiming?.let { timing ->
            Log.i(
                TileBudgetLogTag,
                "page-to-map-cache id=${timing.id} from=${timing.fromPage} elapsedMs=${SystemClock.elapsedRealtime() - timing.startedMs} loadMs=${SystemClock.elapsedRealtime() - loadStartMs} loaded=$loadedThisPassCount/${missingTiles.size}",
            )
            withFrameNanos { }
            Log.i(
                TileBudgetLogTag,
                "page-to-map-frame id=${timing.id} from=${timing.fromPage} elapsedMs=${SystemClock.elapsedRealtime() - timing.startedMs}",
            )
            onPageTilePaintTimingComplete(timing.id)
        }
        val loadElapsedMs = SystemClock.elapsedRealtime() - loadStartMs
        val cacheLoadedCount = tileBitmapCache.values.count { it != null }
        val cacheMissCount = tileBitmapCache.size - cacheLoadedCount
        val finalDecodedCacheStats = decodedTileBitmapCache.stats()
        val visibleTileByKey = tiles.associateBy { renderTileKey(it) }
        val cacheCounts = linkedMapOf<String, Int>()
        tileBitmapCache.forEach { (key, bitmap) ->
            val tile = visibleTileByKey[key] ?: return@forEach
            val packageLabel = tile.mapView.packageName ?: tile.mapViewId
            val summaryKey = "$packageLabel@z${tile.zoom}:${if (bitmap != null) "loaded" else "empty"}"
            cacheCounts[summaryKey] = (cacheCounts[summaryKey] ?: 0) + 1
        }
        val cacheSummary = cacheCounts.entries
            .sortedBy { it.key }
            .joinToString(", ") { entry -> "${entry.key}=${entry.value}" }
        Log.i(
            TileBudgetLogTag,
            "cache map=${selectedMap.id} entries=${tileBitmapCache.size} loaded=$cacheLoadedCount empty=$cacheMissCount fetched=$loadedThisPassCount bytes=$loadedBytes decodedBytes=$loadedDecodedBytes loadMs=$loadElapsedMs readMs=$readElapsedMs decodeMs=$decodeElapsedMs decodedLru=${finalDecodedCacheStats.entries}/${finalDecodedCacheStats.bytes}B groups=[$cacheSummary]",
        )
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

    LaunchedEffect(selectedMap.id) {
        chartTrayOpen = false
        layerTrayOpen = false
        mapSelection = null
    }
    LaunchedEffect(appCore, uiSession, plan.id, plan.version, plan.guidance, plan.resolvedLegs) {
        if (plan.resolvedLegs.isEmpty()) {
            flightPlanRoute = emptyList()
            return@LaunchedEffect
        }
        runCatching {
            appCore.projectFlightPlanRoute(plan)
        }.onSuccess {
            flightPlanRoute = it
        }.onFailure {
            flightPlanRoute = emptyList()
            Log.e("AerobagGuidance", "failed to project flight plan route", it)
        }
    }
    LaunchedEffect(selectedMap.id, chartTrayOpen, layerTrayOpen) {
        if (!chartTrayOpen && !layerTrayOpen) {
            withFrameNanos { }
            focusRequester.requestFocus()
        }
    }
    LaunchedEffect(uiSession, mapFollowUiState.following, mapFollowTargetViewport, viewport) {
        if (mapFollowUiState.following && mapFollowTargetViewport == null) {
            runCatching { uiSession.engageMapFollow(viewport) }.onSuccess(onSessionSnapshotChange)
        }
    }
    LaunchedEffect(mapFollowUiState.following, mapFollowTargetViewport, mapGestureActive) {
        if (!mapFollowUiState.following) {
            return@LaunchedEffect
        }
        if (mapGestureActive) {
            return@LaunchedEffect
        }
        val target = mapFollowTargetViewport ?: return@LaunchedEffect
        val nextViewport = mapViewportFromCore(target)
        if (!sameMapViewport(nextViewport, viewport)) {
            onViewportChange(nextViewport)
        }
    }
    LaunchedEffect(uiSession, viewport, surfaceSize, mapLayerState.vectors.visible, mapLayerState.metars.visible, mapLayerState.offlineRegions.visible, devServerBaseUrl) {
        if (surfaceSize.width <= 0 || surfaceSize.height <= 0) {
            mapOverlayError = null
            return@LaunchedEffect
        }
        val overlayWidthPx = surfaceSize.width.toFloat()
        val overlayHeightPx = surfaceSize.height.toFloat()
        if (!mapLayerState.vectors.visible && !mapLayerState.metars.visible && !mapLayerState.offlineRegions.visible) {
            committedMapOverlay = MapOverlayQueryResult(
                neededPointTiles = emptyList(),
                neededMetarTiles = emptyList(),
                neededAirspaceRefTiles = emptyList(),
                neededAirspaceFeatures = emptyList(),
                neededAirspaceLabelTiles = emptyList(),
                neededMetars = false,
                neededTfrs = false,
                visibleFeatures = emptyList(),
                visibleMetars = emptyList(),
                visiblePireps = emptyList(),
                airspacePaths = emptyList(),
                tfrPaths = emptyList(),
                airspaceLabels = emptyList(),
                offlineRegions = emptyList(),
                warnings = emptyList(),
            )
            committedOverlayViewport = viewport
            committedOverlaySurfaceUnits = OverlaySurfaceUnits(overlayWidthPx, overlayHeightPx)
            mapOverlayError = null
            return@LaunchedEffect
        }
        val overlay = try {
            currentCoroutineContext().ensureActive()
            val overlayStartMs = SystemClock.elapsedRealtime()
            val overlay = withContext(Dispatchers.IO) {
                uiSession.queryMapOverlay(viewport, overlayWidthPx.toDouble(), overlayHeightPx.toDouble()) { resource ->
                    fetchResourceBytes(resolvePlaybackTraceUrl(resource.address, devServerBaseUrl))
                }
            }
            currentCoroutineContext().ensureActive()
            val (centerLat, centerLon) = viewportCenterLatLon(viewport)
            Log.i(
                MapLayerLogTag,
                "overlay center=${"%.3f".format(centerLat)},${"%.3f".format(centerLon)} zoom=${"%.2f".format(viewport.zoom)} size=${surfaceSize.width}x${surfaceSize.height} vectorsVisible=${mapLayerState.vectors.visible} metarsVisible=${mapLayerState.metars.visible} offlineRegionsVisible=${mapLayerState.offlineRegions.visible} neededMetars=${overlay.neededMetars} features=${overlay.visibleFeatures.size} airspace=${overlay.airspacePaths.size} airspaceLabels=${overlay.airspaceLabels.size} offlineRegions=${overlay.offlineRegions.size} metars=${overlay.visibleMetars.size} pireps=${overlay.visiblePireps.size} neededPoints=${overlay.neededPointTiles.size} neededAirspaceRefs=${overlay.neededAirspaceRefTiles.size} neededAirspaceFeatures=${overlay.neededAirspaceFeatures.size} neededAirspaceLabels=${overlay.neededAirspaceLabelTiles.size} warnings=${overlay.warnings.size} elapsedMs=${SystemClock.elapsedRealtime() - overlayStartMs}",
            )
            overlay
        } catch (error: CancellationException) {
            mapOverlayError = null
            throw error
        } catch (error: Throwable) {
            mapOverlayError = error.message ?: error::class.java.simpleName
            Log.e(MapLayerLogTag, "overlay failed: $mapOverlayError", error)
            return@LaunchedEffect
        }
        committedMapOverlay = overlay
        committedOverlayViewport = viewport
        committedOverlaySurfaceUnits = OverlaySurfaceUnits(overlayWidthPx, overlayHeightPx)
        mapOverlayError = null
    }
    LaunchedEffect(uiSession, mapLayerState.nexrad.visible, mapLayerState.nexrad.enabled, devServerBaseUrl) {
        val effectStartMs = SystemClock.elapsedRealtime()
        if (!mapLayerState.nexrad.visible || !mapLayerState.nexrad.enabled) {
            nexradFrameIndex = 0
            Log.i(MapLayerLogTag, "nexrad hidden cachedFrames=${nexradFrames.size} elapsedMs=${SystemClock.elapsedRealtime() - effectStartMs}")
            return@LaunchedEffect
        }
        if (nexradFrameSourceUrl == devServerBaseUrl && nexradFrames.isNotEmpty()) {
            Log.i(MapLayerLogTag, "nexrad cached frames=${nexradFrames.size} elapsedMs=${SystemClock.elapsedRealtime() - effectStartMs}")
            return@LaunchedEffect
        }
        runCatching {
            var manifestBytes = 0
            var imageBytes = 0L
            var fetchMs = 0L
            var decodeMs = 0L
            val manifestJson = withContext(Dispatchers.IO) {
                val fetchStartMs = SystemClock.elapsedRealtime()
                URL(resolvePlaybackTraceUrl("/fast-products/nexrad/nexrad.json", devServerBaseUrl)).readText()
                    .also {
                        fetchMs += SystemClock.elapsedRealtime() - fetchStartMs
                        manifestBytes = it.length
                    }
            }
            val manifest = json.decodeFromString<NexradManifest>(manifestJson)
            require(manifest.projection == "EPSG:3857") { "unsupported nexrad projection ${manifest.projection}" }
            val frames = withContext(Dispatchers.IO) {
                manifest.frames.reversed().map { frame ->
                    val fetchStartMs = SystemClock.elapsedRealtime()
                    val bytes = URL(resolvePlaybackTraceUrl("/fast-products/nexrad/${frame.filename}", devServerBaseUrl)).openStream().buffered().use { it.readBytes() }
                    fetchMs += SystemClock.elapsedRealtime() - fetchStartMs
                    imageBytes += bytes.size
                    val decodeStartMs = SystemClock.elapsedRealtime()
                    val bitmap = BitmapFactory.decodeByteArray(bytes, 0, bytes.size)
                        ?: error("failed to decode nexrad frame ${frame.filename}")
                    decodeMs += SystemClock.elapsedRealtime() - decodeStartMs
                    NexradOverlayFrame(frame = frame, bitmap = bitmap.asImageBitmap())
                }
            }
            Log.i(
                MapLayerLogTag,
                "nexrad loaded frames=${frames.size} manifestBytes=$manifestBytes imageBytes=$imageBytes fetchMs=$fetchMs decodeMs=$decodeMs elapsedMs=${SystemClock.elapsedRealtime() - effectStartMs}",
            )
            frames
        }.onSuccess { frames ->
            nexradFrames = frames
            nexradFrameSourceUrl = devServerBaseUrl
            nexradFrameIndex = 0
        }.onFailure { error ->
            nexradFrames = emptyList()
            nexradFrameSourceUrl = null
            nexradFrameIndex = 0
            Log.w("AerobagLayers", "nexrad unavailable", error)
        }
    }
    LaunchedEffect(nexradFrames) {
        if (nexradFrames.size <= 1) {
            nexradFrameIndex = 0
            return@LaunchedEffect
        }
        while (true) {
            delay(NexradFrameIntervalMs)
            nexradFrameIndex = (nexradFrameIndex + 1) % nexradFrames.size
        }
    }
    LaunchedEffect(uiSession, viewport, surfaceSize, mapLayerState.terrainWarning.visible, devServerBaseUrl) {
        val effectStartMs = SystemClock.elapsedRealtime()
        if (surfaceSize.width <= 0 || surfaceSize.height <= 0) {
            terrainOverlay = emptyList()
            terrainOverlayError = null
            Log.i(MapLayerLogTag, "terrain skipped reason=empty-surface elapsedMs=${SystemClock.elapsedRealtime() - effectStartMs}")
            return@LaunchedEffect
        }
        if (!mapLayerState.terrainWarning.visible) {
            terrainOverlay = emptyList()
            terrainOverlayError = null
            Log.i(MapLayerLogTag, "terrain disabled elapsedMs=${SystemClock.elapsedRealtime() - effectStartMs}")
            return@LaunchedEffect
        }
        runCatching {
            var queryMs = 0L
            var fetchMs = 0L
            var renderMs = 0L
            var parseMs = 0L
            var sourceBytesTotal = 0L
            var rawBytesTotal = 0L
            var requestCount = 0
            val query = uiSession.queryTerrainOverlay(viewport, surfaceWidthPx.toDouble(), surfaceHeightPx.toDouble())
                .also { queryMs = SystemClock.elapsedRealtime() - effectStartMs }
            if (query.status !is net.jonh.aerobag.prototype.domain.TerrainOverlayStatus.Ready) {
                Log.i(
                    MapLayerLogTag,
                    "terrain not-ready status=${query.status::class.java.simpleName} queryMs=$queryMs elapsedMs=${SystemClock.elapsedRealtime() - effectStartMs}",
                )
                emptyList()
            } else {
                requestCount = query.tileRequests.size
                val images = withContext(Dispatchers.IO) {
                    query.tileRequests.map { request ->
                        val sourceTiles =
                            if (request.sourceTiles.isNotEmpty()) request.sourceTiles
                            else listOf(net.jonh.aerobag.prototype.domain.TerrainOverlaySourceTile(request.productId, request.path))
                        val sourceBytes =
                            sourceTiles.mapNotNull { sourceTile ->
                                runCatching {
                                    val fetchStartMs = SystemClock.elapsedRealtime()
                                    URL(resolvePlaybackTraceUrl("/terrain-products/${sourceTile.productId}/${sourceTile.path}", devServerBaseUrl))
                                        .openStream()
                                        .buffered()
                                        .use { it.readBytes() }
                                        .also {
                                            fetchMs += SystemClock.elapsedRealtime() - fetchStartMs
                                            sourceBytesTotal += it.size
                                        }
                                }.getOrNull()
                            }
                        if (sourceBytes.isEmpty()) {
                            return@map null
                        }
                        val renderStartMs = SystemClock.elapsedRealtime()
                        val rawBytes =
                            if (sourceBytes.size == 1) {
                                uiSession.renderTerrainOverlayTile(sourceBytes.first(), Double.NaN)
                            } else {
                                uiSession.renderTerrainOverlayTiles(packTerrainTileBytes(sourceBytes), Double.NaN)
                            }
                        renderMs += SystemClock.elapsedRealtime() - renderStartMs
                        rawBytesTotal += rawBytes.size
                        val parseStartMs = SystemClock.elapsedRealtime()
                        val bitmap = parseTerrainRawRgba(rawBytes)
                        parseMs += SystemClock.elapsedRealtime() - parseStartMs
                        TerrainOverlayImage(
                            key = request.key,
                            z = request.z,
                            x = request.x,
                            yTms = request.yTms,
                            left = request.left,
                            top = request.top,
                            size = request.size,
                            bitmap = bitmap,
                        )
                    }.filterNotNull()
                }
                Log.i(
                    MapLayerLogTag,
                    "terrain loaded requests=$requestCount images=${images.size} sourceBytes=$sourceBytesTotal rawBytes=$rawBytesTotal queryMs=$queryMs fetchMs=$fetchMs renderMs=$renderMs parseMs=$parseMs elapsedMs=${SystemClock.elapsedRealtime() - effectStartMs}",
                )
                images
            }
        }.onSuccess { images ->
            terrainOverlay = images
            terrainOverlayError = null
        }.onFailure { error ->
            terrainOverlay = emptyList()
            terrainOverlayError = error.message ?: error::class.java.simpleName
            Log.w("AerobagLayers", "terrain overlay unavailable", error)
        }
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
    LaunchedEffect(currentViewport, surfaceWidthPx, surfaceHeightPx, tiles, nexradFrames, nexradFrameIndex, terrainOverlay) {
        if (surfaceWidthPx <= 0f || surfaceHeightPx <= 0f) return@LaunchedEffect
        val topLeftWorld = screenToWorldOffset(currentViewport, 0f, 0f, surfaceWidthPx, surfaceHeightPx)
        val bottomRightWorld = screenToWorldOffset(currentViewport, surfaceWidthPx, surfaceHeightPx, surfaceWidthPx, surfaceHeightPx)
        val sampleTile = tiles.firstOrNull()
        val sampleTerrain = terrainOverlay.firstOrNull()
        val sampleNexrad = nexradFrames.getOrNull(nexradFrameIndex)
        val nexradMessage =
            if (sampleNexrad == null) {
                "nexrad=none"
            } else {
                val nwWorld = mercatorMetersToWorld(sampleNexrad.frame.bounds.west, sampleNexrad.frame.bounds.north)
                val seWorld = mercatorMetersToWorld(sampleNexrad.frame.bounds.east, sampleNexrad.frame.bounds.south)
                val nwScreen = worldToScreen(currentViewport, nwWorld, surfaceWidthPx, surfaceHeightPx)
                val seScreen = worldToScreen(currentViewport, seWorld, surfaceWidthPx, surfaceHeightPx)
                "nexrad=nwWorld=${"%.3f".format(nwWorld.x)},${"%.3f".format(nwWorld.y)} seWorld=${"%.3f".format(seWorld.x)},${"%.3f".format(seWorld.y)} nwScreen=${"%.1f".format(nwScreen.x)},${"%.1f".format(nwScreen.y)} seScreen=${"%.1f".format(seScreen.x)},${"%.1f".format(seScreen.y)}"
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
        Log.i(
            MapLayerLogTag,
            "viewport zoom=${"%.2f".format(currentViewport.zoom)} center=${"%.3f".format(currentViewport.centerWorldX)},${"%.3f".format(currentViewport.centerWorldY)} worldTL=${"%.3f".format(topLeftWorld.x)},${"%.3f".format(topLeftWorld.y)} worldBR=${"%.3f".format(bottomRightWorld.x)},${"%.3f".format(bottomRightWorld.y)} $chartMessage $terrainMessage $nexradMessage",
        )
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
    fun mapInputBlockedAt(position: Offset): Boolean {
        if (topLeftTrayOpen) {
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
                Log.i(
                    MapViewportLogTag,
                    "key-zoom map=${selectedMap.id} delta=${"%.2f".format(delta)} base=${"%.2f".format(viewportState.value.zoom)}",
                )
                updateViewport(
                    zoomAroundPoint(
                        viewport = viewportState.value,
                        mapView = selectedMap.mapView,
                        anchor = ScreenPoint(surfaceWidthPx / 2f, surfaceHeightPx / 2f),
                        widthPx = surfaceWidthPx,
                        heightPx = surfaceHeightPx,
                        nextZoom = clampZoom(viewportState.value.zoom + delta, selectedMap.mapView),
                    ),
                    syncFollow = false,
                )
                syncFollowStateForViewport(viewportState.value)
                true
            }
            .focusable()
            .pointerInput(selectedMap.mapView, surfaceSize, topLeftTrayOpen, mapSelection, mapSelectionTrayBounds, mapSurfaceBounds) {
                if (surfaceWidthPx == 0f || surfaceHeightPx == 0f) {
                    return@pointerInput
                }
                awaitEachGesture {
                    var dragPointerId: PointerId? = null
                    var dragLastPosition: Offset? = null
                    var pinchSnapshot: net.jonh.aerobag.prototype.domain.PinchSnapshot? = null
                    var gestureViewport = viewportState.value
                    var movedViewportDuringGesture = false
                    var loggedGestureSeed = false
                    try {
                        while (true) {
                            val event = awaitPointerEvent()
                            val pressed = event.changes.filter { it.pressed && !it.isConsumed }
                            if (pressed.isEmpty()) break
                            if (pressed.any { mapInputBlockedAt(it.position) }) {
                                break
                            }
                            mapGestureActive = true
                            if (!loggedGestureSeed) {
                                Log.i(
                                    MapViewportLogTag,
                                    "gesture-start map=${selectedMap.id} seed=${"%.2f".format(viewportState.value.zoom)} local=${"%.2f".format(viewportState.value.zoom)} center=${"%.3f".format(viewportState.value.centerWorldX)},${"%.3f".format(viewportState.value.centerWorldY)}",
                                )
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
                                        mapView = selectedMap.mapView,
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
                        if (movedViewportDuringGesture) {
                            syncFollowStateForViewport(viewportState.value)
                        } else if (loggedGestureSeed && dragLastPosition != null) {
                            val point = dragLastPosition
                            val world = screenToWorld(
                                viewportState.value,
                                ScreenPoint(point.x, point.y),
                                surfaceWidthPx,
                                surfaceHeightPx,
                            )
                            val (lat, lon) = worldToLatLon(world.x, world.y)
                            runCatching {
                                uiSession.queryMapSelection(
                                    viewportState.value,
                                    surfaceWidthPx.toDouble(),
                                    surfaceHeightPx.toDouble(),
                                    LatLonPoint(lat = lat, lon = lon),
                                    with(density) { (ThumbSize * 0.5f).toPx().toDouble() },
                                )
                            }.onSuccess { result ->
                                mapSelection = MapSelectionUiState(point = point, result = result, selectedItem = null)
                                chartTrayOpen = false
                                layerTrayOpen = false
                            }.onFailure { error ->
                                Log.w("AerobagSelection", "map selection failed", error)
                            }
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
                            mapView = selectedMap.mapView,
                            anchor = ScreenPoint(surfaceWidthPx / 2f, surfaceHeightPx / 2f),
                            widthPx = surfaceWidthPx,
                            heightPx = surfaceHeightPx,
                            nextZoom = clampZoom(viewportState.value.zoom - wheelDelta * 0.28, selectedMap.mapView),
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
                } else {
                    val fallback = findParentTileFallback(tile, decodedTileBitmapCache)
                    if (fallback != null) {
                        val factor = 1 shl fallback.sourceLevelDelta
                        val sourceWidth = max(1, fallback.bitmap.width / factor)
                        val sourceHeight = max(1, fallback.bitmap.height / factor)
                        drawImage(
                            image = fallback.bitmap,
                            srcOffset = IntOffset(
                                fallback.sourceColumn * sourceWidth,
                                fallback.sourceRow * sourceHeight,
                            ),
                            srcSize = IntSize(sourceWidth, sourceHeight),
                            dstOffset = IntOffset(tileRect.leftPx, tileRect.topPx),
                            dstSize = IntSize(tileRect.widthPx, tileRect.heightPx),
                        )
                    } else {
                        findChildTileFallbacks(tile, decodedTileBitmapCache).forEach { child ->
                            val factor = 1 shl child.targetLevelDelta
                            val childLeft = tileRect.leftPx + tileRect.widthPx * child.targetColumn / factor
                            val childTop = tileRect.topPx + tileRect.heightPx * child.targetRow / factor
                            val childRight = tileRect.leftPx + tileRect.widthPx * (child.targetColumn + 1) / factor
                            val childBottom = tileRect.topPx + tileRect.heightPx * (child.targetRow + 1) / factor
                            drawImage(
                                image = child.bitmap,
                                dstOffset = IntOffset(childLeft, childTop),
                                dstSize = IntSize(max(1, childRight - childLeft), max(1, childBottom - childTop)),
                            )
                        }
                    }
                }
                if (debugState.tileLabels) {
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
            val currentNexradFrame =
                if (mapLayerState.nexrad.visible) nexradFrames.getOrNull(nexradFrameIndex) else null
            if (currentNexradFrame != null && surfaceWidthPx > 0f && surfaceHeightPx > 0f) {
                val northwestWorld = mercatorMetersToWorld(currentNexradFrame.frame.bounds.west, currentNexradFrame.frame.bounds.north)
                val southeastWorld = mercatorMetersToWorld(currentNexradFrame.frame.bounds.east, currentNexradFrame.frame.bounds.south)
                val northwest = worldToScreen(currentViewport, northwestWorld, surfaceWidthPx, surfaceHeightPx)
                val southeast = worldToScreen(currentViewport, southeastWorld, surfaceWidthPx, surfaceHeightPx)
                val widthPx = (southeast.x - northwest.x).roundToInt().coerceAtLeast(1)
                val heightPx = (southeast.y - northwest.y).roundToInt().coerceAtLeast(1)
                drawImage(
                    image = currentNexradFrame.bitmap,
                    dstOffset = IntOffset(northwest.x.roundToInt(), northwest.y.roundToInt()),
                    dstSize = IntSize(widthPx, heightPx),
                    alpha = 0.82f,
                )
            }
            terrainOverlay.forEach { image ->
                val tilesAtZoom = 2.0.pow(image.z.toDouble())
                val tileWorldSize = WebMercatorWorldSize / tilesAtZoom
                val yXyz = (tilesAtZoom - 1.0) - image.yTms.toDouble()
                val scale = scaleForZoom(currentViewport.zoom)
                val leftPx = ((image.x * tileWorldSize - currentViewport.centerWorldX) * scale + surfaceWidthPx / 2f).roundToInt()
                val topPx = ((yXyz * tileWorldSize - currentViewport.centerWorldY) * scale + surfaceHeightPx / 2f).roundToInt()
                val sizePx = (tileWorldSize * scale).roundToInt().coerceAtLeast(1)
                drawImage(
                    image = image.bitmap,
                    dstOffset = IntOffset(leftPx, topPx),
                    dstSize = IntSize(sizePx, sizePx),
                    alpha = 0.68f,
                )
            }
        }
        if (displayedMapOverlay.airspacePaths.isNotEmpty() || displayedMapOverlay.tfrPaths.isNotEmpty() || displayedMapOverlay.airspaceLabels.isNotEmpty()) {
            Canvas(modifier = Modifier.fillMaxSize()) {
                (displayedMapOverlay.airspacePaths + displayedMapOverlay.tfrPaths).forEach { feature ->
                    drawAirspaceDisplayPath(uiTheme, feature)
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
        if (displayedMapOverlay.visibleFeatures.isNotEmpty()) {
            Canvas(modifier = Modifier.fillMaxSize()) {
                val densityScale = density.density
                fixLabelStrokePaint.textSize = 14f * densityScale
                fixLabelStrokePaint.strokeWidth = 3f * densityScale
                airportLabelStrokePaint.textSize = 14f * densityScale
                airportLabelStrokePaint.strokeWidth = 3f * densityScale
                fixLabelFillPaint.textSize = 14f * densityScale
                airportToweredLabelFillPaint.textSize = 14f * densityScale
                airportUntoweredLabelFillPaint.textSize = 14f * densityScale
                vorLabelFillPaint.textSize = 14f * densityScale
                displayedMapOverlay.visibleFeatures.forEach { feature ->
                    val center = Offset(feature.screenX.toFloat(), feature.screenY.toFloat())
                    val isAirport = feature.styleClass == "airport" || feature.kind.equals("airport", ignoreCase = true)
                    val isVor = feature.styleClass == "nav" || feature.kind.lowercase().contains("vor")
                    val isObstacle =
                        feature.styleClass.startsWith("obstacle") ||
                            feature.kind.equals("obs", ignoreCase = true) ||
                            feature.kind.equals("obstacle", ignoreCase = true)
                    if (isAirport) {
                        val airportFillColor = if (feature.towered) airportToweredFillColor else airportUntoweredFillColor
                        val airportLabelPaint = if (feature.towered) airportToweredLabelFillPaint else airportUntoweredLabelFillPaint
                        val usesOpenAirportCircle =
                            feature.heliport == true ||
                                feature.hasWaterRunway == true ||
                                feature.hasPavedRunway == false
                        if (usesOpenAirportCircle) {
                            airportOpenMarkerSymbol(center, densityScale).forEach { layer ->
                                drawNavSymbolLayer(layer, densityScale, uiTheme)
                            }
                        } else if (feature.fuelAvailable) {
                            val markerPath = airportFuelMarkerPath(center, densityScale)
                            drawPath(markerPath, airportFillColor)
                            drawPath(markerPath, airportMarkerStrokeColor, style = Stroke(width = 2f * densityScale))
                        } else {
                            val markerPath = airportCircleMarkerPath(center, densityScale)
                            drawPath(markerPath, airportFillColor)
                            drawPath(markerPath, airportMarkerStrokeColor, style = Stroke(width = 2f * densityScale))
                        }
                        if (feature.heliport == true) {
                            val heliportPath = heliportHPath(center, densityScale)
                            drawPath(
                                heliportPath,
                                airportUntoweredFillColor,
                                style = Stroke(width = 2.4f * densityScale, cap = StrokeCap.Round),
                            )
                        } else if (feature.hasWaterRunway == true) {
                            rotate(15f, center) {
                                val anchorPath = seaplaneAnchorPath(center, densityScale)
                                drawPath(
                                    anchorPath,
                                    airportUntoweredFillColor,
                                    style = Stroke(width = 2.2f * densityScale, cap = StrokeCap.Round),
                                )
                            }
                        }
                        if (!usesOpenAirportCircle) feature.longestRunwayHeadingTrueDeg?.let { headingDeg ->
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
                        drawContext.canvas.nativeCanvas.apply {
                            val textX = center.x
                            val textY = center.y - 24f * densityScale
                            drawText(feature.label, textX, textY, airportLabelStrokePaint)
                            drawText(feature.label, textX, textY, airportLabelPaint)
                        }
                    } else if (isVor) {
                        val radius = 8f * densityScale
                        val outerHex = vorOuterHexPath(center, radius)
                        val band = vorBandPath(center, radius)
                        drawPath(band, vorMarkerColor)
                        drawPath(band, vorMarkerStrokeColor, style = Stroke(width = 1.6f * densityScale))
                        drawPath(outerHex, vorMarkerStrokeColor, style = Stroke(width = 1.6f * densityScale))
                        drawContext.canvas.nativeCanvas.apply {
                            val textY = center.y - 24f * densityScale
                            drawText(feature.label, center.x, textY, fixLabelStrokePaint)
                            drawText(feature.label, center.x, textY, vorLabelFillPaint)
                        }
                    } else if (isObstacle) {
                        val isTallObstacle = feature.obstacleVariant == "tall"
                        val obstaclePath = if (isTallObstacle) {
                            obstacleTallPath(center, densityScale)
                        } else {
                            obstacleShortPath(center, densityScale)
                        }
                        val dotY = if (isTallObstacle) obstacleTallDotY else obstacleShortDotY
                        val obstacleColor = when (feature.styleClass) {
                            "obstacle-danger" -> Color(0xFFD83A2E)
                            "obstacle-muted" -> Color(0xB8FFD34D)
                            else -> Color(0xFFFFD34D)
                        }
                        val obstacleUnderColor = Color(0xD1081218)
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
                        if (feature.label.isNotEmpty()) {
                            drawContext.canvas.nativeCanvas.apply {
                                val textY = center.y - 14f * densityScale
                                drawText(feature.label, center.x, textY, fixLabelStrokePaint)
                                drawText(feature.label, center.x, textY, fixLabelFillPaint)
                            }
                        }
                    } else {
                        val triangle = fixTrianglePath(center, 8f * densityScale)
                        drawPath(triangle, fixMarkerFillColor)
                        drawPath(triangle, fixMarkerStrokeColor, style = Stroke(width = 2.5f * densityScale))
                        drawContext.canvas.nativeCanvas.apply {
                            val textY = center.y - 15f * densityScale
                            drawText(feature.label, center.x, textY, fixLabelStrokePaint)
                            drawText(feature.label, center.x, textY, fixLabelFillPaint)
                        }
                    }
                }
            }
            displayedMapOverlay.visibleFeatures.forEach { feature ->
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
        if (displayedMapOverlay.visibleMetars.isNotEmpty()) {
            Canvas(modifier = Modifier.fillMaxSize()) {
                displayedMapOverlay.visibleMetars.forEach { feature ->
                    drawMetarSymbol(feature, Offset(feature.screenX.toFloat(), feature.screenY.toFloat()), density.density, uiTheme)
                }
            }
        }
        if (displayedMapOverlay.visiblePireps.isNotEmpty()) {
            Canvas(modifier = Modifier.fillMaxSize()) {
                displayedMapOverlay.visiblePireps.forEach { feature ->
                    drawPirepSymbol(feature, Offset(feature.screenX.toFloat(), feature.screenY.toFloat()), density.density, uiTheme, symbolScale = 0.32f)
                }
            }
        }
        if (displayedMapOverlay.offlineRegions.isNotEmpty()) {
            Canvas(modifier = Modifier.fillMaxSize()) {
                val densityScale = density.density
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
                    val path = Path().apply {
                        val first = region.points.firstOrNull() ?: return@forEach
                        moveTo(first.x.toFloat(), first.y.toFloat())
                        region.points.drop(1).forEach { point -> lineTo(point.x.toFloat(), point.y.toFloat()) }
                        close()
                    }
                    val color = aviationColor(uiTheme, region.colorKey)
                    drawPath(
                        path,
                        Color.White.copy(alpha = 0.8f),
                        style = Stroke(width = 5f * densityScale, join = StrokeJoin.Round),
                    )
                    drawPath(
                        path,
                        color,
                        style = Stroke(width = 2.5f * densityScale, join = StrokeJoin.Round),
                    )
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
        if (routeScreenSegments.isNotEmpty()) {
            Canvas(modifier = Modifier.fillMaxSize()) {
                val densityScale = density.density
                routeScreenSegments.forEach { (path, segment) ->
                    path.zipWithNext().forEach { (from, to) ->
                        drawLine(
                            color = Color(0x8C000000),
                            start = from,
                            end = to,
                            strokeWidth = 7f * densityScale,
                            cap = StrokeCap.Round,
                        )
                        drawLine(
                            color = routeSegmentColor(segment.status),
                            start = from,
                            end = to,
                            strokeWidth = 3.5f * densityScale,
                            cap = StrokeCap.Round,
                        )
                    }
                }
            }
        }
        mapSelection?.selectedItem?.let { item ->
            Canvas(modifier = Modifier.fillMaxSize()) {
                when (val highlight = item.highlight) {
                    is MapSelectionHighlight.FeatureRef -> {
                        val feature = displayedMapOverlay.visibleFeatures.firstOrNull { it.id == highlight.id }
                        if (feature != null) {
                            drawCircle(Color.White, radius = 20f * density.density, center = Offset(feature.screenX.toFloat(), feature.screenY.toFloat()), style = Stroke(width = 4f * density.density))
                        }
                        (displayedMapOverlay.airspacePaths + displayedMapOverlay.tfrPaths).firstOrNull { it.id == highlight.id }?.let { path ->
                            drawAirspaceDisplayPath(uiTheme, path)
                        }
                    }
                    is MapSelectionHighlight.Metar -> {
                        val feature = displayedMapOverlay.visibleMetars.firstOrNull { it.stationId == highlight.stationId } ?: item.metarFeature
                        if (feature != null) {
                            drawCircle(Color.White, radius = 16f * density.density, center = Offset(feature.screenX.toFloat(), feature.screenY.toFloat()), style = Stroke(width = 4f * density.density))
                            drawMetarSymbol(feature, Offset(feature.screenX.toFloat(), feature.screenY.toFloat()), density.density, uiTheme)
                        }
                    }
                    is MapSelectionHighlight.Pirep -> {
                        val feature = displayedMapOverlay.visiblePireps.firstOrNull { it.id == highlight.id } ?: item.pirepFeature
                        if (feature != null) {
                            val center = Offset(feature.screenX.toFloat(), feature.screenY.toFloat())
                            drawCircle(Color.White, radius = 25f * density.density, center = center, style = Stroke(width = 4f * density.density))
                            drawPirepSymbol(feature, center, density.density, uiTheme, symbolScale = 0.32f)
                        }
                    }
                    is MapSelectionHighlight.Spot -> {
                        val point = latLonToScreen(highlight.lat, highlight.lon, currentViewport, surfaceWidthPx, surfaceHeightPx)
                        drawMapSelectionSpotSymbol(point, density.density, uiTheme)
                    }
                }
            }
        }
        if (situationOverlay != null) {
            Canvas(modifier = Modifier.fillMaxSize()) {
                val densityScale = density.density
                val center = situationOverlay.pointUnits
                val ringRadius = situationOverlay.ring.radiusUnits
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
                situationOverlay.ring.tickMarks.forEach { tick ->
                    val inner = tick.innerUnits
                    val outer = tick.outerUnits
                    drawLine(Color(0x66000000), inner, outer, strokeWidth = 8f)
                    drawLine(Color.White, inner, outer, strokeWidth = 6f)
                }
                drawContext.canvas.nativeCanvas.apply {
                    labelStrokePaint.textSize = 16f * densityScale
                    labelFillPaint.textSize = 16f * densityScale
                    situationOverlay.ring.cardinalLabels.forEach { label ->
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
                    val labelPoint = situationOverlay.ring.labelPointUnits
                    save()
                    rotate(situationOverlay.ring.labelRotationDeg, labelPoint.x, labelPoint.y)
                    labelStrokePaint.textSize = 16f * densityScale
                    labelFillPaint.textSize = 16f * densityScale
                    drawText(situationOverlay.ring.labelText, labelPoint.x, labelPoint.y + labelFillPaint.textSize * 0.33f, labelStrokePaint)
                    drawText(situationOverlay.ring.labelText, labelPoint.x, labelPoint.y + labelFillPaint.textSize * 0.33f, labelFillPaint)
                    restore()
                    val iconSizePx = ThumbSize.toPx() * 0.72f
                    val left = (center.x - iconSizePx / 2f).roundToInt()
                    val top = (center.y - iconSizePx / 2f).roundToInt()
                    val drawable = aircraftDrawable
                    if (drawable != null) {
                        save()
                        rotate(situationOverlay.headingDeg, center.x, center.y)
                        drawable.setBounds(left, top, (left + iconSizePx).roundToInt(), (top + iconSizePx).roundToInt())
                        drawable.draw(this)
                        restore()
                    }
                }
            }
        }
        SituationStatusBadge(
            controls = ownshipControls,
            modifier = Modifier
                .align(Alignment.TopEnd)
                .padding(top = situationDockTopPadding, end = ThumbGap),
            onSelectSource = onSelectOwnshipSource,
            onSituationControlInput = onSituationControlInput,
        )

        MapTopLeftControls(
            modifier = Modifier.align(Alignment.TopStart),
            currentPage = page,
            onSelectPage = {
                onSelectPage(it)
                chartTrayOpen = false
                layerTrayOpen = false
            },
            selectedLabel = selectedLauncher.launcherLabel,
            trayOptions = trayOptions,
            trayOpen = chartTrayOpen,
            onToggle = {
                chartTrayOpen = !chartTrayOpen
                layerTrayOpen = false
            },
            layerTrayOpen = layerTrayOpen,
            onToggleLayerTray = {
                layerTrayOpen = !layerTrayOpen
                chartTrayOpen = false
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
            onChartSearchSuggestionClick = { suggestion -> recenterOnNavRef(suggestion.navRef) },
        )

        val playbackLeftRoomUnits = surfaceWidthDp / 2f - (ThumbSize.value * 1.5f) - (ThumbGap.value * 2f)
        val playbackBottomPadding =
            if (playbackLeftRoomUnits < ThumbSize.value * 2.8f) {
                ThumbGap + (ThumbSize * 0.67f) + ThumbGap
            } else {
                ThumbGap
            }
        if (debugState.playbackVisible) {
            PlaybackWidget(
                uiSession = uiSession,
                playbackUiState = playbackUiState,
                sourcePath = playbackSourcePath,
                onSourcePathChange = onPlaybackSourcePathChange,
                onSnapshotChange = onSessionSnapshotChange,
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
            enabled = mapFollowUiState.canCenterHere,
            selected = mapFollowUiState.following,
            selectedColor = Color(0xFF0D6F67),
            onClick = {
                runCatching { uiSession.engageMapFollow(viewport) }.onSuccess(onSessionSnapshotChange)
            },
        )

        if (topLeftTrayOpen) {
            Scrim {
                chartTrayOpen = false
                layerTrayOpen = false
            }
        }

        mapSelection?.let { selection ->
            Scrim(modifier = Modifier.zIndex(OverlayPlaneModalScrim)) { mapSelection = null }
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
                    mapSelection = selection.copy(selectedItem = item)
                },
                onSelectAction = { item, action ->
                    action.flightPlanRowAction?.let { rowAction ->
                        runCatching { uiSession.performFlightPlanRowAction(rowAction.rowUid, rowAction.actionUid) }
                            .onSuccess(onSessionSnapshotChange)
                            .onFailure { Log.w("AerobagSelection", "flight-plan row action failed", it) }
                        mapSelection = null
                        return@MapSelectionTray
                    }
                    action.sessionAction?.let { sessionAction ->
                        runCatching { uiSession.performMapSelectionAction(sessionAction) }
                            .onSuccess(onSessionSnapshotChange)
                            .onFailure { Log.w("AerobagSelection", "map selection action failed", it) }
                        mapSelection = null
                        return@MapSelectionTray
                    }
                    when (action.id) {
                        "plates", "csup" -> {
                            val airportId = (item.navRef as? NavRef.Airport)?.code
                            if (airportId != null) {
                                val target = if (action.id == "csup") "CSup" else "Folder"
                                val snapshot = uiSession.selectAirport(airportId)
                                onSessionSnapshotChange(snapshot)
                                runCatching { uiSession.selectChart("Plate:$airportId:$target") }
                                    .onSuccess(onSessionSnapshotChange)
                                onSelectPage(AppPage.Charts)
                                mapSelection = null
                            }
                        }
                    }
                },
            )
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
    val visibleActions = if (selectedItem?.detailText != null) actionSlots.take(3) else actionSlots.take(6)
    Surface(
        modifier = modifier
            .testTag("parity:map-selection-tray")
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
                    Text(
                        text = buildAnnotatedString {
                            if (selectedItem != null) {
                                withStyle(SpanStyle(fontWeight = FontWeight.Bold)) { append(selectedItem.label) }
                                selectedItem.description?.takeIf { it.isNotBlank() }?.let { append(" · $it") }
                            } else {
                                append(" ")
                            }
                        },
                        style = MaterialTheme.typography.labelMedium,
                        color = uiTheme.controls.panelFg,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                    )
                    Column(verticalArrangement = Arrangement.spacedBy(ThumbGap * 0.45f)) {
                        visibleActions.chunked(3).forEach { rowActions ->
                            Row(horizontalArrangement = Arrangement.spacedBy(ThumbGap * 0.45f)) {
                                rowActions.forEach { action ->
                                    MapSelectionActionButton(
                                        action = action,
                                        enabled = action.enabled && !action.displayOnly,
                                        onClick = {
                                            if (selectedItem != null) onSelectAction(selectedItem, action)
                                        },
                                    )
                                }
                            }
                        }
                    }
                    selectedItem?.detailText?.let { detail ->
                        Text(
                            text = detail,
                            style = MaterialTheme.typography.labelMedium.copy(fontSize = 14.sp),
                            color = uiTheme.controls.panelFg,
                            maxLines = 2,
                            overflow = TextOverflow.Ellipsis,
                        )
                    }
                }
            }
        }
    }
}

@Composable
internal fun MapSelectionItemButton(
    item: MapSelectionItem,
    selected: Boolean,
    testTag: String,
    onClick: () -> Unit,
) {
    val uiTheme = LocalAerobagUiTheme.current
    Surface(
        modifier = Modifier
            .size(ThumbSize)
            .testTag(testTag)
            .clickable(onClick = onClick),
        shape = RoundedCornerShape(ThumbRadius),
        color = if (selected) lerp(uiTheme.controls.buttonBg, Color.White, 0.28f) else uiTheme.controls.buttonBg,
        contentColor = uiTheme.controls.buttonFg,
        border = BorderStroke(
            if (selected) 2.dp else 1.dp,
            if (selected) uiTheme.controls.buttonFg else lerp(uiTheme.controls.buttonBg, Color.Black, 0.22f),
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
    "button_bg" -> uiTheme.controls.buttonBg
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
    onClick: () -> Unit,
) {
    val uiTheme = LocalAerobagUiTheme.current
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
            .then(if (enabled) Modifier.clickable(onClick = onClick) else Modifier),
        shape = RoundedCornerShape(ThumbRadius),
        color = when {
            action.displayOnly -> uiTheme.controls.mapSelectionDisplayBg
            enabled -> uiTheme.controls.buttonBg
            else -> uiTheme.controls.panelMuted
        },
        contentColor = if (action.displayOnly) uiTheme.controls.mapSelectionDisplayFg else uiTheme.controls.buttonFg,
        border = BorderStroke(
            1.dp,
            if (action.displayOnly) uiTheme.controls.panelBorder else lerp(uiTheme.controls.buttonBg, Color.Black, 0.22f),
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
