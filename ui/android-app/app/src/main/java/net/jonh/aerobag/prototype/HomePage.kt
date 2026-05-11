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
internal fun HomePage(
    page: AppPage,
    pageHistory: List<AppViewSnapshot>,
    mostRecentChartOrPlatePage: AppPage = AppPage.Map,
    uptimeLabel: String,
    bootstrap: net.jonh.aerobag.prototype.domain.BootstrapFixture,
    debugState: UiDebugState,
    navElement: NavElementUiView?,
    onSelectPage: (AppPage) -> Unit,
    onOpenPlan: () -> Unit,
    onOpenRecentChartOrPlate: () -> Unit = {},
    initialOfflinePackagesOpen: Boolean = false,
    offlinePackagesControllerHandle: Long,
    forceOfflinePackagesOpen: Boolean = false,
    bootstrapMessage: String? = null,
    onOfflinePackagesClosed: (() -> Unit)? = null,
    onRuntimeMaybeAvailable: (() -> Unit)? = null,
) {
    val uiTheme = LocalAerobagUiTheme.current
    val context = LocalContext.current
    val prefs = remember(context) { context.applicationContext.getSharedPreferences(UiPrefsName, Context.MODE_PRIVATE) }
    val coroutineScope = rememberCoroutineScope()
    val offlinePackagesControllerAlive = remember(offlinePackagesControllerHandle) { AtomicBoolean(true) }
    DisposableEffect(offlinePackagesControllerHandle) {
        onDispose { offlinePackagesControllerAlive.set(false) }
    }
    var packageSourceBaseUrl by remember(context, prefs) {
        mutableStateOf(readPackageSourceBaseUrl(context.applicationContext, prefs))
    }
    var offlinePackagesOpen by remember { mutableStateOf(forceOfflinePackagesOpen || initialOfflinePackagesOpen) }
    val regionOptions = remember { offlineRegionOptions() }
    val regionIds = remember(regionOptions) { regionOptions.map { it.id } }
    val productIds = remember { OfflineProductOptions.map { it.id } }
    var offlinePackagesControllerResult by remember { mutableStateOf<OfflinePackagesControllerResultWire?>(null) }
    var offlinePackageOperationJob by remember { mutableStateOf<Job?>(null) }
    var offlinePackageCancelRequested by remember { mutableStateOf(false) }
    var navDbStatusRefreshToken by remember { mutableIntStateOf(0) }
    val activePackageConnections = remember { ActivePackageConnections() }
    fun launchOfflinePackageOperation(block: suspend () -> Unit) {
        if (offlinePackageOperationJob?.isActive == true) {
            return
        }
        offlinePackageCancelRequested = false
        val job = coroutineScope.launch {
            try {
                block()
            } finally {
                activePackageConnections.disconnectAll()
                offlinePackageOperationJob = null
                offlinePackageCancelRequested = false
            }
        }
        offlinePackageOperationJob = job
    }
    suspend fun dispatchOfflinePackagesController(event: OfflinePackagesControllerEventWire) {
        if (!offlinePackagesControllerAlive.get()) {
            Log.i(
                "OfflinePackages",
                "dropping event=${event::class.simpleName} for disposed controller handle=$offlinePackagesControllerHandle",
            )
            return
        }
        val startMs = SystemClock.elapsedRealtime()
        Log.i(
            "OfflinePackages",
            "controller event=${event::class.simpleName} handle=$offlinePackagesControllerHandle scanning installed packages",
        )
        val installed = withContext(Dispatchers.IO) {
            listInstalledPackageArtifacts(context.applicationContext)
        }
        val installedScanElapsedMs = SystemClock.elapsedRealtime() - startMs
        val input = OfflinePackagesControllerInputWire(
            packageSourceBaseUrl = packageSourceBaseUrl,
            discoveryFilenames = emptyList(),
            regionIds = regionIds,
            productIds = productIds,
            nowEpochMs = bootstrap.packageManagementNowEpochMsOverride ?: System.currentTimeMillis(),
            installed = installed,
            event = event,
        )
        val inputJson = PackageManagementJson.encodeToString(input)
        Log.i(
            "OfflinePackages",
            "controller event=${event::class.simpleName} handle=$offlinePackagesControllerHandle " +
                "installed=${installed.size} installedScanMs=$installedScanElapsedMs inputBytes=${inputJson.length}",
        )
        if (!offlinePackagesControllerAlive.get()) {
            Log.i(
                "OfflinePackages",
                "dropping event=${event::class.simpleName} after package scan for disposed controller handle=$offlinePackagesControllerHandle",
            )
            return
        }
        val outputJson = try {
            NativeBindings.dispatchOfflinePackagesControllerJson(offlinePackagesControllerHandle, inputJson)
        } catch (error: RuntimeException) {
            if (error.message?.contains("invalid offline packages controller handle") == true) {
                Log.i(
                    "OfflinePackages",
                    "dropping event=${event::class.simpleName} for stale controller handle=$offlinePackagesControllerHandle",
                )
                return
            }
            throw error
        }
        val result = PackageManagementJson.decodeFromString<OfflinePackagesControllerResultWire>(outputJson)
        writeOfflinePackagesStateJson(prefs, result.packagesStateJson)
        offlinePackagesControllerResult = result
        when (val command = result.command) {
            is OfflinePackagesControllerCommandWire.RefreshLibrary -> {
                val refreshResult: Result<OfflinePackagesControllerEventWire.LibraryRefreshSucceeded> = runCatching {
                    withContext(Dispatchers.IO) {
                        refreshOfflinePackageLibrary(
                            packageSourceBaseUrl = command.packageSourceBaseUrl,
                            activeConnections = activePackageConnections,
                        )
                    }
                }
                val nextEvent = refreshResult.getOrNull()?.let<OfflinePackagesControllerEventWire.LibraryRefreshSucceeded, OfflinePackagesControllerEventWire> { refreshed ->
                    refreshed
                } ?: refreshResult.exceptionOrNull()?.let { error ->
                    val canceled = error is CancellationException || offlinePackageCancelRequested
                    Log.e("OfflinePackages", "library refresh failed", error)
                    OfflinePackagesControllerEventWire.LibraryRefreshFailed(
                        if (canceled) {
                            "package library refresh canceled"
                        } else {
                            error.message ?: error::class.simpleName ?: "offline packages library refresh failed"
                        },
                    )
                } ?: error("offline packages library refresh produced no result")
                withContext(NonCancellable) {
                    dispatchOfflinePackagesController(
                        nextEvent,
                    )
                }
                navDbStatusRefreshToken += 1
            }
            is OfflinePackagesControllerCommandWire.Sync -> {
                val summary = try {
                    withContext(Dispatchers.IO) {
                        syncOfflinePackages(
                            context = context.applicationContext,
                            plan = command.plan,
                            bundle = command.bundle,
                            packageSourceBaseUrl = command.packageSourceBaseUrl,
                            packagedArtifactRoot = command.packagedArtifactRoot,
                            maxParallelFetches = command.maxParallelFetches,
                            activeConnections = activePackageConnections,
                            onProgress = { message, progress ->
                                withContext(Dispatchers.Main) {
                                    if (progress != null) {
                                        dispatchOfflinePackagesController(
                                            OfflinePackagesControllerEventWire.SyncProgressObserved(progress),
                                        )
                                    }
                                }
                            },
                        )
                    }
                } catch (error: CancellationException) {
                    OfflinePackagesSyncSummary(
                        fetchedCount = 0,
                        gcCount = 0,
                        warnings = listOf(
                            OfflinePackagesWarning(
                                artifactId = "sync",
                                familyId = null,
                                regionId = null,
                                message = "sync canceled",
                            ),
                        ),
                    )
                }
                withContext(NonCancellable) {
                    dispatchOfflinePackagesController(
                        OfflinePackagesControllerEventWire.SyncFinished(summary = summary),
                    )
                }
                navDbStatusRefreshToken += 1
                onRuntimeMaybeAvailable?.invoke()
            }
            null -> Unit
        }
    }
    LaunchedEffect(forceOfflinePackagesOpen) {
        if (forceOfflinePackagesOpen) {
            offlinePackagesOpen = true
        }
    }
    LaunchedEffect(forceOfflinePackagesOpen, offlinePackagesOpen) {
        if (forceOfflinePackagesOpen || offlinePackagesOpen) {
            launchOfflinePackageOperation {
                dispatchOfflinePackagesController(OfflinePackagesControllerEventWire.EnsureLibrary)
            }
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
            onHomeClick = { onSelectPage(AppPage.Home) },
            onOpenChartOrPlate = onOpenRecentChartOrPlate,
        )

        LazyVerticalGrid(
            columns = GridCells.Fixed(3),
            modifier = Modifier
                .align(Alignment.TopStart)
                .padding(
                    start = ThumbGap + (ThumbSize * 0.5f),
                    top = ThumbSize + ThumbGap * 2f,
                )
                .width((ThumbSize * 6f) + (ThumbGap * 2f)),
            horizontalArrangement = Arrangement.spacedBy(ThumbGap),
            verticalArrangement = Arrangement.spacedBy(ThumbGap),
            userScrollEnabled = false,
        ) {
            lazyGridItems(HomeGridButtons, key = { it.key }) { button ->
                CompactSquareButton(
                    label = button.label,
                    modifier = Modifier
                        .width(ThumbSize * 2f)
                        .height(ThumbSize * 2f),
                    maxLines = 2,
                    enabled = button.enabled,
                    iconResId = button.iconResId,
                    wide = true,
                    onClick = {
                        Log.i("AerobagNavigation", "home button key=${button.key} target=${button.targetPage}")
                        if (button.targetPage != null) {
                            onSelectPage(button.targetPage)
                        } else if (button.key == "offline-packages") {
                            Log.i("AerobagNavigation", "offline packages open requested")
                            offlinePackagesOpen = true
                        }
                    },
                )
            }
        }

        NavElementDock(
            navElement = navElement,
            onClick = onOpenPlan,
            modifier = Modifier
                .align(Alignment.BottomCenter)
                .padding(bottom = ThumbGap),
        )

        if (offlinePackagesOpen || forceOfflinePackagesOpen) {
            if (!forceOfflinePackagesOpen) {
                Scrim { offlinePackagesOpen = false }
            }
            val controllerUiState = offlinePackagesControllerResult?.uiState
            val navDbStatus by produceState<net.jonh.aerobag.prototype.domain.NavDbStatus?>(initialValue = null, context, navDbStatusRefreshToken, offlinePackagesOpen, forceOfflinePackagesOpen) {
                if (!offlinePackagesOpen && !forceOfflinePackagesOpen) {
                    value = null
                    return@produceState
                }
                value = withContext(Dispatchers.IO) {
                    SampleData.inspectNavDbStatus(context.applicationContext)
                }
            }
            LaunchedEffect(navDbStatus) {
                val status = navDbStatus ?: return@LaunchedEffect
                dispatchOfflinePackagesController(
                    OfflinePackagesControllerEventWire.InstalledArtifactHealthObserved(
                        unreadableInstalledFilenameMessages = status.installed
                            .filter { !it.readable }
                            .associate { it.filename to (it.message ?: "unreadable") },
                    ),
                )
            }
            if (controllerUiState == null || (controllerUiState.plannerUiState == null && !controllerUiState.libraryLoaded)) {
                val packageOperationActive = offlinePackageOperationJob?.isActive == true
                val libraryRefreshInFlight = controllerUiState?.libraryLoading == true ||
                    (controllerUiState == null && packageOperationActive)
                val packageSourceEditable = controllerUiState?.packageSourceEditable ?: !packageOperationActive
                val libraryRefreshEnabled = controllerUiState?.refreshEnabled ?: !packageOperationActive
                val libraryRefreshCancelEnabled = controllerUiState?.refreshCancelEnabled ?: packageOperationActive
                OfflinePackagesLibraryPanel(
                    message = listOfNotNull(
                        bootstrapMessage,
                        controllerUiState?.libraryErrorMessage,
                        if (libraryRefreshInFlight) {
                            "Refreshing package library..."
                        } else {
                            "Refresh package library to continue."
                        },
                    ).joinToString("\n\n"),
                    packageSourceBaseUrl = packageSourceBaseUrl,
                    onPackageSourceBaseUrlChange = { nextBaseUrl ->
                        if (!packageSourceEditable) {
                            return@OfflinePackagesLibraryPanel
                        }
                        packageSourceBaseUrl = nextBaseUrl
                        writePackageSourceBaseUrl(prefs, nextBaseUrl)
                        offlinePackagesControllerResult = null
                    },
                    refreshInFlight = libraryRefreshInFlight,
                    sourceEditable = packageSourceEditable,
                    refreshEnabled = libraryRefreshEnabled,
                    refreshCancelEnabled = libraryRefreshCancelEnabled,
                    cancelRequested = offlinePackageCancelRequested,
                    onRefresh = {
                        if (!libraryRefreshEnabled) {
                            return@OfflinePackagesLibraryPanel
                        }
                        launchOfflinePackageOperation {
                            dispatchOfflinePackagesController(OfflinePackagesControllerEventWire.RefreshLibraryRequested)
                        }
                    },
                    onCancelRefresh = {
                        Log.i("OfflinePackages", "refresh cancel requested")
                        offlinePackageCancelRequested = true
                        activePackageConnections.disconnectAll()
                        offlinePackageOperationJob?.cancel(CancellationException("offline package refresh canceled"))
                    },
                    closeEnabled = !forceOfflinePackagesOpen,
                    onClose = {
                        if (!forceOfflinePackagesOpen) {
                            offlinePackagesOpen = false
                            onOfflinePackagesClosed?.invoke()
                        }
                    },
                    modifier = Modifier
                        .align(Alignment.Center)
                        .padding(ThumbGap * 1.4f)
                        .zIndex(1f),
                )
            } else if (controllerUiState.plannerUiState == null) {
                OfflinePackagesErrorPanel(
                    message = listOfNotNull(
                        bootstrapMessage,
                        controllerUiState.libraryErrorMessage ?: "Loading offline package planner...",
                    ).joinToString("\n\n"),
                    closeEnabled = !forceOfflinePackagesOpen,
                    onClose = {
                        if (!forceOfflinePackagesOpen) {
                            offlinePackagesOpen = false
                            onOfflinePackagesClosed?.invoke()
                        }
                    },
                    modifier = Modifier
                        .align(Alignment.Center)
                        .padding(ThumbGap * 1.4f)
                        .zIndex(1f),
                )
            } else {
                OfflinePackagesPanel(
                    regionOptions = regionOptions,
                    productOptions = OfflineProductOptions,
                    uiState = controllerUiState.plannerUiState,
                    navDbStatusText = navDbStatus?.let(::formatNavDbStatusLine),
                    syncMessage = controllerUiState.syncMessage,
                    cancelRequested = offlinePackageCancelRequested,
                    showSimulatedClockButtons = debugState.offlineSimulatedClockButtons,
                    packageSourceBaseUrl = packageSourceBaseUrl,
                    onPackageSourceBaseUrlChange = { nextBaseUrl ->
                        if (!controllerUiState.packageSourceEditable) {
                            return@OfflinePackagesPanel
                        }
                        packageSourceBaseUrl = nextBaseUrl
                        writePackageSourceBaseUrl(prefs, nextBaseUrl)
                        offlinePackagesControllerResult = null
                    },
                    onRefreshLibrary = {
                        if (!controllerUiState.refreshEnabled) {
                            return@OfflinePackagesPanel
                        }
                        launchOfflinePackageOperation {
                            dispatchOfflinePackagesController(OfflinePackagesControllerEventWire.RefreshLibraryRequested)
                        }
                    },
                    libraryRefreshInFlight = controllerUiState.libraryLoading,
                    packageSourceEditable = controllerUiState.packageSourceEditable,
                    refreshEnabled = controllerUiState.refreshEnabled,
                    refreshCancelEnabled = controllerUiState.refreshCancelEnabled,
                    syncEnabled = controllerUiState.syncEnabled,
                    syncCancelEnabled = controllerUiState.syncCancelEnabled,
                    plannerInteractionsEnabled = controllerUiState.plannerInteractionsEnabled,
                    onCancelRefresh = {
                        Log.i("OfflinePackages", "refresh cancel requested")
                        offlinePackageCancelRequested = true
                        activePackageConnections.disconnectAll()
                        offlinePackageOperationJob?.cancel(CancellationException("offline package refresh canceled"))
                    },
                    onRowClick = { event ->
                        if (!controllerUiState.plannerInteractionsEnabled) {
                            return@OfflinePackagesPanel
                        }
                        coroutineScope.launch {
                            dispatchOfflinePackagesController(
                                OfflinePackagesControllerEventWire.PackagesEvent(event),
                            )
                        }
                    },
                    onClockClick = { clockId ->
                        if (!controllerUiState.plannerInteractionsEnabled) {
                            return@OfflinePackagesPanel
                        }
                        coroutineScope.launch {
                            dispatchOfflinePackagesController(
                                OfflinePackagesControllerEventWire.PackagesEvent(
                                    if (clockId == "system") {
                                        OfflinePackagesEventWire(kind = "use_system_clock")
                                    } else {
                                        OfflinePackagesEventWire(
                                            kind = "set_clock_override",
                                            epochMs = clockId.toLong(),
                                        )
                                    },
                                ),
                            )
                        }
                    },
                    onSync = {
                        Log.i(
                            "OfflinePackages",
                            "sync button clicked syncInFlight=${controllerUiState.syncInFlight} libraryLoading=${controllerUiState.libraryLoading}",
                        )
                        if (!controllerUiState.syncEnabled || offlinePackageOperationJob?.isActive == true) {
                            return@OfflinePackagesPanel
                        }
                        launchOfflinePackageOperation {
                            Log.i("OfflinePackages", "dispatching SyncRequested")
                            dispatchOfflinePackagesController(OfflinePackagesControllerEventWire.SyncRequested)
                        }
                    },
                    onCancelOperation = {
                        Log.i("OfflinePackages", "sync cancel requested")
                        offlinePackageCancelRequested = true
                        activePackageConnections.disconnectAll()
                        offlinePackageOperationJob?.cancel(CancellationException("offline package operation canceled"))
                    },
                    syncInFlight = controllerUiState.syncInFlight,
                    closeEnabled = !forceOfflinePackagesOpen,
                    onClose = {
                        if (!forceOfflinePackagesOpen) {
                            offlinePackagesOpen = false
                            onOfflinePackagesClosed?.invoke()
                        }
                    },
                    modifier = Modifier
                        .align(Alignment.Center)
                        .padding(ThumbGap * 1.4f)
                        .zIndex(1f),
                )
            }
        }
    }
}
