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
import org.aerobag.app.domain.ChartPackages
import org.aerobag.app.domain.AppState
import org.aerobag.app.domain.AirwayPresentationPlan
import org.aerobag.app.domain.AirwaySuggestion
import org.aerobag.app.domain.WaypointIdentifierSuggestion
import org.aerobag.app.domain.CoreMapViewport
import org.aerobag.app.domain.DerivedChartPageState
import org.aerobag.app.domain.FlightPlan
import org.aerobag.app.domain.FlightPlanEntryPreview
import org.aerobag.app.domain.FlightPlanUiMutation
import org.aerobag.app.domain.FlightDataCell
import org.aerobag.app.domain.FlightPlanDisplayRowKind
import org.aerobag.app.domain.FlightPlanDisplayRowUiView
import org.aerobag.app.domain.FlightPlanRowActionUiView
import org.aerobag.app.domain.FlightPlanRouteSegment
import org.aerobag.app.domain.FlightPlanUiState
import org.aerobag.app.domain.GuidanceState
import org.aerobag.app.domain.InstalledPackages
import org.aerobag.app.domain.AirspaceDisplayDecoration
import org.aerobag.app.domain.AirspaceDisplayDecorationSegment
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
import org.aerobag.app.domain.RasterMapUiState
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
import org.aerobag.app.domain.AndroidLiveFeedClient
import org.aerobag.app.domain.LiveFeedCacheStore
import org.aerobag.app.domain.LiveFeedFetchPolicy
import org.aerobag.app.domain.LiveFeedInstalledSummary
import org.aerobag.app.domain.SequencingMode
import org.aerobag.app.domain.SituationControlInput
import org.aerobag.app.domain.SituationRingCandidate
import org.aerobag.app.domain.TileStorageKind
import org.aerobag.app.domain.UiDataStatusState
import org.aerobag.app.domain.UiDebugState
import org.aerobag.app.domain.UiMapLayerToggleState
import org.aerobag.app.domain.UiStatusSeverity
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

internal val LocalAerobagUiTheme = staticCompositionLocalOf<UiTheme> {
    error("Aerobag UI theme not provided")
}

internal val ThumbSize = 56.dp
internal val ThumbGap = 5.6.dp
internal val SituationDockOverlapWidth = ThumbSize * 10f
internal val PlanGridGap = 2.dp
internal const val DefaultPlaybackTracePath = "/adsb-traces/n550ar/n550ar-2024-09-29.json"
internal const val DefaultAndroidDevServerBaseUrl = "http://10.0.2.2:8080"
internal const val DefaultAndroidPackageSourceBaseUrl = "aerobag.org"
internal const val CurrentArtifactsDiscoveryFilename = "current_artifacts.json"
internal const val PublicationPackageRootPath = "packages"
internal const val WebMercatorWorldSize = 256.0
internal const val WebMercatorHalfWorldM = 20037508.342789244
internal const val TerrainAltitudeBucketFt = 200
internal const val MapLayerLogTag = "MapLayers"
internal const val TileBudgetLogTag = "AerobagTileBudget"
internal const val DecodedTileCacheMaxBytes = 96L * 1024L * 1024L
internal const val MapTileLoadWorkerCount = 4
internal const val SlowTileLoadLogMs = 1000L
internal val TileLoadGenerationIds = AtomicLong()
internal val VampsPosition = LatLon(47.3648944444444, -121.980275)

internal data class PageTilePaintTiming(
    val id: Long,
    val fromPage: AppPage,
    val startedMs: Long,
    val trigger: String,
)

@kotlinx.serialization.Serializable
internal data class WireRasterTilePlan(
    val tiles: List<WireRasterTileDraw> = emptyList(),
)

@kotlinx.serialization.Serializable
internal data class WireRasterTileDraw(
    val family: String,
    val source_zoom: Int,
    val x: Int,
    val y_tms: Int,
    val left_px: Double,
    val top_px: Double,
    val size_px: Double,
    val primary: WireRasterTileSource,
    val fallbacks: List<WireRasterTileSource> = emptyList(),
)

@kotlinx.serialization.Serializable
internal data class WireRasterTileSource(
    val map_view_id: String,
    val package_name: String? = null,
    val storage_kind: String? = null,
    val relative_path: String? = null,
    val resource: WireRasterTileResource,
)

@kotlinx.serialization.Serializable
internal data class WireRasterTileResource(
    val kind: String,
    val package_name: String? = null,
    val member_path: String? = null,
    val path: String? = null,
)

internal data class LatLon(val lat: Double, val lon: Double)

internal data class TerrainOverlayImage(
    val key: String,
    val z: Int,
    val x: Int,
    val yTms: Int,
    val left: Double,
    val top: Double,
    val size: Double,
    val bitmap: androidx.compose.ui.graphics.ImageBitmap,
)

internal data class SituationOverlay(
    val pointUnits: Offset,
    val headingDeg: Float,
    val predictorUnits: Offset?,
    val ring: SituationRing?,
)

internal data class SituationRing(
    val radiusUnits: Float,
    val tickMarks: List<SituationTickMark>,
    val cardinalLabels: List<SituationCardinalLabel>,
    val labelPointUnits: Offset,
    val labelRotationDeg: Float,
    val labelText: String,
)

internal data class SituationTickMark(
    val innerUnits: Offset,
    val outerUnits: Offset,
)

internal data class SituationCardinalLabel(
    val text: String,
    val pointUnits: Offset,
    val rotationDeg: Float,
)

internal val ThumbRadius = 10.dp
internal val FolderThumbGutter = ThumbSize * 0.3f
internal val PlateFolderTileWidth = ThumbSize * 2f
internal val PlateFolderTileHeight = ThumbSize * 3f
internal val PlatePageTrayWidth = ThumbSize * 4f
internal val PlanArrowLane = ThumbSize * 0.5f
internal val PlanArrowButtonInset = 5.dp
internal const val UiPrefsName = "aerobag_ui"
internal const val UiPrefsPageKey = "page"
internal const val UiPrefsSelectedAirportKey = "selected_airport_id"
internal const val UiPrefsSelectedChartKey = "selected_chart_id"
internal const val UiPrefsRecentAirportsKey = "recent_airport_ids"
internal const val UiPrefsOfflinePackagePreferencesKey = "offline_package_preferences"
internal const val UiPrefsPackageSourceBaseUrlKey = "package_source_base_url"
internal const val MapViewportLogTag = "MapViewport"
internal const val MaxViewHistoryDepth = 64
internal const val OverlayPlaneControls = 10f
internal const val OverlayPlaneModalScrim = 80f
internal const val OverlayPlaneModal = 90f
internal fun defaultUiDebugState() = UiDebugState(
    tileLabels = false,
    nexradTileLabels = false,
    playbackVisible = false,
    fastTiles = false,
    offlineSimulatedClockButtons = false,
)
internal val PackageManagementJson = Json {
    encodeDefaults = true
    ignoreUnknownKeys = true
    classDiscriminator = "kind"
}

internal enum class AppPage {
    Map,
    Plan,
    Charts,
    Home,
}

internal data class AppViewSnapshot(
    val page: AppPage,
    val selectedMapId: String,
    val selectedMapLauncherLabel: String,
    val mapViewport: MapViewportState,
    val selectedAirportId: String,
    val selectedChartId: String,
    val selectedChartLabel: String,
    val recentAirportIds: List<String>,
    val chartViewport: org.aerobag.app.domain.ImageViewportState?,
    val chartFolderOpen: Boolean,
)

internal data class MapSelectionUiState(
    val point: Offset,
    val result: MapSelectionQueryResult,
    val selectedItem: MapSelectionItem?,
)

internal data class FlightPlanDisplayRow(
    val id: String,
    val selectionKey: String,
    val label: String,
    val rowKind: String,
    val componentKind: RouteComponentViewKind? = null,
    val componentUid: String? = null,
    val componentIndex: Int? = null,
    val procedureId: String? = null,
    val procedureKind: org.aerobag.app.domain.ProcedureKind? = null,
    val legIndex: Int? = null,
    val dataCells: List<FlightDataCell> = emptyList(),
    val showPlateTargetId: String? = null,
    val chartAirportId: String? = null,
    val navRef: NavRef? = null,
    val symbolFeature: org.aerobag.app.domain.NavSymbolFeature? = null,
    val depth: Int = 0,
    val active: Boolean = false,
    val enabled: Boolean = true,
    val syntheticDirectTo: Boolean = false,
    val canAddAirwayAfter: Boolean = false,
    val canAddProcedureBefore: Boolean = false,
    val canRemoveComponent: Boolean = false,
    val canReorderComponent: Boolean = false,
    val canReorderUp: Boolean = false,
    val canReorderDown: Boolean = false,
    val actionMatrix: List<List<FlightPlanRowActionUiView>> = emptyList(),
    val startComponentIndex: Int? = null,
    val endComponentIndex: Int? = null,
    val originAnchor: NavRef? = null,
    val destinationAnchor: NavRef? = null,
)

internal sealed interface FlightPlanDisplayBlock {
    data class Single(
        val index: Int,
        val row: FlightPlanDisplayRow,
    ) : FlightPlanDisplayBlock

    data class Group(
        val headerIndex: Int,
        val header: FlightPlanDisplayRow,
        val children: List<Pair<Int, FlightPlanDisplayRow>>,
    ) : FlightPlanDisplayBlock
}

internal data class StructuredArrowSpec(
    val fromPoint: Offset,
    val toPoint: Offset,
    val toClipped: Boolean,
    val fromClippedAbove: Boolean,
    val elbowX: Float,
    val shaftEndX: Float,
    val headLength: Float,
)

internal data class StructuredArrowEndpoint(
    val point: Offset,
    val clipped: Boolean,
    val clippedAbove: Boolean,
    val clippedBelow: Boolean,
)

@Composable
internal fun rememberStructuredRowBounds(
    rowId: String,
    structuredRowBounds: MutableMap<String, Rect>,
): Modifier {
    DisposableEffect(rowId, structuredRowBounds) {
        onDispose {
            structuredRowBounds.remove(rowId)
        }
    }
    return Modifier.onGloballyPositioned { coordinates ->
        structuredRowBounds[rowId] = coordinates.boundsInWindow()
    }
}

internal data class AndroidAirwayPickerState(
    val loading: Boolean,
    val error: String?,
    val rowUid: String,
    val originAnchor: NavRef,
    val destinationAnchor: NavRef?,
    val suggestions: List<AirwaySuggestion>,
    val selectedAirwayName: String?,
    val presentation: AirwayPresentationPlan?,
    val selectedEntryIndex: Int?,
)

internal data class AndroidProcedurePickerState(
    val loading: Boolean,
    val error: String?,
    val rowUid: String,
    val airportId: String,
    val procedures: List<ProcedureSummary>,
    val selectedProcedureId: String?,
    val options: ProcedureOptions?,
)

internal data class AndroidAirportInsertState(
    val rowUid: String,
    val before: Boolean,
    val airportId: String,
    val error: String?,
    val loading: Boolean,
    val suggestions: List<WaypointIdentifierSuggestion>,
)

internal data class PageTrayOption(
    val page: AppPage,
    val label: String,
    val launcherLabel: String,
    @DrawableRes val iconResId: Int? = null,
)

internal data class OfflinePackageDimension(
    val id: String,
    val label: String,
)

@Serializable
internal enum class OfflinePackageSelection {
    @SerialName("unselected")
    Unselected,
    @SerialName("pause")
    Pause,
    @SerialName("play")
    Play,
}

@Serializable
internal data class OfflinePackagePreferencesWire(
    val regions: Map<String, OfflinePackageSelection> = emptyMap(),
    val products: Map<String, OfflinePackageSelection> = emptyMap(),
)

@Serializable
internal data class InstalledArtifactWire(
    @SerialName("artifact_id")
    val artifactId: String,
    val filename: String? = null,
    @SerialName("size_bytes")
    val sizeBytes: Long? = null,
    @SerialName("checksum_sha256")
    val checksumSha256: String? = null,
)


