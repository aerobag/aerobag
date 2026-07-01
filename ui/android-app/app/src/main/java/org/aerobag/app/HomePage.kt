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
import androidx.compose.ui.platform.LocalUriHandler
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
import org.aerobag.app.domain.DerivedChartPageState
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
import org.aerobag.app.domain.LatLonPoint
import org.aerobag.app.domain.MapChartFamily
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
import org.aerobag.app.domain.ResolvedLeg
import org.aerobag.app.domain.ResolvedLegSource
import org.aerobag.app.domain.RenderTile
import org.aerobag.app.domain.RouteSegmentStatus
import org.aerobag.app.domain.RouteComponentViewKind
import org.aerobag.app.domain.RouteComponent
import org.aerobag.app.domain.ScreenPoint
import org.aerobag.app.domain.SectionalPackages
import org.aerobag.app.domain.AndroidRuntimeContent
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

private const val HomeGridColumnCount = 3
private val HomeGridTileSize = ThumbSize * 2f
private val HomeGridWidth =
    (HomeGridTileSize * HomeGridColumnCount.toFloat()) +
        (ThumbGap * (HomeGridColumnCount - 1).toFloat())

@Composable
private fun HomePageBackdrop() {
    BoxWithConstraints(modifier = Modifier.fillMaxSize()) {
        val backdropAlignment = if (maxHeight > maxWidth) {
            Alignment.CenterStart
        } else {
            Alignment.BottomCenter
        }
        Image(
            painter = painterResource(R.drawable.home_page_backdrop),
            contentDescription = null,
            contentScale = ContentScale.Crop,
            alignment = backdropAlignment,
            modifier = Modifier.fillMaxSize(),
        )
    }
}