@Serializable
internal data class OfflinePackagesUiRowWire(
    val id: String,
    val selection: OfflinePackageSelection,
    @SerialName("fetch_count")
    val fetchCount: Int = 0,
    @SerialName("gc_count")
    val gcCount: Int = 0,
    @SerialName("pause_count")
    val pauseCount: Int = 0,
    @SerialName("plan_entries")
    val planEntries: List<OfflinePackagesUiPlanEntryWire> = emptyList(),
    @SerialName("installed_size_label")
    val installedSizeLabel: String = "0M",
    @SerialName("planned_delta_label")
    val plannedDeltaLabel: String = "+0M",
    @SerialName("planned_total_size_label")
    val plannedTotalSizeLabel: String = "0M",
    @SerialName("planned_size_change_visible")
    val plannedSizeChangeVisible: Boolean = false,
    @SerialName("sync_progress_per_mille")
    val syncProgressPerMille: Int? = null,
)

@Serializable
internal enum class OfflinePackagesUiPlanActionWire {
    @SerialName("delete")
    Delete,
    @SerialName("keep")
    Keep,
    @SerialName("pause")
    Pause,
    @SerialName("fetch")
    Fetch,
}

@Serializable
internal data class OfflinePackagesUiPlanEntryWire(
    val action: OfflinePackagesUiPlanActionWire,
    val count: Int = 0,
    val cycles: List<String> = emptyList(),
)

@Serializable
internal data class OfflinePackagesClockOptionWire(
    val id: String,
    val label: String,
    val active: Boolean = false,
)

@Serializable
internal data class OfflinePackagesUiStateWire(
    @SerialName("clock_label")
    val clockLabel: String = "",
    @SerialName("clock_options")
    val clockOptions: List<OfflinePackagesClockOptionWire> = emptyList(),
    @SerialName("all_packages")
    val allPackages: OfflinePackagesUiRowWire = OfflinePackagesUiRowWire(
        id = "all-packages",
        selection = OfflinePackageSelection.Play,
    ),
    @SerialName("core_products")
    val coreProducts: List<OfflinePackagesUiRowWire> = emptyList(),
    val regions: List<OfflinePackagesUiRowWire> = emptyList(),
    val products: List<OfflinePackagesUiRowWire> = emptyList(),
)

@Serializable
internal data class PackageManagementPlanWire(
    val fetch: List<String> = emptyList(),
    @SerialName("retain_installed")
    val retainInstalled: List<String> = emptyList(),
    val gc: List<String> = emptyList(),
    @SerialName("protected_by_pause")
    val protectedByPause: List<String> = emptyList(),
)

@Serializable
internal data class BundlePackageArtifactWire(
    val id: String,
    @SerialName("family_id")
    val familyId: String,
    @SerialName("region_id")
    val regionId: String? = null,
    val filename: String,
    @SerialName("relative_path")
    val relativePath: String,
    @SerialName("cycle")
    val cycle: String? = null,
    @SerialName("cycle_version")
    val cycleVersion: String? = null,
    @SerialName("checksum_sha256")
    val checksumSha256: String? = null,
    @SerialName("size_bytes")
    val sizeBytes: Long? = null,
    @SerialName("effective_date")
    val effectiveDate: String? = null,
    @SerialName("expiration_date")
    val expirationDate: String? = null,
)

@Serializable
internal data class BundleManifestWire(
    val packages: List<BundlePackageArtifactWire> = emptyList(),
)

@Serializable
internal data class OfflinePackagesSyncSummary(
    @SerialName("fetched_count")
    val fetchedCount: Int,
    @SerialName("gc_count")
    val gcCount: Int,
    val warnings: List<OfflinePackagesWarning>,
    @SerialName("remote_poisoned_filename_messages")
    val remotePoisonedFilenameMessages: Map<String, String> = emptyMap(),
)

@Serializable
internal data class OfflinePackagesSyncProgressWire(
    @SerialName("completed_fetch_artifact_ids")
    val completedFetchArtifactIds: Set<String> = emptySet(),
    @SerialName("active_fetch_bytes_by_artifact_id")
    val activeFetchBytesByArtifactId: Map<String, Long> = emptyMap(),
)

@Serializable
internal data class OfflinePackagesWarning(
    @SerialName("artifact_id")
    val artifactId: String,
    @SerialName("family_id")
    val familyId: String?,
    @SerialName("region_id")
    val regionId: String?,
    val message: String,
)

@Serializable
internal data class OfflinePackagesReduceResultWire(
    val state: OfflinePackagesStateWire,
    @SerialName("ui_state")
    val uiState: OfflinePackagesUiStateWire,
    @SerialName("effective_now_epoch_ms")
    val effectiveNowEpochMs: Long,
    val plan: PackageManagementPlanWire,
    val bundle: BundleManifestWire,
)

@Serializable
internal data class CurrentArtifactsDiscoveryInputWire(
    @SerialName("publication_root_url")
    val publicationRootUrl: String,
    @SerialName("current_artifacts_json")
    val currentArtifactsJson: String,
)

@Serializable
internal data class CurrentArtifactsDiscoveryPlanWire(
    @SerialName("discovery_jsons")
    val discoveryJsons: List<String>,
    @SerialName("bundle_requests")
    val bundleRequests: List<CurrentArtifactsBundleRequestWire>,
)

@Serializable
internal data class CurrentArtifactsBundleRequestWire(
    val filename: String,
    val url: String,
)

@Serializable
internal data class OfflinePackagesControllerUiStateWire(
    @SerialName("planner_ui_state")
    val plannerUiState: OfflinePackagesUiStateWire? = null,
    @SerialName("library_loaded")
    val libraryLoaded: Boolean = false,
    @SerialName("library_loading")
    val libraryLoading: Boolean = false,
    @SerialName("library_error_message")
    val libraryErrorMessage: String? = null,
    @SerialName("sync_in_flight")
    val syncInFlight: Boolean = false,
    @SerialName("sync_message")
    val syncMessage: String? = null,
    @SerialName("package_source_editable")
    val packageSourceEditable: Boolean = true,
    @SerialName("refresh_enabled")
    val refreshEnabled: Boolean = false,
    @SerialName("refresh_cancel_enabled")
    val refreshCancelEnabled: Boolean = false,
    @SerialName("sync_enabled")
    val syncEnabled: Boolean = false,
    @SerialName("sync_cancel_enabled")
    val syncCancelEnabled: Boolean = false,
    @SerialName("planner_interactions_enabled")
    val plannerInteractionsEnabled: Boolean = true,
)

@Serializable
internal sealed interface OfflinePackagesControllerCommandWire {
    @Serializable
    @SerialName("refresh_library")
    data class RefreshLibrary(
        @SerialName("package_source_base_url")
        val packageSourceBaseUrl: String,
        @SerialName("discovery_filenames")
        val discoveryFilenames: List<String>,
    ) : OfflinePackagesControllerCommandWire

    @Serializable
    @SerialName("sync")
    data class Sync(
        @SerialName("package_source_base_url")
        val packageSourceBaseUrl: String,
        @SerialName("packaged_artifact_root")
        val packagedArtifactRoot: String,
        val plan: PackageManagementPlanWire,
        val bundle: BundleManifestWire,
        @SerialName("max_parallel_fetches")
        val maxParallelFetches: Int = 4,
    ) : OfflinePackagesControllerCommandWire
}

@Serializable
internal data class OfflinePackagesControllerResultWire(
    @SerialName("packages_state_json")
    val packagesStateJson: String? = null,
    @SerialName("ui_state")
    val uiState: OfflinePackagesControllerUiStateWire,
    val command: OfflinePackagesControllerCommandWire? = null,
)

@Serializable
internal data class OfflinePackagesInitInputWire(
    val state: OfflinePackagesStateWire? = null,
    @SerialName("region_ids")
    val regionIds: List<String>,
    @SerialName("product_ids")
    val productIds: List<String>,
    @SerialName("now_epoch_ms")
    val nowEpochMs: Long,
    @SerialName("discovery_jsons")
    val discoveryJsons: List<String>,
    @SerialName("bundle_jsons_by_filename")
    val bundleJsonsByFilename: Map<String, String>,
    val installed: List<InstalledArtifactWire>,
)

@Serializable
internal data class OfflinePackagesEventWire(
    val kind: String,
    val id: String? = null,
    @SerialName("epoch_ms")
    val epochMs: Long? = null,
)

@Serializable
internal data class OfflinePackagesReduceInputWire(
    val state: OfflinePackagesStateWire,
    val event: OfflinePackagesEventWire,
    @SerialName("region_ids")
    val regionIds: List<String>,
    @SerialName("product_ids")
    val productIds: List<String>,
    @SerialName("now_epoch_ms")
    val nowEpochMs: Long,
    @SerialName("discovery_jsons")
    val discoveryJsons: List<String>,
    @SerialName("bundle_jsons_by_filename")
    val bundleJsonsByFilename: Map<String, String>,
    val installed: List<InstalledArtifactWire>,
)

@Serializable
internal sealed interface OfflinePackagesControllerEventWire {
    @Serializable
    @SerialName("ensure_library")
    data object EnsureLibrary : OfflinePackagesControllerEventWire

    @Serializable
    @SerialName("refresh_library_requested")
    data object RefreshLibraryRequested : OfflinePackagesControllerEventWire

    @Serializable
    @SerialName("library_refresh_succeeded")
    data class LibraryRefreshSucceeded(
        @SerialName("fetched_at_epoch_ms")
        val fetchedAtEpochMs: Long,
        @SerialName("discovery_jsons")
        val discoveryJsons: List<String>,
        @SerialName("bundle_jsons_by_filename")
        val bundleJsonsByFilename: Map<String, String>,
    ) : OfflinePackagesControllerEventWire

    @Serializable
    @SerialName("library_refresh_failed")
    data class LibraryRefreshFailed(
        val message: String,
    ) : OfflinePackagesControllerEventWire

    @Serializable
    @SerialName("installed_artifact_health_observed")
    data class InstalledArtifactHealthObserved(
        @SerialName("unreadable_installed_filename_messages")
        val unreadableInstalledFilenameMessages: Map<String, String>,
    ) : OfflinePackagesControllerEventWire

    @Serializable
    @SerialName("packages_event")
    data class PackagesEvent(
        val event: OfflinePackagesEventWire,
    ) : OfflinePackagesControllerEventWire

    @Serializable
    @SerialName("sync_requested")
    data object SyncRequested : OfflinePackagesControllerEventWire

    @Serializable
    @SerialName("sync_progress_observed")
    data class SyncProgressObserved(
        val progress: OfflinePackagesSyncProgressWire,
    ) : OfflinePackagesControllerEventWire

    @Serializable
    @SerialName("sync_finished")
    data class SyncFinished(
        val summary: OfflinePackagesSyncSummary,
    ) : OfflinePackagesControllerEventWire
}

@Serializable
internal data class OfflinePackagesControllerInputWire(
    @SerialName("package_source_base_url")
    val packageSourceBaseUrl: String,
    @SerialName("discovery_filenames")
    val discoveryFilenames: List<String>,
    @SerialName("region_ids")
    val regionIds: List<String>,
    @SerialName("product_ids")
    val productIds: List<String>,
    @SerialName("now_epoch_ms")
    val nowEpochMs: Long,
    val installed: List<InstalledArtifactWire>,
    val event: OfflinePackagesControllerEventWire,
)

@Serializable
internal data class OfflinePackagesStateWire(
    val preferences: OfflinePackagePreferencesWire = OfflinePackagePreferencesWire(),
    @SerialName("now_override_epoch_ms")
    val nowOverrideEpochMs: Long? = null,
)

internal data class HomeGridButton(
    val key: String,
    val label: String,
    val targetPage: AppPage? = null,
    val enabled: Boolean = false,
    @DrawableRes val iconResId: Int? = null,
)

internal data class MenuDockOption(
    val key: String,
    val label: String,
    val active: Boolean = false,
    val enabled: Boolean = true,
    val accentColor: Color? = null,
    val toggleState: UiMapLayerToggleState? = null,
    @DrawableRes val iconResId: Int? = null,
    val onSelect: () -> Unit,
)

internal enum class MenuDockStyle(
    val buttonWidth: androidx.compose.ui.unit.Dp,
    val buttonHeight: androidx.compose.ui.unit.Dp = ThumbSize,
    val trayWidth: androidx.compose.ui.unit.Dp,
    val launcherMaxLines: Int,
) {
    Compact(
        buttonWidth = ThumbSize,
        trayWidth = ThumbSize * 2.9f,
        launcherMaxLines = 2,
    ),
    PlateAirport(
        buttonWidth = ThumbSize,
        trayWidth = PlatePageTrayWidth,
        launcherMaxLines = 1,
    ),
    PlateWide(
        buttonWidth = ThumbSize * 1.75f,
        trayWidth = PlatePageTrayWidth,
        launcherMaxLines = 2,
    ),
    Layers(
        buttonWidth = ThumbSize,
        trayWidth = ThumbSize * 4f,
        launcherMaxLines = 2,
    ),
    Situation(
        buttonWidth = ThumbSize * 2f,
        buttonHeight = ThumbSize * 0.5f,
        trayWidth = (ThumbSize * 4f) + 9.dp,
        launcherMaxLines = 1,
    ),
    DataStatus(
        buttonWidth = ThumbSize * 1.45f,
        trayWidth = ThumbSize * 4f,
        launcherMaxLines = 1,
    ),
}

internal val PageOptions = listOf(
    PageTrayOption(AppPage.Map, "CHART", "CHART", R.drawable.page_chart_icon),
    PageTrayOption(AppPage.Charts, "PLATE", "PLATE", R.drawable.page_plate_icon),
    PageTrayOption(AppPage.Plan, "FLIGHT PLAN", "PLAN", R.drawable.page_plan1_icon),
    PageTrayOption(AppPage.Home, "HOME", "HOME"),
)

internal fun mostRecentChartOrPlatePageFromHistory(pageHistory: List<AppViewSnapshot>): AppPage =
    pageHistory
        .asReversed()
        .firstOrNull { it.page == AppPage.Map || it.page == AppPage.Charts }
        ?.page
        ?: AppPage.Map

internal val OfflineProductOptions = listOf(
    OfflinePackageDimension("sec", "Sectional"),
    OfflinePackageDimension("tac", "TAC"),
    OfflinePackageDimension("shaded-relief", "Shaded Relief"),
    OfflinePackageDimension("enr-l", "IFR-L"),
    OfflinePackageDimension("enr-h", "IFR-H"),
    OfflinePackageDimension("tpp", "TPP"),
    OfflinePackageDimension("csup", "CSUP"),
)

internal val HomeGridButtons = listOf(
    HomeGridButton("chart", "CHART", targetPage = AppPage.Map, enabled = true, iconResId = R.drawable.page_chart_icon),
    HomeGridButton("plate", "PLATE", targetPage = AppPage.Charts, enabled = true, iconResId = R.drawable.page_plate_icon),
    HomeGridButton("flight-plan", "FLIGHT\nPLAN", targetPage = AppPage.Plan, enabled = true),
    HomeGridButton("offline-packages", "OFFLINE\nPACKAGES", enabled = true),
    HomeGridButton("s5", "S5"),
    HomeGridButton("s6", "S6"),
    HomeGridButton("s7", "S7"),
    HomeGridButton("s8", "S8"),
    HomeGridButton("s9", "S9"),
)

internal data class ChartTrayOption(
    val id: String,
    val label: String,
    val launcherLabel: String,
    val available: Boolean,
    @DrawableRes val iconResId: Int? = null,
    val select: (() -> Unit)?,
)

internal data class TileRect(
    val leftPx: Int,
    val topPx: Int,
    val widthPx: Int,
    val heightPx: Int,
)

internal data class LoadedTileBitmap(
    val key: org.aerobag.app.domain.RenderTileKey,
    val bitmap: androidx.compose.ui.graphics.ImageBitmap?,
    val bytes: Int,
    val decodedBytes: Long,
    val readMs: Long,
    val decodeMs: Long,
)

internal data class LoadedRenderTileBitmap(
    val tile: org.aerobag.app.domain.RenderTile,
    val result: LoadedTileBitmap,
)

internal data class TileLoadWork(
    val generationId: Long,
    val mapId: String,
    val tile: org.aerobag.app.domain.RenderTile,
    val result: CompletableDeferred<LoadedRenderTileBitmap?>,
)

internal data class DecodedTileCacheEntry(
    val bitmap: androidx.compose.ui.graphics.ImageBitmap,
    val decodedBytes: Long,
)

internal class DecodedTileBitmapCache(
    private val maxBytes: Long,
) {
    private val entries = LinkedHashMap<String, DecodedTileCacheEntry>(256, 0.75f, true)
    private var currentBytes = 0L

    @Synchronized
    fun get(key: String): androidx.compose.ui.graphics.ImageBitmap? = entries[key]?.bitmap

    @Synchronized
    fun put(key: String, bitmap: androidx.compose.ui.graphics.ImageBitmap, decodedBytes: Long) {
        val previous = entries.remove(key)
        if (previous != null) {
            currentBytes -= previous.decodedBytes
        }
        entries[key] = DecodedTileCacheEntry(bitmap, decodedBytes.coerceAtLeast(1L))
        currentBytes += decodedBytes.coerceAtLeast(1L)
        trimToBudget()
    }

    @Synchronized
    fun clear() {
        entries.clear()
        currentBytes = 0L
    }

    @Synchronized
    fun stats(): DecodedTileCacheStats =
        DecodedTileCacheStats(entries = entries.size, bytes = currentBytes)

    private fun trimToBudget() {
        val iterator = entries.entries.iterator()
        while (currentBytes > maxBytes && iterator.hasNext()) {
            val eldest = iterator.next()
            currentBytes -= eldest.value.decodedBytes
            iterator.remove()
        }
    }
}

internal data class DecodedTileCacheStats(
    val entries: Int,
    val bytes: Long,
)

internal data class OverlaySurfaceUnits(
    val width: Float,
    val height: Float,
)

internal fun mergeRecentAirportIds(
    airports: List<ChartAirport>,
    storedIds: List<String>,
): List<String> {
    val validIds = airports.map { it.id }.toSet()
    val orderedIds = storedIds.filterIndexed { index, id ->
        validIds.contains(id) && storedIds.indexOf(id) == index
    }.toMutableList()
    airports.forEach { airport ->
        if (!orderedIds.contains(airport.id)) {
            orderedIds += airport.id
        }
    }
    return orderedIds
}

@DrawableRes
internal fun chartFamilyIconResId(chartFamily: MapChartFamily): Int = when (chartFamily) {
    MapChartFamily.Sec -> R.drawable.sectional_icon
    MapChartFamily.Tac -> R.drawable.tac_icon
    MapChartFamily.EnrL -> R.drawable.ifr_l_icon
    MapChartFamily.EnrH -> R.drawable.ifr_h_icon
    MapChartFamily.ShadedRelief -> R.drawable.shaded_relief_icon
    MapChartFamily.WorldBasemap -> R.drawable.shaded_relief_icon
}

internal fun chartFamilyId(chartFamily: MapChartFamily): String = when (chartFamily) {
    MapChartFamily.Sec -> "sec"
    MapChartFamily.Tac -> "tac"
    MapChartFamily.EnrL -> "enr-l"
    MapChartFamily.EnrH -> "enr-h"
    MapChartFamily.ShadedRelief -> "shaded-relief"
    MapChartFamily.WorldBasemap -> "world-basemap"
}

@DrawableRes
internal fun mapLayerIconResId(layerId: MapLayerId): Int = when (layerId) {
    MapLayerId.WorldBasemap -> R.drawable.shaded_relief_icon
    MapLayerId.Vectors -> R.drawable.layer_vectors_icon
    MapLayerId.Metars -> R.drawable.layer_nexrad_icon
    MapLayerId.Nexrad -> R.drawable.layer_nexrad_icon
    MapLayerId.TerrainWarning -> R.drawable.layer_terrain_warning_icon
    MapLayerId.OfflineRegions -> R.drawable.layer_vectors_icon
}

internal fun moveAirportToFront(
    currentIds: List<String>,
    airportId: String,
    airports: List<ChartAirport>,
): List<String> = mergeRecentAirportIds(airports, listOf(airportId) + currentIds.filterNot { it == airportId })

internal fun boundedHistory(history: List<AppViewSnapshot>): List<AppViewSnapshot> =
    if (history.size <= MaxViewHistoryDepth) history else history.takeLast(MaxViewHistoryDepth)

internal fun routeSegmentColor(uiTheme: UiTheme, status: RouteSegmentStatus): Color =
    when (status) {
        RouteSegmentStatus.Completed -> uiTheme.flightPlanRoute.completed
        RouteSegmentStatus.Active -> uiTheme.flightPlanRoute.active
        RouteSegmentStatus.ActiveLegRemaining -> uiTheme.flightPlanRoute.activeLegRemaining
        RouteSegmentStatus.Remaining -> uiTheme.flightPlanRoute.remaining
    }

internal fun latLonToScreenPoint(
    viewport: MapViewportState,
    point: LatLonPoint,
    widthPx: Float,
    heightPx: Float,
): Offset {
    val world = latLonToWorld(point.lat, point.lon)
    val scale = scaleForZoom(viewport.zoom)
    return Offset(
        x = ((world.x - viewport.centerWorldX) * scale + widthPx / 2f).toFloat(),
        y = ((world.y - viewport.centerWorldY) * scale + heightPx / 2f).toFloat(),
    )
}

internal fun plateFolderColor(uiTheme: UiTheme, category: String): Color =
    uiTheme.plateFolder.labelColors[category] ?: uiTheme.plateFolder.labelColors["other"] ?: Color(0xFF52656D)

internal fun createInitialSituationViewport(): MapViewportState {
    val center = latLonToWorld(VampsPosition.lat, VampsPosition.lon)
    return MapViewportState(
        centerWorldX = center.x,
        centerWorldY = center.y,
        zoom = 10.0,
    )
}

internal fun mapViewportFromCore(viewport: CoreMapViewport): MapViewportState {
    val center = latLonToWorld(viewport.center.lat, viewport.center.lon)
    return MapViewportState(
        centerWorldX = center.x,
        centerWorldY = center.y,
        zoom = viewport.zoom,
    )
}

internal fun sameMapViewport(left: MapViewportState, right: MapViewportState): Boolean =
    abs(left.centerWorldX - right.centerWorldX) < 1e-9 &&
        abs(left.centerWorldY - right.centerWorldY) < 1e-9 &&
        abs(left.zoom - right.zoom) < 1e-9

@Composable
internal fun SituationStatusBadge(
    controls: OwnshipControlModel,
    modifier: Modifier = Modifier,
    open: Boolean,
    onToggle: () -> Unit,
    onSelectSource: (String) -> Unit = {},
    onSituationControlInput: (SituationControlInput) -> Unit = {},
) {
    val trayColumnCount = max(controls.sources.size, controls.situationControls.size).coerceAtLeast(1)
    val trayWidth = (ThumbSize * trayColumnCount.toFloat()) + (3.dp * (trayColumnCount - 1).toFloat()) + 6.dp
    Box(modifier = modifier.wrapContentSize(unbounded = true, align = Alignment.TopEnd)) {
        MenuDock(
            launcherLabel = controls.launcherLabel,
            open = open,
            onToggle = onToggle,
            style = MenuDockStyle.Situation,
            trayWidthOverride = trayWidth,
            options = emptyList(),
            body = {
                SituationSourceRow(
                    sources = controls.sources,
                    onSelectSource = { sourceId ->
                        onSelectSource(sourceId)
                    },
                )
            },
            footer = {
                SituationTransportRow(
                    controls = controls.situationControls,
                    onInput = onSituationControlInput,
                )
            },
        )
    }
}