@Composable
internal fun HomePage(
    page: AppPage,
    pageHistory: List<AppViewSnapshot>,
    mostRecentChartOrPlatePage: AppPage = AppPage.Map,
    uptimeLabel: String,
    bootstrap: org.aerobag.app.domain.RuntimeBootstrap,
    debugState: UiDebugState,
    navElement: NavElementUiView?,
    onSelectPage: (AppPage) -> Unit,
    onOpenPlan: () -> Unit,
    onOpenRecentChartOrPlate: () -> Unit = {},
    offlinePackagesControllerHandle: Long,
    onOfflinePackagesClosed: (() -> Unit)? = null,
    onOfflinePackageLibraryCacheChanged: (String?) -> Unit = {},
) {
    val uiTheme = LocalAerobagUiTheme.current
    val context = LocalContext.current
    val uriHandler = LocalUriHandler.current
    val prefs = remember(context) { context.applicationContext.getSharedPreferences(UiPrefsName, Context.MODE_PRIVATE) }
    val coroutineScope = rememberCoroutineScope()
    val offlinePackagesControllerAlive = remember(offlinePackagesControllerHandle) { AtomicBoolean(true) }
    DisposableEffect(offlinePackagesControllerHandle) {
        onDispose { offlinePackagesControllerAlive.set(false) }
    }
    var packageSourceBaseUrl by remember(context, prefs) {
        mutableStateOf(readPackageSourceBaseUrl(context.applicationContext, prefs))
    }
    val offlinePackagesRouted = page == AppPage.OfflinePackages
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
            diagnosticLogInfo("OfflinePackages") {
                "dropping event=${event::class.simpleName} for disposed controller handle=$offlinePackagesControllerHandle"
            }
            return
        }
        val startMs = SystemClock.elapsedRealtime()
        diagnosticLogInfo("OfflinePackages") {
            "controller event=${event::class.simpleName} handle=$offlinePackagesControllerHandle scanning installed packages"
        }
        val (installed, storage) = withContext(Dispatchers.IO) {
            Pair(
                listInstalledPackageArtifacts(context.applicationContext),
                installedPackageStorageInfo(context.applicationContext),
            )
        }
        val packageStateScanElapsedMs = SystemClock.elapsedRealtime() - startMs
        val input = OfflinePackagesControllerInputWire(
            packageSourceBaseUrl = packageSourceBaseUrl,
            discoveryFilenames = emptyList(),
            regionIds = regionIds,
            productIds = productIds,
            nowEpochMs = bootstrap.packageManagementNowEpochMsOverride ?: System.currentTimeMillis(),
            installed = installed,
            storage = storage,
            event = event,
        )
        val inputJson = PackageManagementJson.encodeToString(input)
        diagnosticLogInfo("OfflinePackages") {
            "controller event=${event::class.simpleName} handle=$offlinePackagesControllerHandle " +
                "installed=${installed.size} packageStateScanMs=$packageStateScanElapsedMs inputBytes=${inputJson.length}"
        }
        if (!offlinePackagesControllerAlive.get()) {
            diagnosticLogInfo("OfflinePackages") {
                "dropping event=${event::class.simpleName} after package scan for disposed controller handle=$offlinePackagesControllerHandle"
            }
            return
        }
        val outputJson = try {
            NativeBindings.dispatchOfflinePackagesControllerJson(offlinePackagesControllerHandle, inputJson)
        } catch (error: RuntimeException) {
            if (error.message?.contains("invalid offline packages controller handle") == true) {
                diagnosticLogInfo("OfflinePackages") {
                    "dropping event=${event::class.simpleName} for stale controller handle=$offlinePackagesControllerHandle"
                }
                return
            }
            throw error
        }
        val result = PackageManagementJson.decodeFromString<OfflinePackagesControllerResultWire>(outputJson)
        Log.i(
            "OfflinePackages",
            "controller result event=${event::class.simpleName} source=$packageSourceBaseUrl " +
                "planner=${result.uiState.plannerUiState != null} " +
                "libraryLoaded=${result.uiState.libraryLoaded} " +
                "libraryLoading=${result.uiState.libraryLoading} " +
                "refreshEnabled=${result.uiState.refreshEnabled} " +
                "syncEnabled=${result.uiState.syncEnabled} " +
                "status=${result.uiState.libraryStatusMessage?.lineSequence()?.firstOrNull()} " +
                "error=${result.uiState.libraryErrorMessage?.lineSequence()?.firstOrNull()}",
        )
        writeOfflinePackagesStateJson(prefs, result.packagesStateJson)
        writeOfflinePackagesLibraryCacheJson(prefs, result.libraryCacheJson)
        offlinePackagesControllerResult = result
        if (event is OfflinePackagesControllerEventWire.LibraryRefreshSucceeded) {
            onOfflinePackageLibraryCacheChanged(result.libraryCacheJson)
        }
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
                    Log.e(
                        "OfflinePackages",
                        "library refresh failed source=${command.packageSourceBaseUrl}",
                        error,
                    )
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
            }
            null -> Unit
        }
    }
    LaunchedEffect(offlinePackagesRouted) {
        if (offlinePackagesRouted) {
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
        if (!offlinePackagesRouted) {
            HomePageBackdrop()
        }

        val homeGridRowCount =
            ((HomeGridButtons.size + HomeGridColumnCount - 1) / HomeGridColumnCount)
                .coerceAtLeast(1)
        val homeGridHeight =
            (HomeGridTileSize * homeGridRowCount.toFloat()) +
                (ThumbGap * (homeGridRowCount - 1).toFloat())

        HomeReturnDock(
            modifier = Modifier
                .align(Alignment.TopStart)
                .zIndex(2f),
            currentPage = page,
            chartPlateTargetPage = mostRecentChartOrPlatePage,
            onHomeClick = { onSelectPage(AppPage.Home) },
            onOpenChartOrPlate = onOpenRecentChartOrPlate,
        )

        if (!offlinePackagesRouted) {
            LazyVerticalGrid(
                columns = GridCells.Fixed(HomeGridColumnCount),
                modifier = Modifier
                    .align(Alignment.Center)
                    .width(HomeGridWidth)
                    .height(homeGridHeight),
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
                            diagnosticLogInfo("AerobagNavigation") {
                                "home button key=${button.key} target=${button.targetPage} external=${button.externalUrl}"
                            }
                            if (button.targetPage != null) {
                                onSelectPage(button.targetPage)
                            } else if (button.externalUrl != null) {
                                uriHandler.openUri(button.externalUrl)
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
        }

        if (offlinePackagesRouted) {
            val offlinePanelModifier = Modifier
                .fillMaxSize()
                .padding(
                    start = ThumbGap,
                    end = ThumbGap,
                    top = ThumbSize + (ThumbGap * 2f),
                    bottom = ThumbGap,
                )
                .zIndex(1f)
            val controllerUiState = offlinePackagesControllerResult?.uiState
            val navDbStatus by produceState<org.aerobag.app.domain.NavDbStatus?>(initialValue = null, context, navDbStatusRefreshToken, offlinePackagesRouted) {
                value = withContext(Dispatchers.IO) {
                    AndroidRuntimeContent.inspectNavDbStatus(
                        context.applicationContext,
                        readOfflinePackagesLibraryCacheJson(prefs),
                    )
                }
            }
            LaunchedEffect(
                offlinePackagesRouted,
                packageSourceBaseUrl,
                controllerUiState?.plannerUiState != null,
                controllerUiState?.libraryLoaded,
                controllerUiState?.libraryLoading,
                controllerUiState?.libraryStatusMessage,
                controllerUiState?.libraryErrorMessage,
            ) {
                val uiState = controllerUiState ?: return@LaunchedEffect
                if (
                    offlinePackagesRouted &&
                    uiState.plannerUiState == null &&
                    !uiState.libraryLoading &&
                    (uiState.libraryStatusMessage != null || uiState.libraryErrorMessage != null)
                ) {
                    Log.w(
                        "OfflinePackages",
                        "planner table unavailable source=$packageSourceBaseUrl " +
                            "libraryLoaded=${uiState.libraryLoaded} " +
                            "refreshEnabled=${uiState.refreshEnabled} " +
                            "status=${uiState.libraryStatusMessage} " +
                            "error=${uiState.libraryErrorMessage}",
                    )
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
                        controllerUiState?.libraryStatusMessage ?: controllerUiState?.libraryErrorMessage,
                        if (libraryRefreshInFlight) {
                            "Refreshing package library..."
                        } else {
                            "Refresh package library to continue."
                        },
                    ).joinToString("\n\n"),
                    storageCapacityLabel = controllerUiState?.storageCapacityLabel,
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
                        diagnosticLogInfo("OfflinePackages") { "refresh cancel requested" }
                        offlinePackageCancelRequested = true
                        activePackageConnections.disconnectAll()
                        offlinePackageOperationJob?.cancel(CancellationException("offline package refresh canceled"))
                    },
                    closeEnabled = false,
                    onClose = { onOfflinePackagesClosed?.invoke() },
                    showCloseButton = false,
                    modifier = offlinePanelModifier,
                )
            } else if (controllerUiState.plannerUiState == null) {
                OfflinePackagesErrorPanel(
                    message = listOfNotNull(
                        controllerUiState.libraryErrorMessage ?: "Loading offline package planner...",
                    ).joinToString("\n\n"),
                    closeEnabled = false,
                    onClose = { onOfflinePackagesClosed?.invoke() },
                    showCloseButton = false,
                    modifier = offlinePanelModifier,
                )
            } else {
                OfflinePackagesPanel(
                    regionOptions = regionOptions,
                    productOptions = OfflineProductOptions,
                    uiState = controllerUiState.plannerUiState,
                    navDbStatusText = navDbStatus?.let(::formatNavDbStatusLine),
                    storageCapacityLabel = controllerUiState.storageCapacityLabel,
                    syncMessage = listOfNotNull(
                        controllerUiState.libraryStatusMessage,
                        controllerUiState.syncMessage,
                    ).joinToString("\n\n").ifBlank { null },
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
                        diagnosticLogInfo("OfflinePackages") { "refresh cancel requested" }
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
                        diagnosticLogInfo("OfflinePackages") {
                            "sync button clicked syncInFlight=${controllerUiState.syncInFlight} libraryLoading=${controllerUiState.libraryLoading}"
                        }
                        if (!controllerUiState.syncEnabled || offlinePackageOperationJob?.isActive == true) {
                            return@OfflinePackagesPanel
                        }
                        launchOfflinePackageOperation {
                            diagnosticLogInfo("OfflinePackages") { "dispatching SyncRequested" }
                            dispatchOfflinePackagesController(OfflinePackagesControllerEventWire.SyncRequested)
                        }
                    },
                    onCancelOperation = {
                        diagnosticLogInfo("OfflinePackages") { "sync cancel requested" }
                        offlinePackageCancelRequested = true
                        activePackageConnections.disconnectAll()
                        offlinePackageOperationJob?.cancel(CancellationException("offline package operation canceled"))
                    },
                    syncInFlight = controllerUiState.syncInFlight,
                    closeEnabled = false,
                    onClose = { onOfflinePackagesClosed?.invoke() },
                    showCloseButton = false,
                    modifier = offlinePanelModifier,
                )
            }
        }
    }
}