@Composable
internal fun DataStatusBadge(
    dataStatusState: UiDataStatusState,
    modifier: Modifier = Modifier,
    open: Boolean,
    onToggle: () -> Unit,
    onAction: (String) -> Unit = {},
) {
    val hasStatus = dataStatusState.boxes.isNotEmpty()
    if (!hasStatus) return

    val launcherLabel = dataStatusState.launcherCount?.let { "\u26A0 $it" } ?: "STATUS"
    val accentColor = statusSeverityColor(dataStatusState.launcherSeverity)
    Box(modifier = modifier.wrapContentSize(unbounded = true, align = Alignment.TopEnd)) {
        MenuDock(
            launcherLabel = launcherLabel,
            open = open,
            onToggle = onToggle,
            style = MenuDockStyle.DataStatus,
            options = listOf(
                MenuDockOption(
                    key = "status",
                    label = launcherLabel,
                    active = dataStatusState.launcherCount != null,
                    accentColor = accentColor,
                ) {},
            ),
            body = {
                dataStatusState.boxes.forEach { box ->
                    DataStatusBoxRow(
                        label = box.label,
                        value = box.value ?: "\u2014",
                        detail = box.detail,
                        severity = box.severity,
                        hushed = box.hushed,
                    )
                    if (box.actions.isNotEmpty()) {
                        Row(
                            horizontalArrangement = Arrangement.spacedBy(3.dp),
                            modifier = Modifier.fillMaxWidth(),
                        ) {
                            box.actions.forEach { action ->
                                CompactSquareButton(
                                    label = action.label.uppercase(),
                                    enabled = action.enabled,
                                    wide = true,
                                    modifier = Modifier
                                        .weight(1f)
                                        .height(ThumbSize * 0.72f),
                                    maxLines = 1,
                                    onClick = { onAction(action.id) },
                                )
                            }
                        }
                    }
                }
            },
        )
    }
}

@Composable
private fun DataStatusBoxRow(
    label: String,
    value: String,
    detail: String,
    severity: UiStatusSeverity,
    hushed: Boolean,
) {
    val uiTheme = LocalAerobagUiTheme.current
    val accentColor = statusSeverityColor(severity)
    val background = if (hushed) {
        uiTheme.controls.buttonBg.copy(alpha = 0.62f)
    } else {
        uiTheme.controls.buttonBg
    }
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(ThumbRadius))
            .background(background)
            .border(1.dp, accentColor.copy(alpha = if (hushed) 0.42f else 0.9f), RoundedCornerShape(ThumbRadius))
            .padding(8.dp),
        verticalArrangement = Arrangement.spacedBy(3.dp),
    ) {
        Row(
            horizontalArrangement = Arrangement.spacedBy(6.dp),
            verticalAlignment = Alignment.CenterVertically,
            modifier = Modifier.fillMaxWidth(),
        ) {
            Text(
                text = label,
                modifier = Modifier.weight(1f),
                style = MaterialTheme.typography.labelLarge,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
                color = uiTheme.controls.buttonFg,
            )
            Text(
                text = value,
                style = MaterialTheme.typography.labelLarge.copy(fontWeight = FontWeight.Bold),
                maxLines = 1,
                color = accentColor,
            )
        }
        Text(
            text = detail,
            style = MaterialTheme.typography.bodySmall,
            maxLines = 3,
            overflow = TextOverflow.Ellipsis,
            color = uiTheme.controls.buttonFg.copy(alpha = if (hushed) 0.68f else 0.9f),
        )
    }
}

private fun statusSeverityColor(severity: UiStatusSeverity): Color = when (severity) {
    UiStatusSeverity.Ok -> Color(0xFF7ED6A7)
    UiStatusSeverity.Info -> Color(0xFF8FB7FF)
    UiStatusSeverity.Caution -> Color(0xFFFFD35A)
    UiStatusSeverity.Warning -> Color(0xFFFF8B5A)
    UiStatusSeverity.Unavailable -> Color(0xFFB7BDC7)
}

@Composable
internal fun SituationSourceRow(
    sources: List<org.aerobag.app.domain.OwnshipSourceMenuItem>,
    onSelectSource: (String) -> Unit,
) {
    Row(
        horizontalArrangement = Arrangement.spacedBy(3.dp),
        modifier = Modifier.fillMaxWidth(),
    ) {
        sources.forEach { source ->
            CompactSquareButton(
                label = source.label,
                enabled = source.enabled,
                selected = source.active,
                wide = false,
                modifier = Modifier.size(ThumbSize),
                maxLines = 2,
                onClick = { onSelectSource(source.sourceId) },
            )
        }
    }
}

@Composable
internal fun SituationTransportRow(
    controls: List<org.aerobag.app.domain.SituationControlMenuItem>,
    onInput: (SituationControlInput) -> Unit,
) {
    Row(
        horizontalArrangement = Arrangement.spacedBy(3.dp),
        modifier = Modifier.fillMaxWidth(),
    ) {
        controls.forEach { control ->
            SituationTransportButton(control.label, control.input, control.enabled, onInput)
        }
    }
}

@Composable
internal fun SituationTransportButton(
    label: String,
    input: SituationControlInput,
    enabled: Boolean,
    onInput: (SituationControlInput) -> Unit,
) {
    CompactSquareButton(
        label = label,
        enabled = enabled,
        wide = false,
        modifier = Modifier
            .size(ThumbSize),
        onClick = { onInput(input) },
    )
}

internal fun resolveSituationOverlay(
    ownship: OwnshipRenderState,
    viewport: MapViewportState,
    widthUnits: Float,
    heightUnits: Float,
    ringCandidates: List<SituationRingCandidate>,
): SituationOverlay? {
    if (widthUnits <= 0f || heightUnits <= 0f) return null
    if (!ownship.drawAircraft) return null
    val current = ownship.position ?: return null
    val position = LatLon(current.lat, current.lon)
    val point = latLonToScreen(position.lat, position.lon, viewport, widthUnits, heightUnits)
    val heading = (ownship.orientationDeg ?: 0.0).toFloat()
    val predictor = ownship.speedKt?.takeIf { ownship.drawPredictor }?.let { speedKt ->
        val ahead = projectAhead(position.lat, position.lon, heading.toDouble(), speedKt / 60.0)
        latLonToScreen(ahead.lat, ahead.lon, viewport, widthUnits, heightUnits)
    }
    return SituationOverlay(
        pointUnits = point,
        headingDeg = heading,
        predictorUnits = predictor,
        ring = selectSituationRing(
            position,
            viewport,
            widthUnits,
            heightUnits,
            ringCandidates,
            ownship.magneticVariationDeg?.toFloat(),
        ),
    )
}

internal fun latLonToScreen(
    lat: Double,
    lon: Double,
    viewport: MapViewportState,
    widthUnits: Float,
    heightUnits: Float,
): Offset {
    val world = latLonToWorld(lat, lon)
    val scale = scaleForZoom(viewport.zoom)
    return Offset(
        x = (((world.x - viewport.centerWorldX) * scale) + widthUnits / 2f).toFloat(),
        y = (((world.y - viewport.centerWorldY) * scale) + heightUnits / 2f).toFloat(),
    )
}

internal fun mercatorMetersToWorld(xMeters: Double, yMeters: Double): Offset {
    val worldSpanMeters = WebMercatorHalfWorldM * 2.0
    return Offset(
        x = (((xMeters + WebMercatorHalfWorldM) / worldSpanMeters) * WebMercatorWorldSize).toFloat(),
        y = (((WebMercatorHalfWorldM - yMeters) / worldSpanMeters) * WebMercatorWorldSize).toFloat(),
    )
}

internal fun worldToScreen(
    viewport: MapViewportState,
    world: Offset,
    widthUnits: Float,
    heightUnits: Float,
): Offset {
    val scale = scaleForZoom(viewport.zoom)
    return Offset(
        x = ((world.x - viewport.centerWorldX) * scale + widthUnits / 2f).toFloat(),
        y = ((world.y - viewport.centerWorldY) * scale + heightUnits / 2f).toFloat(),
    )
}

internal fun screenToWorldOffset(
    viewport: MapViewportState,
    screenX: Float,
    screenY: Float,
    widthUnits: Float,
    heightUnits: Float,
): Offset {
    val scale = scaleForZoom(viewport.zoom)
    return Offset(
        x = (((screenX - widthUnits / 2f) / scale) + viewport.centerWorldX).toFloat(),
        y = (((screenY - heightUnits / 2f) / scale) + viewport.centerWorldY).toFloat(),
    )
}

internal fun terrainAltitudeBucketForOwnship(ownship: OwnshipRenderState): Double? = null

internal class RasterTileBitmapLoader(
    private val context: Context,
    scope: CoroutineScope,
    workerCount: Int = MapTileLoadWorkerCount,
) {
    private val workerThreadIds = AtomicInteger()
    private val workerDispatcher = Executors.newFixedThreadPool(workerCount) { task ->
        Thread(task, "AerobagRasterTile-${workerThreadIds.incrementAndGet()}").apply {
            isDaemon = true
        }
    }.asCoroutineDispatcher()
    private val workerScope = CoroutineScope(SupervisorJob(scope.coroutineContext[Job]) + workerDispatcher)
    private val closed = AtomicBoolean(false)
    private val latestGenerationId = AtomicLong()
    private val queueSignal = Channel<Unit>(capacity = Channel.UNLIMITED)
    private val queueMutex = Mutex()
    private val pendingWork = ArrayDeque<TileLoadWork>()

    init {
        repeat(workerCount) { workerIndex ->
            workerScope.launch {
                Log.i(TileBudgetLogTag, "worker-start worker=$workerIndex")
                try {
                    while (true) {
                        if (queueSignal.receiveCatching().isClosed) {
                            break
                        }
                        while (true) {
                            val work = queueMutex.withLock {
                                while (pendingWork.isNotEmpty() && pendingWork.first().generationId != latestGenerationId.get()) {
                                    pendingWork.removeFirst().result.complete(null)
                                }
                                pendingWork.removeFirstOrNull()
                            } ?: break
                            if (work.generationId != latestGenerationId.get()) {
                                work.result.complete(null)
                                continue
                            }
                            try {
                                currentCoroutineContext().ensureActive()
                                val workerStartMs = SystemClock.elapsedRealtime()
                                val result = loadOneVisibleTileBitmap(context, work.mapId, work.generationId, work.tile)
                                val workerElapsedMs = SystemClock.elapsedRealtime() - workerStartMs
                                if (workerElapsedMs >= SlowTileLoadLogMs) {
                                    Log.w(
                                        TileBudgetLogTag,
                                        "tile-slow gen=${work.generationId} worker=$workerIndex elapsedMs=$workerElapsedMs loaded=${result.bitmap != null} bytes=${result.bytes} readMs=${result.readMs} decodeMs=${result.decodeMs} ${formatTileRef(work.tile)}",
                                    )
                                }
                                if (work.generationId == latestGenerationId.get()) {
                                    work.result.complete(LoadedRenderTileBitmap(work.tile, result))
                                } else {
                                    work.result.complete(null)
                                }
                            } catch (error: CancellationException) {
                                work.result.cancel()
                                throw error
                            } catch (error: Throwable) {
                                Log.e(TileBudgetLogTag, "worker failed worker=$workerIndex gen=${work.generationId} ${formatTileRef(work.tile)}", error)
                                work.result.complete(null)
                            }
                        }
                    }
                } finally {
                    Log.w(TileBudgetLogTag, "worker-stop worker=$workerIndex")
                }
            }
        }
    }

    fun close() {
        if (!closed.compareAndSet(false, true)) {
            return
        }
        queueSignal.close()
        workerScope.coroutineContext[Job]?.cancel()
        workerDispatcher.close()
    }

    suspend fun loadVisibleTileBitmaps(
        mapId: String,
        generationId: Long,
        missingTiles: List<org.aerobag.app.domain.RenderTile>,
        onTileLoaded: suspend (LoadedRenderTileBitmap) -> Unit = {},
    ): List<LoadedRenderTileBitmap> {
        if (missingTiles.isEmpty()) {
            return emptyList()
        }
        if (closed.get()) {
            return emptyList()
        }
        latestGenerationId.set(generationId)
        val batchStartMs = SystemClock.elapsedRealtime()
        val deferredResults = missingTiles.map { tile ->
            TileLoadWork(
                generationId = generationId,
                mapId = mapId,
                tile = tile,
                result = CompletableDeferred(),
            )
        }
        Log.i(
            TileBudgetLogTag,
            "load-start gen=$generationId map=$mapId missing=${missingTiles.size} workers=$MapTileLoadWorkerCount groups=[${formatTileBudgetSummary(missingTiles)}] first=${missingTiles.firstOrNull()?.let(::formatTileRef) ?: "none"}",
        )
        try {
            val droppedCount = queueMutex.withLock {
                val dropped = pendingWork.size
                while (pendingWork.isNotEmpty()) {
                    pendingWork.removeFirst().result.complete(null)
                }
                pendingWork.addAll(deferredResults)
                dropped
            }
            repeat(MapTileLoadWorkerCount) {
                if (queueSignal.trySend(Unit).isFailure) {
                    return emptyList()
                }
            }
            Log.i(
                TileBudgetLogTag,
                "load-enqueued gen=$generationId map=$mapId count=${missingTiles.size} droppedQueued=$droppedCount enqueueMs=${SystemClock.elapsedRealtime() - batchStartMs}",
            )
            val loadedTiles = mutableListOf<LoadedRenderTileBitmap>()
            deferredResults.forEach { work ->
                currentCoroutineContext().ensureActive()
                val loaded = work.result.await() ?: return@forEach
                loadedTiles += loaded
                onTileLoaded(loaded)
            }
            return loadedTiles
        } catch (error: CancellationException) {
            deferredResults.forEach { work ->
                work.result.cancel()
            }
            Log.w(
                TileBudgetLogTag,
                "load-cancel gen=$generationId map=$mapId missing=${missingTiles.size} elapsedMs=${SystemClock.elapsedRealtime() - batchStartMs}",
            )
            throw error
        }
    }
}

internal suspend fun loadOneVisibleTileBitmap(
    context: Context,
    mapId: String,
    generationId: Long,
    tile: org.aerobag.app.domain.RenderTile,
): LoadedTileBitmap {
    val key = renderTileKey(tile)
    return try {
        val readStartMs = SystemClock.elapsedRealtime()
        val bytes = SectionalPackages.loadTileBytes(context, tile)
        val readMs = SystemClock.elapsedRealtime() - readStartMs
        currentCoroutineContext().ensureActive()
        if (bytes == null) {
            LoadedTileBitmap(key, null, 0, 0L, readMs, 0L)
        } else {
            val decodeStartMs = SystemClock.elapsedRealtime()
            val bitmap = BitmapFactory.decodeByteArray(bytes, 0, bytes.size)
            val decodeMs = SystemClock.elapsedRealtime() - decodeStartMs
            currentCoroutineContext().ensureActive()
            LoadedTileBitmap(key, bitmap?.asImageBitmap(), bytes.size, bitmap?.byteCount?.toLong() ?: 0L, readMs, decodeMs)
        }
    } catch (error: CancellationException) {
        throw error
    } catch (error: Throwable) {
        Log.w(
            TileBudgetLogTag,
            "tile load failed gen=$generationId map=$mapId ${formatTileRef(tile)}",
            error,
        )
        LoadedTileBitmap(key, null, 0, 0L, 0L, 0L)
    }
}

internal fun packTerrainTileBytes(tileBytesList: List<ByteArray>): ByteArray {
    val totalBytes = 4 + tileBytesList.sumOf { 4 + it.size }
    val buffer = ByteBuffer.allocate(totalBytes).order(java.nio.ByteOrder.LITTLE_ENDIAN)
    buffer.putInt(tileBytesList.size)
    tileBytesList.forEach { bytes ->
        buffer.putInt(bytes.size)
        buffer.put(bytes)
    }
    return buffer.array()
}

internal fun parseTerrainRawRgba(bytes: ByteArray): androidx.compose.ui.graphics.ImageBitmap {
    require(bytes.size >= 4) { "terrain raw RGBA payload missing header" }
    val header = ByteBuffer.wrap(bytes, 0, 4).order(java.nio.ByteOrder.LITTLE_ENDIAN)
    val width = header.short.toInt() and 0xFFFF
    val height = header.short.toInt() and 0xFFFF
    val expectedBytes = 4 + width * height * 4
    require(bytes.size >= expectedBytes) {
        "terrain raw RGBA payload truncated: expected $expectedBytes, got ${bytes.size}"
    }
    val bitmap = android.graphics.Bitmap.createBitmap(width, height, android.graphics.Bitmap.Config.ARGB_8888)
    bitmap.copyPixelsFromBuffer(ByteBuffer.wrap(bytes, 4, width * height * 4))
    return bitmap.asImageBitmap()
}

internal fun formatNexradObservedTime(value: String): String =
    runCatching {
        java.time.Instant.parse(value).toString().substring(11, 16)
    }.getOrElse { value }

internal fun formatTileBudgetSummary(
    tiles: List<org.aerobag.app.domain.RenderTile>,
): String {
    val counts = linkedMapOf<String, Int>()
    tiles.forEach { tile ->
        val packageLabel = tile.sources.firstOrNull()?.packageName ?: tile.mapViewId
        val key = "$packageLabel@z${tile.zoom}"
        counts[key] = (counts[key] ?: 0) + 1
    }
    return counts.entries
        .sortedBy { it.key }
        .joinToString(", ") { entry -> "${entry.key}=${entry.value}" }
}

internal fun formatTileRef(tile: org.aerobag.app.domain.RenderTile): String =
    "package=${tile.sources.firstOrNull()?.packageName ?: tile.mapViewId} storage=${tile.sources.firstOrNull()?.storageKind} z=${tile.zoom} x=${tile.x} y=${tile.yTms} candidates=${tile.sources.size}"

internal fun decodedTileCacheKey(tile: org.aerobag.app.domain.RenderTile): String {
    val candidates = tile.sources
        .distinctBy { "${it.packageName}:${it.storageKind}:${it.path}" }
        .joinToString("|") { source ->
            "${source.packageName}:${source.storageKind}:${source.path}"
        }
    return "${tile.zoom}:${tile.x}:${tile.yTms}:$candidates"
}

internal fun projectAhead(lat: Double, lon: Double, bearingDeg: Double, distanceNm: Double): LatLon {
    val angularDistance = distanceNm / 3440.065
    val bearing = Math.toRadians(bearingDeg)
    val startLat = Math.toRadians(lat)
    val startLon = Math.toRadians(lon)
    val nextLat = kotlin.math.asin(
        kotlin.math.sin(startLat) * kotlin.math.cos(angularDistance) +
            kotlin.math.cos(startLat) * kotlin.math.sin(angularDistance) * kotlin.math.cos(bearing),
    )
    val nextLon = startLon + atan2(
        sin(bearing) * sin(angularDistance) * kotlin.math.cos(startLat),
        kotlin.math.cos(angularDistance) - kotlin.math.sin(startLat) * kotlin.math.sin(nextLat),
    )
    return LatLon(Math.toDegrees(nextLat), Math.toDegrees(nextLon))
}

internal fun selectSituationRing(
    position: LatLon,
    viewport: MapViewportState,
    widthUnits: Float,
    heightUnits: Float,
    ringCandidates: List<SituationRingCandidate>,
    magneticVariationDeg: Float?,
): SituationRing {
    val center = latLonToScreen(position.lat, position.lon, viewport, widthUnits, heightUnits)
    val smaller = minOf(widthUnits, heightUnits)
    val minDiameter = smaller * 0.5f
    val maxDiameter = smaller * 0.8f
    val targetDiameter = smaller * 0.65f
    val best = ringCandidates
        .map { candidate ->
            val edge = projectAhead(position.lat, position.lon, 90.0, candidate.radiusNm)
            val edgePoint = latLonToScreen(edge.lat, edge.lon, viewport, widthUnits, heightUnits)
            val radiusUnits = hypot(edgePoint.x - center.x, edgePoint.y - center.y)
            val diameterUnits = radiusUnits * 2f
            val outOfBounds = when {
                diameterUnits < minDiameter -> minDiameter - diameterUnits
                diameterUnits > maxDiameter -> diameterUnits - maxDiameter
                else -> 0f
            }
            val score = if (outOfBounds > 0f) 10000f + outOfBounds else kotlin.math.abs(diameterUnits - targetDiameter)
            Triple(candidate, radiusUnits, score)
        }
        .minBy { it.third }
    val labelPoint = pointOnCircle(center, best.second + 16f, -45f)
    return SituationRing(
        radiusUnits = best.second,
        tickMarks = magneticVariationDeg?.let { buildSituationTickMarks(center, best.second, it) }.orEmpty(),
        cardinalLabels = magneticVariationDeg?.let { buildSituationCardinalLabels(center, best.second, it) }.orEmpty(),
        labelPointUnits = labelPoint,
        labelRotationDeg = 45f,
        labelText = best.first.label,
    )
}

internal fun buildSituationTickMarks(center: Offset, radiusUnits: Float, magneticVariationDeg: Float): List<SituationTickMark> =
    List(12) { index ->
        val angle = index * 30f + magneticVariationDeg
        SituationTickMark(
            innerUnits = pointOnCircle(center, radiusUnits - 14f, angle),
            outerUnits = pointOnCircle(center, radiusUnits, angle),
        )
    }

internal fun buildSituationCardinalLabels(center: Offset, radiusUnits: Float, magneticVariationDeg: Float): List<SituationCardinalLabel> {
    val labelRadius = maxOf(0f, radiusUnits - 30f)
    return listOf(
        Triple("N", -90f, 0f),
        Triple("E", 0f, 90f),
        Triple("S", 90f, 0f),
        Triple("W", 180f, -90f),
    ).map { (text, angleDeg, rotationDeg) ->
        SituationCardinalLabel(
            text = text,
            pointUnits = pointOnCircle(center, labelRadius, angleDeg + magneticVariationDeg),
            rotationDeg = rotationDeg,
        )
    }
}

internal fun pointOnCircle(center: Offset, radiusUnits: Float, angleDeg: Float): Offset {
    val radians = Math.toRadians(angleDeg.toDouble())
    return Offset(
        x = center.x + (radiusUnits * cos(radians)).toFloat(),
        y = center.y + (radiusUnits * sin(radians)).toFloat(),
    )
}

internal fun arrowShaftEndPoint(from: Offset, to: Offset): Offset {
    val angle = atan2(to.y - from.y, to.x - from.x)
    val headLength = 14f
    return Offset(
        x = to.x - headLength * cos(angle),
        y = to.y - headLength * sin(angle),
    )
}

internal fun arrowHeadPath(from: Offset, to: Offset): Path {
    val angle = atan2(to.y - from.y, to.x - from.x)
    val size = 20f
    val left = Offset(
        x = to.x - size * cos(angle - Math.PI.toFloat() / 6f),
        y = to.y - size * sin(angle - Math.PI.toFloat() / 6f),
    )
    val right = Offset(
        x = to.x - size * cos(angle + Math.PI.toFloat() / 6f),
        y = to.y - size * sin(angle + Math.PI.toFloat() / 6f),
    )
    return Path().apply {
        moveTo(to.x, to.y)
        lineTo(left.x, left.y)
        lineTo(right.x, right.y)
        close()
    }
}

internal fun transformVisibleFeature(
    feature: org.aerobag.app.domain.VisibleMapFeature,
    fromViewport: MapViewportState,
    fromSurface: OverlaySurfaceUnits,
    toViewport: MapViewportState,
    toSurface: OverlaySurfaceUnits,
): org.aerobag.app.domain.VisibleMapFeature {
    val transformed = transformScreenPoint(
        x = feature.screenX,
        y = feature.screenY,
        fromViewport = fromViewport,
        fromSurface = fromSurface,
        toViewport = toViewport,
        toSurface = toSurface,
    )
    return feature.copy(screenX = transformed.x, screenY = transformed.y)
}

internal fun transformMapOverlayForDisplay(
    overlay: MapOverlayQueryResult,
    fromViewport: MapViewportState?,
    fromSurface: OverlaySurfaceUnits?,
    toViewport: MapViewportState,
    toSurface: OverlaySurfaceUnits,
): MapOverlayQueryResult {
    if (fromViewport == null ||
        fromSurface == null ||
        fromSurface.width <= 0f ||
        fromSurface.height <= 0f ||
        toSurface.width <= 0f ||
        toSurface.height <= 0f
    ) {
        return overlay
    }
    return overlay.copy(
        visibleFeatures = overlay.visibleFeatures.map { feature ->
            transformVisibleFeature(feature, fromViewport, fromSurface, toViewport, toSurface)
        },
        visibleMetars = overlay.visibleMetars.map { feature ->
            transformVisibleMetarFeature(feature, fromViewport, fromSurface, toViewport, toSurface)
        },
        visiblePireps = overlay.visiblePireps.map { feature ->
            transformVisiblePirepFeature(feature, fromViewport, fromSurface, toViewport, toSurface)
        },
        airspacePaths = overlay.airspacePaths.map { feature ->
            transformAirspaceDisplayPath(feature, fromViewport, fromSurface, toViewport, toSurface)
        },
        tfrPaths = overlay.tfrPaths.map { feature ->
            transformAirspaceDisplayPath(feature, fromViewport, fromSurface, toViewport, toSurface)
        },
        airspaceLabels = overlay.airspaceLabels.map { label ->
            transformAirspaceDisplayLabel(label, fromViewport, fromSurface, toViewport, toSurface)
        },
    )
}

internal fun transformVisibleMetarFeature(
    feature: VisibleMetarFeature,
    fromViewport: MapViewportState,
    fromSurface: OverlaySurfaceUnits,
    toViewport: MapViewportState,
    toSurface: OverlaySurfaceUnits,
): VisibleMetarFeature {
    val transformed = transformScreenPoint(
        x = feature.screenX,
        y = feature.screenY,
        fromViewport = fromViewport,
        fromSurface = fromSurface,
        toViewport = toViewport,
        toSurface = toSurface,
    )
    return feature.copy(screenX = transformed.x, screenY = transformed.y)
}

internal fun transformVisiblePirepFeature(
    feature: VisiblePirepFeature,
    fromViewport: MapViewportState,
    fromSurface: OverlaySurfaceUnits,
    toViewport: MapViewportState,
    toSurface: OverlaySurfaceUnits,
): VisiblePirepFeature {
    val transformed = transformScreenPoint(
        x = feature.screenX,
        y = feature.screenY,
        fromViewport = fromViewport,
        fromSurface = fromSurface,
        toViewport = toViewport,
        toSurface = toSurface,
    )
    return feature.copy(screenX = transformed.x, screenY = transformed.y)
}

internal fun transformAirspaceDisplayLabel(
    label: AirspaceDisplayLabel,
    fromViewport: MapViewportState,
    fromSurface: OverlaySurfaceUnits,
    toViewport: MapViewportState,
    toSurface: OverlaySurfaceUnits,
): AirspaceDisplayLabel {
    val transformed = transformScreenPoint(
        x = label.screenX,
        y = label.screenY,
        fromViewport = fromViewport,
        fromSurface = fromSurface,
        toViewport = toViewport,
        toSurface = toSurface,
    )
    return label.copy(screenX = transformed.x, screenY = transformed.y)
}

internal fun transformAirspaceDisplayPath(
    feature: AirspaceDisplayPath,
    fromViewport: MapViewportState,
    fromSurface: OverlaySurfaceUnits,
    toViewport: MapViewportState,
    toSurface: OverlaySurfaceUnits,
): AirspaceDisplayPath =
    feature.copy(
        paths = feature.paths.map { subpath ->
            transformAirspaceDisplaySubpath(subpath, fromViewport, fromSurface, toViewport, toSurface)
        },
        decorations = feature.decorations.map { decoration ->
            transformAirspaceDisplayDecoration(decoration, fromViewport, fromSurface, toViewport, toSurface)
        },
    )

internal fun transformAirspaceDisplayDecoration(
    decoration: AirspaceDisplayDecoration,
    fromViewport: MapViewportState,
    fromSurface: OverlaySurfaceUnits,
    toViewport: MapViewportState,
    toSurface: OverlaySurfaceUnits,
): AirspaceDisplayDecoration =
    decoration.copy(
        paths = decoration.paths.map { subpath ->
            transformAirspaceDisplaySubpath(subpath, fromViewport, fromSurface, toViewport, toSurface)
        },
        segments = decoration.segments.map { segment ->
            transformAirspaceDisplayDecorationSegment(
                segment,
                fromViewport,
                fromSurface,
                toViewport,
                toSurface,
            )
        },
    )

internal fun transformAirspaceDisplayDecorationSegment(
    segment: AirspaceDisplayDecorationSegment,
    fromViewport: MapViewportState,
    fromSurface: OverlaySurfaceUnits,
    toViewport: MapViewportState,
    toSurface: OverlaySurfaceUnits,
): AirspaceDisplayDecorationSegment {
    val start = transformScreenPoint(
        x = segment.x1,
        y = segment.y1,
        fromViewport = fromViewport,
        fromSurface = fromSurface,
        toViewport = toViewport,
        toSurface = toSurface,
    )
    val end = transformScreenPoint(
        x = segment.x2,
        y = segment.y2,
        fromViewport = fromViewport,
        fromSurface = fromSurface,
        toViewport = toViewport,
        toSurface = toSurface,
    )
    return segment.copy(x1 = start.x, y1 = start.y, x2 = end.x, y2 = end.y)
}

internal fun transformAirspaceDisplaySubpath(
    subpath: AirspaceDisplaySubpath,
    fromViewport: MapViewportState,
    fromSurface: OverlaySurfaceUnits,
    toViewport: MapViewportState,
    toSurface: OverlaySurfaceUnits,
): AirspaceDisplaySubpath =
    subpath.copy(
        points = subpath.points.map { point ->
            val transformed = transformScreenPoint(
                x = point.x,
                y = point.y,
                fromViewport = fromViewport,
                fromSurface = fromSurface,
                toViewport = toViewport,
                toSurface = toSurface,
            )
            AirspaceScreenPoint(transformed.x, transformed.y)
        },
    )

internal fun transformScreenPoint(
    x: Double,
    y: Double,
    fromViewport: MapViewportState,
    fromSurface: OverlaySurfaceUnits,
    toViewport: MapViewportState,
    toSurface: OverlaySurfaceUnits,
): AirspaceScreenPoint {
    val world =
        screenToWorld(
            viewport = fromViewport,
            point = ScreenPoint(x.toFloat(), y.toFloat()),
            widthPx = fromSurface.width,
            heightPx = fromSurface.height,
        )
    val nextScale = scaleForZoom(toViewport.zoom)
    return AirspaceScreenPoint(
        x = (world.x - toViewport.centerWorldX) * nextScale + toSurface.width / 2.0,
        y = (world.y - toViewport.centerWorldY) * nextScale + toSurface.height / 2.0,
    )
}

internal fun readRecentAirportIds(context: Context): List<String> =
    context.getSharedPreferences(UiPrefsName, Context.MODE_PRIVATE)
        .getString(UiPrefsRecentAirportsKey, "")
        .orEmpty()
        .split('\n')
        .map(String::trim)
        .filter(String::isNotEmpty)

internal fun writeUiPrefs(
    context: Context,
    page: AppPage,
    selectedAirportId: String,
    selectedChartId: String,
    recentAirportIds: List<String>,
) {
    context.getSharedPreferences(UiPrefsName, Context.MODE_PRIVATE)
        .edit()
        .putString(UiPrefsPageKey, page.name)
        .putString(UiPrefsSelectedAirportKey, selectedAirportId)
        .putString(UiPrefsSelectedChartKey, selectedChartId)
        .putString(UiPrefsRecentAirportsKey, recentAirportIds.joinToString("\n"))
        .apply()
}

internal fun readStoredPage(prefs: SharedPreferences): AppPage {
    val stored = prefs.getString(UiPrefsPageKey, AppPage.Map.name) ?: AppPage.Map.name
    return if (stored == "Settings") {
        AppPage.Home
    } else {
        runCatching { AppPage.valueOf(stored) }.getOrDefault(AppPage.Map)
    }
}

internal fun summarizeRuntimeBootstrapFailure(error: Throwable): String {
    val messages = generateSequence(error) { it.cause }
        .mapNotNull { it.message?.trim() }
        .filter { it.isNotEmpty() }
        .toList()
    messages.firstOrNull { it.contains("no readable installed nav-db package") }?.let { detail ->
        return "No usable NAV DB is installed. Sync will fetch a compatible NAV DB and clean up rejected package files.\n\n$detail"
    }
    val chain = generateSequence(error) { it.cause }
        .mapNotNull { throwable ->
            val detail = throwable.message?.trim().orEmpty()
            if (detail.isEmpty()) {
                throwable::class.simpleName
            } else {
                "${throwable::class.simpleName}: $detail"
            }
        }
        .distinct()
        .toList()
    return if (chain.isEmpty()) {
        "Runtime bootstrap failed."
    } else {
        "Runtime bootstrap failed: ${chain.joinToString(" <- ")}"
    }
}

class MainActivity : ComponentActivity() {
    var onHardwareZoomDelta: ((Double) -> Boolean)? = null
    var onSituationControlInput: ((SituationControlInput) -> Boolean)? = null

    private val gpsPermissionLauncher =
        registerForActivityResult(ActivityResultContracts.RequestMultiplePermissions()) { grants ->
            if (grants[Manifest.permission.ACCESS_FINE_LOCATION] == true) {
                startAndroidGpsService()
            } else {
                AndroidGpsSource.publishStatus(AndroidGpsSource.unavailableStatus("Precise location required"))
            }
        }

    @OptIn(ExperimentalComposeUiApi::class)
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        requestAndroidGps()
        setContent {
            MaterialTheme {
                Surface(
                    modifier = Modifier
                        .fillMaxSize()
                        .semantics { testTagsAsResourceId = true },
                    color = Color(0xFFF3EFE4),
                ) {
                    AerobagApp()
                }
            }
        }
    }

    override fun dispatchKeyEvent(event: AndroidKeyEvent): Boolean {
        if (event.action == AndroidKeyEvent.ACTION_DOWN) {
            val situationInput = situationControlInputForKeyEvent(event)
            if (situationInput != null && (onSituationControlInput?.invoke(situationInput) == true)) {
                return true
            }
            val delta = when (event.keyCode) {
                AndroidKeyEvent.KEYCODE_EQUALS,
                AndroidKeyEvent.KEYCODE_PLUS,
                AndroidKeyEvent.KEYCODE_NUMPAD_ADD -> 0.35
                AndroidKeyEvent.KEYCODE_MINUS,
                AndroidKeyEvent.KEYCODE_NUMPAD_SUBTRACT -> -0.35
                else -> null
            }
            if (delta != null && (onHardwareZoomDelta?.invoke(delta) == true)) {
                return true
            }
        }
        return super.dispatchKeyEvent(event)
    }

    private fun requestAndroidGps() {
        if (hasPreciseLocationPermission()) {
            startAndroidGpsService()
            return
        }
        gpsPermissionLauncher.launch(requiredGpsPermissions())
    }

    private fun hasPreciseLocationPermission(): Boolean =
        ContextCompat.checkSelfPermission(this, Manifest.permission.ACCESS_FINE_LOCATION) == PackageManager.PERMISSION_GRANTED

    private fun requiredGpsPermissions(): Array<String> =
        buildList {
            add(Manifest.permission.ACCESS_FINE_LOCATION)
            add(Manifest.permission.ACCESS_COARSE_LOCATION)
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
                add(Manifest.permission.POST_NOTIFICATIONS)
            }
        }.toTypedArray()

    private fun startAndroidGpsService() {
        AndroidGpsSource.publishStatus(AndroidGpsSource.searchingStatus())
        ContextCompat.startForegroundService(this, Intent(this, AndroidGpsService::class.java))
    }
}

internal fun situationControlInputForKeyEvent(event: AndroidKeyEvent): SituationControlInput? =
    when (event.unicodeChar.takeIf { it != 0 }?.toChar()) {
        '<' -> SituationControlInput.SkipBackward
        '(' -> SituationControlInput.FastRewind
        ')' -> SituationControlInput.FastForward
        '>' -> SituationControlInput.SkipForward
        else -> null
    }

@Composable
internal fun AerobagApp() {
    val context = LocalContext.current
    val prefs = remember(context) { context.applicationContext.getSharedPreferences(UiPrefsName, Context.MODE_PRIVATE) }
    val bootstrap = remember(context) { AndroidRuntimeContent.loadBootstrap(context.applicationContext) }
    var runtimeReloadToken by remember { mutableStateOf(0) }
    val offlinePackagesControllerHandle = remember(prefs) { initialOfflinePackagesControllerHandle(prefs) }
    DisposableEffect(offlinePackagesControllerHandle) {
        onDispose { NativeBindings.destroyOfflinePackagesController(offlinePackagesControllerHandle) }
    }
    val uiTheme = remember(context) { UiThemeLoader.load(context.applicationContext) }
    val runtimeFixture by produceState<Result<org.aerobag.app.domain.RuntimeContent>?>(initialValue = null, context, bootstrap, runtimeReloadToken) {
        value = withContext(Dispatchers.IO) {
            runCatching { AndroidRuntimeContent.loadInstalledRuntime(context.applicationContext, bootstrap) }
        }
    }
    var keepOfflinePackagesVisible by remember { mutableStateOf(false) }
    var runtimeFailureMessage by remember { mutableStateOf<String?>(null) }
    LaunchedEffect(runtimeFixture) {
        when {
            runtimeFixture?.isFailure == true -> {
                keepOfflinePackagesVisible = true
                val error = runtimeFixture?.exceptionOrNull() ?: return@LaunchedEffect
                val message = summarizeRuntimeBootstrapFailure(error)
                runtimeFailureMessage = message
                Log.e("AerobagRuntime", message, error)
            }
            runtimeFixture?.isSuccess == true -> {
                runtimeFailureMessage = null
            }
        }
    }
    if (runtimeFixture == null) {
        CompositionLocalProvider(LocalAerobagUiTheme provides uiTheme) {
            HomePage(
                page = AppPage.Home,
                pageHistory = emptyList(),
                uptimeLabel = rememberUptimeLabel(SystemClock.elapsedRealtime()),
                bootstrap = bootstrap,
                debugState = defaultUiDebugState(),
                navElement = null,
                onSelectPage = {},
                onOpenPlan = {},
                initialOfflinePackagesOpen = keepOfflinePackagesVisible,
                forceOfflinePackagesOpen = keepOfflinePackagesVisible,
                bootstrapMessage = runtimeFailureMessage
                    ?: if (keepOfflinePackagesVisible) {
                        "Opening runtime..."
                    } else {
                        "Loading..."
                    },
                offlinePackagesControllerHandle = offlinePackagesControllerHandle,
                onRuntimeMaybeAvailable = { runtimeReloadToken += 1 },
            )
        }
        return
    }
    if (runtimeFixture!!.isFailure) {
        keepOfflinePackagesVisible = true
        CompositionLocalProvider(LocalAerobagUiTheme provides uiTheme) {
            HomePage(
                page = AppPage.Home,
                pageHistory = emptyList(),
                uptimeLabel = rememberUptimeLabel(SystemClock.elapsedRealtime()),
                bootstrap = bootstrap,
                debugState = defaultUiDebugState(),
                navElement = null,
                onSelectPage = {},
                onOpenPlan = {},
                initialOfflinePackagesOpen = true,
                forceOfflinePackagesOpen = true,
                bootstrapMessage = runtimeFailureMessage
                    ?: "Runtime bootstrap failed. Refresh library, then sync required packages in Offline Packages to continue.",
                offlinePackagesControllerHandle = offlinePackagesControllerHandle,
                onRuntimeMaybeAvailable = { runtimeReloadToken += 1 },
            )
        }
        return
    }
    val fixture = runtimeFixture!!.getOrThrow()
    val appCore = remember(fixture.navKvStore) {
        NativeAppCoreAdapter(
            navKvStore = fixture.navKvStore,
        )
    }
    val situationRingCandidates = remember(appCore) { appCore.situationRingCandidates() }
    val initialPlan = remember(appCore) { appCore.emptyFlightPlan() }
    val sessionStartElapsedMs = remember { SystemClock.elapsedRealtime() }
    val uptimeLabel = rememberUptimeLabel(sessionStartElapsedMs)
    val storedRecentAirportIds = remember { readRecentAirportIds(context.applicationContext) }
    val storedSelectedAirportId = remember { prefs.getString(UiPrefsSelectedAirportKey, null).orEmpty() }
    val storedSelectedChartId = remember { prefs.getString(UiPrefsSelectedChartKey, null).orEmpty() }
    var page by remember {
        mutableStateOf(
            if (keepOfflinePackagesVisible) {
                AppPage.Home
            } else {
                readStoredPage(prefs)
            },
        )
    }
    var pageHistory by remember { mutableStateOf<List<AppViewSnapshot>>(emptyList()) }
    val uiSession = remember(appCore) {
        appCore.createUiSession(
            initialPlan,
            storedRecentAirportIds,
            storedSelectedAirportId.ifBlank { null },
            storedSelectedChartId.ifBlank { null },
        )
    }
    val initialRasterMapState = remember(uiSession) {
        requireNotNull(uiSession.snapshot.rasterMap) {
            "core session did not provide raster map state"
        }
    }
    var rasterMapState by remember(uiSession) { mutableStateOf(initialRasterMapState) }
    var selectedMapId by remember(uiSession) { mutableStateOf(initialRasterMapState.selectedMapId) }
    DisposableEffect(uiSession) {
        onDispose { uiSession.destroy() }
    }
    var sessionSnapshot by remember(uiSession) { mutableStateOf(uiSession.snapshot) }
    LaunchedEffect(uiSession, sessionSnapshot.nextCycleProductFreshnessCheckEpochMs) {
        val nextCheckEpochMs = sessionSnapshot.nextCycleProductFreshnessCheckEpochMs ?: return@LaunchedEffect
        val delayMs = (nextCheckEpochMs - System.currentTimeMillis())
            .coerceAtLeast(0L)
        delay(delayMs)
        sessionSnapshot = uiSession.refreshSnapshot()
    }
    val liveFeedCache = remember(uiSession, context) {
        LiveFeedCacheStore.open(context.applicationContext)
    }
    DisposableEffect(liveFeedCache) {
        onDispose { liveFeedCache.close() }
    }
    var liveFeedGeneration by remember(uiSession) { mutableIntStateOf(0) }
    fun promoteLiveFeed(summary: LiveFeedInstalledSummary) {
        runCatching {
            uiSession.installLiveFeedCacheProduct(liveFeedCache, summary.product)
        }.onSuccess {
            sessionSnapshot = it
            liveFeedGeneration += 1
            Log.i(
                "AndroidLiveFeeds",
                "promoted product=${summary.product} version=${summary.version} generation=$liveFeedGeneration",
            )
        }.onFailure { error ->
            Log.w("AndroidLiveFeeds", "failed to promote ${summary.product}/${summary.version}", error)
        }
    }
    LaunchedEffect(uiSession, liveFeedCache, context, prefs) {
        val appContext = context.applicationContext
        withContext(Dispatchers.IO) {
            LiveFeedCacheStore.listInstalled(appContext).map { it.summary }
        }.forEach { promoteLiveFeed(it) }
        val sourceRootUrl = resolveLiveFeedSourceRootUrl(
            appContext,
            prefs,
            androidDevServerBaseUrl(),
        )
        AndroidLiveFeedClient(
            context = appContext,
            cache = liveFeedCache,
            sourceRootUrl = sourceRootUrl,
            policy = LiveFeedFetchPolicy.UnmeteredOrLocal,
        ).bootstrapAndRun(
            promote = { summary ->
                withContext(Dispatchers.Main) {
                    promoteLiveFeed(summary)
                }
            },
            onChanged = {},
        )
    }
    DisposableEffect(uiSession, context) {
        val activity = context as? MainActivity
        activity?.onSituationControlInput = { input ->
            sessionSnapshot = uiSession.applySituationControlInput(input, System.currentTimeMillis().toDouble())
            true
        }
        onDispose {
            if (activity?.onSituationControlInput != null) {
                activity.onSituationControlInput = null
            }
        }
    }
    val appState = sessionSnapshot.appState
    val appUiState = sessionSnapshot.appUiState
    val currentPlan = appState.activePlan ?: initialPlan
    var derivedChartPageState by remember(uiSession) {
        mutableStateOf(
            DerivedChartPageState(
                airports = emptyList<ChartAirport>(),
                recentAirportIds = sessionSnapshot.chartPageState.recentAirportIds,
                selectedAirportId = sessionSnapshot.chartPageState.selectedAirportId,
                selectedChartId = sessionSnapshot.chartPageState.selectedChartId,
            ),
        )
    }
    val selectedMap = rasterMapState
    var mapViewport by remember { mutableStateOf(createInitialSituationViewport()) }
    var chartViewport by remember { mutableStateOf<org.aerobag.app.domain.ImageViewportState?>(null) }
    var chartFolderOpen by remember { mutableStateOf(false) }
    var pageTilePaintTiming by remember { mutableStateOf<PageTilePaintTiming?>(null) }
    var nextPageTilePaintTimingId by remember { mutableStateOf(1L) }
    var debugPanelOpen by remember { mutableStateOf(false) }
    val decodedTileBitmapCache = remember(fixture.navKvStore) { DecodedTileBitmapCache(DecodedTileCacheMaxBytes) }
    var playbackSourcePath by remember { mutableStateOf(DefaultPlaybackTracePath) }
    val planListState = rememberLazyListState()
    val chartAirportById = remember(derivedChartPageState.airports) { derivedChartPageState.airports.associateBy { it.id } }
    val orderedChartAirports = remember(derivedChartPageState.airports) { derivedChartPageState.airports }
    val recentAirportIds = derivedChartPageState.recentAirportIds
    val selectedAirportId = derivedChartPageState.selectedAirportId
    val selectedChartId = derivedChartPageState.selectedChartId
    val selectedAirport = remember(selectedAirportId, orderedChartAirports) {
        orderedChartAirports.find { it.id == selectedAirportId } ?: orderedChartAirports.firstOrNull()
    }
    val selectedChart = remember(selectedAirport, selectedChartId) {
        selectedAirport?.charts?.find { it.id == selectedChartId } ?: selectedAirport?.charts?.firstOrNull()
    }

    LaunchedEffect(page, selectedAirportId, selectedChartId, recentAirportIds) {
        writeUiPrefs(context.applicationContext, page, selectedAirportId, selectedChartId, recentAirportIds)
    }
    LaunchedEffect(
        appCore,
        currentPlan,
        sessionSnapshot.chartPageState.recentAirportIds,
        sessionSnapshot.chartPageState.selectedAirportId,
        sessionSnapshot.chartPageState.selectedChartId,
    ) {
        derivedChartPageState =
            appCore.deriveChartPageState(
                currentPlan,
                sessionSnapshot.chartPageState.recentAirportIds,
                sessionSnapshot.chartPageState.selectedAirportId.ifBlank { null },
                sessionSnapshot.chartPageState.selectedChartId.ifBlank { null },
            )
    }
    LaunchedEffect(uiSession) {
        sessionSnapshot = uiSession.registerOwnshipSource(AndroidGpsSource.registration())
        sessionSnapshot = uiSession.updateOwnshipSourceStatus(AndroidGpsSource.status.value)
        launch {
            AndroidGpsSource.status.collect { status ->
                sessionSnapshot = uiSession.updateOwnshipSourceStatus(status)
            }
        }
        launch {
            AndroidGpsSource.samples.collect { sample ->
                sessionSnapshot = uiSession.pushSituationSample(sample)
            }
        }
    }
    LaunchedEffect(uiSession, sessionSnapshot.playbackUiState.status) {
        while (sessionSnapshot.playbackUiState.status == PlaybackStatus.Playing) {
            delay(250)
            runCatching { uiSession.tickPlayback(System.currentTimeMillis().toDouble()) }
                .onSuccess {
                    sessionSnapshot = it
                }
                .onFailure { Log.e("AerobagPlayback", "tick failed", it) }
        }
    }
    val sessionPlanUiState = requireNotNull(sessionSnapshot.appUiState.activePlan) {
        "UiSessionSnapshot must provide active flight-plan UI state"
    }
    val navElement = sessionPlanUiState.guidance?.navElement

    LaunchedEffect(selectedMap.selectedMapId) {
        mapViewport = preserveViewportForMap(mapViewport, selectedMap.minZoom, selectedMap.maxZoom)
    }

    fun currentSnapshot(): AppViewSnapshot = AppViewSnapshot(
        page = page,
        selectedMapId = selectedMapId,
        selectedMapLauncherLabel =
            rasterMapState.selectedFamilyLauncherLabel,
        mapViewport = mapViewport,
        selectedAirportId = selectedAirportId,
        selectedChartId = selectedChartId,
        selectedChartLabel = selectedChart?.label.orEmpty(),
        recentAirportIds = recentAirportIds,
        chartViewport = chartViewport,
        chartFolderOpen = chartFolderOpen,
    )

    fun restoreSnapshot(snapshot: AppViewSnapshot, history: List<AppViewSnapshot>) {
        if (snapshot.selectedAirportId.isNotBlank() || snapshot.selectedChartId.isNotBlank() || snapshot.recentAirportIds.isNotEmpty()) {
            sessionSnapshot =
                uiSession.restoreChartPageState(
                    recentAirportIds = snapshot.recentAirportIds,
                    selectedAirportId = snapshot.selectedAirportId.ifBlank { null },
                    selectedChartId = snapshot.selectedChartId.ifBlank { null },
                )
        }
        pageHistory = history
        page = snapshot.page
        runCatching {
            val nextSnapshot =
                if (snapshot.selectedMapId.isBlank()) {
                    uiSession.refreshSnapshot()
                } else {
                    uiSession.selectRasterMap(snapshot.selectedMapId)
                }
            sessionSnapshot = nextSnapshot
            val nextRasterMapState = requireNotNull(nextSnapshot.rasterMap) {
                "core session returned no raster map state"
            }
            rasterMapState = nextRasterMapState
            selectedMapId = nextRasterMapState.selectedMapId
        }
        mapViewport = snapshot.mapViewport
        chartViewport = snapshot.chartViewport
        chartFolderOpen = snapshot.chartFolderOpen
    }

    fun navigateToPage(nextPage: AppPage) {
        Log.i("AerobagNavigation", "navigate request from=$page to=$nextPage history=${pageHistory.size}")
        if (nextPage == page) {
            Log.i("AerobagNavigation", "navigate ignored same-page page=$page")
            return
        }
        if (nextPage == AppPage.Map) {
            pageTilePaintTiming = PageTilePaintTiming(
                id = nextPageTilePaintTimingId++,
                fromPage = page,
                startedMs = SystemClock.elapsedRealtime(),
                trigger = "page-to-map",
            )
            Log.i(TileBudgetLogTag, "tile-paint-start id=${pageTilePaintTiming?.id} trigger=${pageTilePaintTiming?.trigger} from=$page")
        }
        pageHistory = boundedHistory(pageHistory + currentSnapshot())
        page = nextPage
        Log.i("AerobagNavigation", "navigate committed page=$page history=${pageHistory.size}")
    }

    fun pushViewSnapshot(snapshot: AppViewSnapshot) {
        restoreSnapshot(snapshot, boundedHistory(pageHistory + currentSnapshot()))
    }

    fun navigateToMostRecentChartOrPlate() {
        val target =
            pageHistory
                .asReversed()
                .firstOrNull { it.page == AppPage.Map || it.page == AppPage.Charts }
        if (target != null) {
            pushViewSnapshot(target)
        } else {
            navigateToPage(AppPage.Map)
        }
    }

    fun setDebugFlag(flagId: String, enabled: Boolean) {
        sessionSnapshot = uiSession.setDebugFlag(flagId, enabled)
    }

    fun openChartsForAirport(airportId: String) {
        sessionSnapshot = uiSession.selectAirport(airportId)
        val airport = chartAirportById[airportId]
        restoreSnapshot(
            currentSnapshot().copy(
                page = AppPage.Charts,
                selectedAirportId = airportId,
                selectedChartId = airport?.charts?.firstOrNull()?.id.orEmpty(),
                selectedChartLabel = airport?.charts?.firstOrNull()?.label.orEmpty(),
                recentAirportIds = sessionSnapshot.chartPageState.recentAirportIds,
                chartViewport = null,
                chartFolderOpen = true,
            ),
            boundedHistory(pageHistory + currentSnapshot()),
        )
    }

    BackHandler(enabled = pageHistory.isNotEmpty()) {
        val previous = pageHistory.lastOrNull() ?: return@BackHandler
        restoreSnapshot(previous, pageHistory.dropLast(1))
    }

    CompositionLocalProvider(LocalAerobagUiTheme provides uiTheme) {
        Box(modifier = Modifier.fillMaxSize()) {
            when (page) {
                AppPage.Map -> {
                    MapExplorerPage(
                        appCore = appCore,
                        page = page,
                        pageHistory = pageHistory,
                        uptimeLabel = uptimeLabel,
                        uiSession = uiSession,
                        sessionSnapshot = sessionSnapshot,
                        liveFeedGeneration = liveFeedGeneration,
                        uiTheme = uiTheme,
                        ownship = appUiState.ownship.render,
                        flightDataBanner = appUiState.flightDataBanner,
                        playbackUiState = sessionSnapshot.playbackUiState,
                        playbackSourcePath = playbackSourcePath,
                        mapFollowUiState = sessionSnapshot.mapFollowUiState,
                        mapFollowTargetViewport = sessionSnapshot.mapFollowTargetViewport,
                        situationRingCandidates = situationRingCandidates,
                        selectedMap = selectedMap,
                        mapFamilyOptions = rasterMapState.familyOptions,
                        viewport = mapViewport,
                        decodedTileBitmapCache = decodedTileBitmapCache,
                        debugState = sessionSnapshot.debugState,
                        pageTilePaintTiming = pageTilePaintTiming,
                        ownshipControls = appUiState.ownship.controls,
                        onPageTilePaintTimingComplete = { completedId ->
                            if (pageTilePaintTiming?.id == completedId) {
                                pageTilePaintTiming = null
                            }
                        },
                        onViewportChange = { mapViewport = it },
                        onSessionSnapshotChange = { sessionSnapshot = it },
                        onSelectOwnshipSource = { sourceId ->
                            sessionSnapshot = uiSession.selectOwnshipSource(OwnshipSelection.Source(sourceId))
                        },
                        onSituationControlInput = { input ->
                            sessionSnapshot = uiSession.applySituationControlInput(input, System.currentTimeMillis().toDouble())
                        },
                        onPlaybackSourcePathChange = { playbackSourcePath = it },
                        onSelectMapFamily = {
                            val timingId = nextPageTilePaintTimingId++
                            val clickStartMs = SystemClock.elapsedRealtime()
                            pageTilePaintTiming = PageTilePaintTiming(
                                id = timingId,
                                fromPage = page,
                                startedMs = SystemClock.elapsedRealtime(),
                                trigger = "map-family:${chartFamilyId(it)}",
                            )
                            Log.i(TileBudgetLogTag, "map-family-click id=$timingId family=${chartFamilyId(it)}")
                            pageHistory = boundedHistory(pageHistory + currentSnapshot())
                            page = AppPage.Map
                            val selectStartMs = SystemClock.elapsedRealtime()
                            val nextSnapshot = uiSession.selectMapFamily(it)
                            Log.i(
                                TileBudgetLogTag,
                                "map-family-select-core id=$timingId family=${chartFamilyId(it)} elapsedMs=${SystemClock.elapsedRealtime() - selectStartMs}",
                            )
                            val nextRasterMapState = requireNotNull(nextSnapshot.rasterMap) {
                                "core selectMapFamily returned no raster map state"
                            }
                            rasterMapState = nextRasterMapState
                            selectedMapId = nextRasterMapState.selectedMapId
                            sessionSnapshot = nextSnapshot
                            Log.i(
                                TileBudgetLogTag,
                                "map-family-click-done id=$timingId family=${chartFamilyId(it)} elapsedMs=${SystemClock.elapsedRealtime() - clickStartMs}",
                            )
                        },
                        onSelectPage = ::navigateToPage,
                        onOpenPlan = { navigateToPage(AppPage.Plan) },
                        navElement = navElement,
                        plan = currentPlan,
                        planUiState = sessionPlanUiState,
                    )
                }
                AppPage.Plan -> {
                    FlightPlanPage(
                        appCore = appCore,
                        uiSession = uiSession,
                        page = page,
                        pageHistory = pageHistory,
                        mostRecentChartOrPlatePage = mostRecentChartOrPlatePageFromHistory(pageHistory),
                        uptimeLabel = uptimeLabel,
                        navElement = navElement,
                        planUiState = sessionPlanUiState,
                        planListState = planListState,
                        plan = currentPlan,
                        uiTheme = uiTheme,
                        onSelectPage = ::navigateToPage,
                        onOpenRecentChartOrPlate = ::navigateToMostRecentChartOrPlate,
                        onOpenCharts = { airportId -> if (airportId != null) openChartsForAirport(airportId) },
                        onApplySessionSnapshot = { snapshot ->
                            sessionSnapshot = snapshot
                        },
                    )
                }
                AppPage.Charts -> {
                    ChartsPage(
                        page = page,
                        pageHistory = pageHistory,
                        uptimeLabel = uptimeLabel,
                        airports = orderedChartAirports,
                        selectedAirport = selectedAirport,
                        selectedChart = selectedChart,
                        plan = currentPlan,
                        uiTheme = uiTheme,
                        ownship = appUiState.ownship.render,
                        ownshipControls = appUiState.ownship.controls,
                        flightDataBanner = appUiState.flightDataBanner,
                        uiSession = uiSession,
                        navElement = navElement,
                        folderOpen = chartFolderOpen,
                        viewport = chartViewport,
                        onViewportChange = { chartViewport = it },
                        onSessionSnapshotChange = { sessionSnapshot = it },
                        onFolderOpenChange = {
                            restoreSnapshot(
                                currentSnapshot().copy(
                                    page = AppPage.Charts,
                                    chartFolderOpen = it,
                                ),
                                boundedHistory(pageHistory + currentSnapshot()),
                            )
                        },
                        onSelectPage = ::navigateToPage,
                        onOpenPlan = { navigateToPage(AppPage.Plan) },
                        onSelectAirport = { airportId ->
                            sessionSnapshot = uiSession.selectAirport(airportId)
                            val airport = chartAirportById[airportId]
                            restoreSnapshot(
                                currentSnapshot().copy(
                                    page = AppPage.Charts,
                                    selectedAirportId = airportId,
                                    selectedChartId = airport?.charts?.firstOrNull()?.id.orEmpty(),
                                    selectedChartLabel = airport?.charts?.firstOrNull()?.label.orEmpty(),
                                    recentAirportIds = sessionSnapshot.chartPageState.recentAirportIds,
                                    chartViewport = null,
                                    chartFolderOpen = false,
                                ),
                                boundedHistory(pageHistory + currentSnapshot()),
                            )
                        },
                        onSelectChart = {
                            sessionSnapshot = uiSession.selectChart(it)
                            restoreSnapshot(
                                currentSnapshot().copy(
                                    page = AppPage.Charts,
                                    selectedChartId = it,
                                    selectedChartLabel = chartAirportById[sessionSnapshot.chartPageState.selectedAirportId]
                                        ?.charts
                                        ?.firstOrNull { chart -> chart.id == it }
                                        ?.label
                                        .orEmpty(),
                                    chartViewport = null,
                                    chartFolderOpen = false,
                                ),
                                boundedHistory(pageHistory + currentSnapshot()),
                            )
                        },
                    )
                }
                AppPage.Home -> {
                    Log.i("AerobagNavigation", "render home history=${pageHistory.size}")
                    HomePage(
                        page = page,
                        pageHistory = pageHistory,
                        mostRecentChartOrPlatePage = mostRecentChartOrPlatePageFromHistory(pageHistory),
                        uptimeLabel = uptimeLabel,
                        bootstrap = bootstrap,
                        debugState = sessionSnapshot.debugState,
                        navElement = navElement,
                        onSelectPage = ::navigateToPage,
                        onOpenPlan = { navigateToPage(AppPage.Plan) },
                        onOpenRecentChartOrPlate = ::navigateToMostRecentChartOrPlate,
                        initialOfflinePackagesOpen = keepOfflinePackagesVisible,
                        offlinePackagesControllerHandle = offlinePackagesControllerHandle,
                        onOfflinePackagesClosed = { keepOfflinePackagesVisible = false },
                        onRuntimeMaybeAvailable = { runtimeReloadToken += 1 },
                    )
                }
            }
            DebugDock(
                open = debugPanelOpen,
                onToggle = { debugPanelOpen = !debugPanelOpen },
                expandAbove = true,
                modifier = Modifier
                    .zIndex(OverlayPlaneControls)
                    .align(Alignment.BottomEnd)
                    .padding(end = ThumbSize + (ThumbSize * 0.1f)),
            ) {
                CommonDebugPanel(
                    uptimeLabel = uptimeLabel,
                    debugState = sessionSnapshot.debugState,
                    onDebugFlagChange = ::setDebugFlag,
                )
            }
        }
    }
}
