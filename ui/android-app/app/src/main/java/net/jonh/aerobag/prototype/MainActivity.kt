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
import androidx.compose.animation.core.animateFloatAsState
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
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
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
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.produceState
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.runtime.withFrameNanos
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Shadow
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.PathEffect
import androidx.compose.ui.graphics.PathFillType
import androidx.compose.ui.graphics.TransformOrigin
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.toArgb
import androidx.compose.ui.graphics.drawscope.clipRect
import androidx.compose.ui.graphics.drawscope.rotate
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.nativeCanvas
import androidx.compose.ui.graphics.lerp
import androidx.compose.ui.ExperimentalComposeUiApi
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
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
import androidx.compose.ui.platform.LocalSoftwareKeyboardController
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.withStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.PlatformImeOptions
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.IntSize
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.zIndex
import androidx.compose.ui.window.Popup
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
import net.jonh.aerobag.prototype.domain.ConcretizedNavItem
import net.jonh.aerobag.prototype.domain.CoreMapViewport
import net.jonh.aerobag.prototype.domain.DerivedChartPageState
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
import net.jonh.aerobag.prototype.domain.MapView
import net.jonh.aerobag.prototype.domain.MapViewOption
import net.jonh.aerobag.prototype.domain.MapViewportState
import net.jonh.aerobag.prototype.domain.NativeAppCoreAdapter
import net.jonh.aerobag.prototype.domain.NativeBindings
import net.jonh.aerobag.prototype.domain.NativeUiSession
import net.jonh.aerobag.prototype.domain.NavKvStore
import net.jonh.aerobag.prototype.domain.NavRef
import net.jonh.aerobag.prototype.domain.NavElementUiView
import net.jonh.aerobag.prototype.domain.OwnshipMode
import net.jonh.aerobag.prototype.domain.OwnshipRenderState
import net.jonh.aerobag.prototype.domain.PackageZipStore
import net.jonh.aerobag.prototype.domain.PlaybackStatus
import net.jonh.aerobag.prototype.domain.PlaybackUiState
import net.jonh.aerobag.prototype.domain.ProcedureKind
import net.jonh.aerobag.prototype.domain.ProcedureOptions
import net.jonh.aerobag.prototype.domain.ProcedureSummary
import net.jonh.aerobag.prototype.domain.ResolvedLeg
import net.jonh.aerobag.prototype.domain.ResolvedLegSource
import net.jonh.aerobag.prototype.domain.RenderTile
import net.jonh.aerobag.prototype.domain.RouteSegmentStatus
import net.jonh.aerobag.prototype.domain.RouteComponentUiView
import net.jonh.aerobag.prototype.domain.RouteComponentViewKind
import net.jonh.aerobag.prototype.domain.RouteComponent
import net.jonh.aerobag.prototype.domain.ScreenPoint
import net.jonh.aerobag.prototype.domain.SectionalPackages
import net.jonh.aerobag.prototype.domain.SampleData
import net.jonh.aerobag.prototype.domain.SequencingMode
import net.jonh.aerobag.prototype.domain.SituationRingCandidate
import net.jonh.aerobag.prototype.domain.TileStorageKind
import net.jonh.aerobag.prototype.domain.UiMapLayerToggleState
import net.jonh.aerobag.prototype.domain.UiTheme
import net.jonh.aerobag.prototype.domain.UiThemeLoader
import net.jonh.aerobag.prototype.domain.UiSessionSnapshot
import net.jonh.aerobag.prototype.domain.VisibleMapFeature
import net.jonh.aerobag.prototype.domain.VisibleMetarFeature
import net.jonh.aerobag.prototype.domain.applyPinchGesture
import net.jonh.aerobag.prototype.domain.clampZoom
import net.jonh.aerobag.prototype.domain.createInitialImageViewport
import net.jonh.aerobag.prototype.domain.createInitialViewport
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

private val LocalAerobagUiTheme = staticCompositionLocalOf<UiTheme> {
    error("Aerobag UI theme not provided")
}

private val ThumbSize = 56.dp
private val ThumbGap = 5.6.dp
private val PlanGridGap = 2.dp
private const val DefaultPlaybackTracePath = "/adsb-traces/n550ar/n550ar-2024-09-29.json"
private const val DefaultAndroidDevServerBaseUrl = "http://10.0.2.2:8080"
private const val DefaultAndroidPackageSourceBaseUrl = "http://10.0.2.2:8092"
private const val WebMercatorWorldSize = 256.0
private const val WebMercatorHalfWorldM = 20037508.342789244
private const val NexradFrameIntervalMs = 900L
private const val TerrainAltitudeBucketFt = 200
private const val MapLayerLogTag = "MapLayers"
private const val TileBudgetLogTag = "AerobagTileBudget"
private const val DecodedTileCacheMaxBytes = 96L * 1024L * 1024L
private const val MapTileLoadWorkerCount = 4
private const val SlowTileLoadLogMs = 1000L
private val TileLoadGenerationIds = AtomicLong()
private val VampsPosition = LatLon(47.3648944444444, -121.980275)

private data class PageTilePaintTiming(
    val id: Long,
    val fromPage: AppPage,
    val startedMs: Long,
)

@kotlinx.serialization.Serializable
private data class WireRasterTilePlan(
    val tiles: List<WireRasterTileDraw> = emptyList(),
)

@kotlinx.serialization.Serializable
private data class WireRasterTileDraw(
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
private data class WireRasterTileSource(
    val map_view_id: String,
)

private fun filterRenderableFamilyMapViews(
    selectedMap: MapViewOption,
    familyMapViews: List<MapViewOption>,
    viewport: MapViewportState,
): List<MapViewOption> =
    familyMapViews
        .groupBy { it.mapView.chartFamily }
        .values
        .flatMap { views ->
            val collapseBelowZoom = views.mapNotNull { it.mapView.fullCoverageZoom }.minOrNull()
            if (collapseBelowZoom == null || viewport.zoom > collapseBelowZoom || views.size <= 1) {
                views
            } else {
                listOf(
                    views.firstOrNull { it.regionId == selectedMap.regionId }
                        ?: views.first(),
                )
            }
        }
        .sortedWith(compareBy<MapViewOption> { it.mapView.chartFamily.name }.thenBy { it.id })

private data class LatLon(val lat: Double, val lon: Double)

@Serializable
private data class NexradManifest(
    @SerialName("schema_version")
    val schemaVersion: Int,
    @SerialName("version_label")
    val versionLabel: String,
    @SerialName("frame_count")
    val frameCount: Int,
    val projection: String,
    val frames: List<NexradFrame>,
)

@Serializable
private data class NexradFrame(
    val filename: String,
    @SerialName("observed_at_utc")
    val observedAtUtc: String,
    val width: Int,
    val height: Int,
    val bounds: NexradBounds,
)

@Serializable
private data class NexradBounds(
    val west: Double,
    val south: Double,
    val east: Double,
    val north: Double,
)

private data class NexradOverlayFrame(
    val frame: NexradFrame,
    val bitmap: androidx.compose.ui.graphics.ImageBitmap,
)

private data class TerrainOverlayImage(
    val key: String,
    val z: Int,
    val x: Int,
    val yTms: Int,
    val left: Double,
    val top: Double,
    val size: Double,
    val bitmap: androidx.compose.ui.graphics.ImageBitmap,
)

private data class SituationOverlay(
    val pointUnits: Offset,
    val headingDeg: Float,
    val predictorUnits: Offset?,
    val ring: SituationRing,
)

private data class SituationRing(
    val radiusUnits: Float,
    val tickMarks: List<SituationTickMark>,
    val cardinalLabels: List<SituationCardinalLabel>,
    val labelPointUnits: Offset,
    val labelRotationDeg: Float,
    val labelText: String,
)

private data class SituationTickMark(
    val innerUnits: Offset,
    val outerUnits: Offset,
)

private data class SituationCardinalLabel(
    val text: String,
    val pointUnits: Offset,
    val rotationDeg: Float,
)

private val ThumbRadius = 10.dp
private val FolderThumbGutter = ThumbSize * 0.3f
private val PlateFolderTileWidth = ThumbSize * 2f
private val PlateFolderTileHeight = ThumbSize * 3f
private val PlatePageTrayWidth = ThumbSize * 4f
private val PlanArrowLane = ThumbSize * 0.5f
private val PlanArrowButtonInset = 5.dp
private const val UiPrefsName = "aerobag_ui"
private const val UiPrefsPageKey = "page"
private const val UiPrefsSelectedAirportKey = "selected_airport_id"
private const val UiPrefsSelectedChartKey = "selected_chart_id"
private const val UiPrefsRecentAirportsKey = "recent_airport_ids"
private const val UiPrefsOfflinePackagePreferencesKey = "offline_package_preferences"
private const val UiPrefsPackageSourceBaseUrlKey = "package_source_base_url"
private const val MapViewportLogTag = "MapViewport"
private const val MaxViewHistoryDepth = 64
private const val OverlayPlaneModalScrim = 80f
private const val OverlayPlaneModal = 90f
private val PackageManagementJson = Json {
    encodeDefaults = true
    ignoreUnknownKeys = true
    classDiscriminator = "kind"
}

private enum class AppPage {
    Map,
    Plan,
    Charts,
    Home,
}

private data class AppViewSnapshot(
    val page: AppPage,
    val selectedMapId: String,
    val mapViewport: MapViewportState,
    val selectedAirportId: String,
    val selectedChartId: String,
    val selectedChartLabel: String,
    val recentAirportIds: List<String>,
    val chartViewport: net.jonh.aerobag.prototype.domain.ImageViewportState?,
    val chartFolderOpen: Boolean,
)

private data class MapSelectionUiState(
    val point: Offset,
    val result: MapSelectionQueryResult,
    val selectedItem: MapSelectionItem?,
)

private data class FlightPlanDisplayRow(
    val id: String,
    val selectionKey: String,
    val label: String,
    val rowKind: String,
    val componentKind: RouteComponentViewKind? = null,
    val componentIndex: Int? = null,
    val legIndex: Int? = null,
    val distanceNm: Double? = null,
    val courseDeg: Double? = null,
    val chartAirportId: String? = null,
    val navRef: NavRef? = null,
    val symbolFeature: net.jonh.aerobag.prototype.domain.NavSymbolFeature? = null,
    val depth: Int = 0,
    val active: Boolean = false,
    val canAddAirwayAfter: Boolean = false,
    val canAddProcedureBefore: Boolean = false,
    val canChangeAirway: Boolean = false,
    val canRemoveComponent: Boolean = false,
    val canReorderComponent: Boolean = false,
    val canReorderUp: Boolean = false,
    val canReorderDown: Boolean = false,
    val actions: List<FlightPlanRowActionUiView> = emptyList(),
    val startComponentIndex: Int? = null,
    val endComponentIndex: Int? = null,
    val originAnchor: NavRef? = null,
    val destinationAnchor: NavRef? = null,
)

private sealed interface FlightPlanDisplayBlock {
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

private data class StructuredArrowSpec(
    val fromPoint: Offset,
    val toPoint: Offset,
    val toClipped: Boolean,
    val fromClippedAbove: Boolean,
    val elbowX: Float,
    val shaftEndX: Float,
    val headLength: Float,
)

private data class StructuredArrowEndpoint(
    val point: Offset,
    val clipped: Boolean,
    val clippedAbove: Boolean,
    val clippedBelow: Boolean,
)

@Composable
private fun rememberStructuredRowBounds(
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

private data class AndroidAirwayPickerState(
    val loading: Boolean,
    val error: String?,
    val mode: String,
    val componentIndex: Int?,
    val startComponentIndex: Int?,
    val endComponentIndex: Int?,
    val originAnchor: NavRef,
    val destinationAnchor: NavRef?,
    val suggestions: List<AirwaySuggestion>,
    val selectedAirwayName: String?,
    val presentation: AirwayPresentationPlan?,
    val selectedEntryIndex: Int?,
)

private data class AndroidProcedurePickerState(
    val loading: Boolean,
    val error: String?,
    val airportId: String,
    val startComponentIndex: Int,
    val endComponentIndex: Int,
    val procedures: List<ProcedureSummary>,
    val selectedProcedureId: String?,
    val options: ProcedureOptions?,
)

private data class AndroidAirportInsertState(
    val componentIndex: Int,
    val before: Boolean,
    val airportId: String,
    val error: String?,
    val loading: Boolean,
    val suggestions: List<WaypointIdentifierSuggestion>,
)

private data class PageTrayOption(
    val page: AppPage,
    val label: String,
    val launcherLabel: String,
    @DrawableRes val iconResId: Int? = null,
)

private data class OfflinePackageDimension(
    val id: String,
    val label: String,
)

@Serializable
private enum class OfflinePackageSelection {
    @SerialName("unselected")
    Unselected,
    @SerialName("pause")
    Pause,
    @SerialName("play")
    Play,
}

@Serializable
private data class OfflinePackagePreferencesWire(
    val regions: Map<String, OfflinePackageSelection> = emptyMap(),
    val products: Map<String, OfflinePackageSelection> = emptyMap(),
)

@Serializable
private data class InstalledArtifactWire(
    @SerialName("artifact_id")
    val artifactId: String,
    val filename: String? = null,
    @SerialName("size_bytes")
    val sizeBytes: Long? = null,
    @SerialName("checksum_sha256")
    val checksumSha256: String? = null,
)


@Serializable
private data class OfflinePackagesUiRowWire(
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
private enum class OfflinePackagesUiPlanActionWire {
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
private data class OfflinePackagesUiPlanEntryWire(
    val action: OfflinePackagesUiPlanActionWire,
    val count: Int = 0,
    val cycles: List<String> = emptyList(),
)

@Serializable
private data class OfflinePackagesClockOptionWire(
    val id: String,
    val label: String,
    val active: Boolean = false,
)

@Serializable
private data class OfflinePackagesUiStateWire(
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
private data class PackageManagementPlanWire(
    val fetch: List<String> = emptyList(),
    @SerialName("retain_installed")
    val retainInstalled: List<String> = emptyList(),
    val gc: List<String> = emptyList(),
    @SerialName("protected_by_pause")
    val protectedByPause: List<String> = emptyList(),
)

@Serializable
private data class BundlePackageArtifactWire(
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
private data class BundleManifestWire(
    val packages: List<BundlePackageArtifactWire> = emptyList(),
)

@Serializable
private data class OfflinePackagesSyncSummary(
    @SerialName("fetched_count")
    val fetchedCount: Int,
    @SerialName("gc_count")
    val gcCount: Int,
    val warnings: List<OfflinePackagesWarning>,
    @SerialName("remote_poisoned_filename_messages")
    val remotePoisonedFilenameMessages: Map<String, String> = emptyMap(),
)

@Serializable
private data class OfflinePackagesSyncProgressWire(
    @SerialName("completed_fetch_artifact_ids")
    val completedFetchArtifactIds: Set<String> = emptySet(),
    @SerialName("current_fetch_artifact_id")
    val currentFetchArtifactId: String? = null,
    @SerialName("current_fetch_bytes")
    val currentFetchBytes: Long = 0,
)

@Serializable
private data class OfflinePackagesWarning(
    @SerialName("artifact_id")
    val artifactId: String,
    @SerialName("family_id")
    val familyId: String?,
    @SerialName("region_id")
    val regionId: String?,
    val message: String,
)

@Serializable
private data class OfflinePackagesReduceResultWire(
    val state: OfflinePackagesStateWire,
    @SerialName("ui_state")
    val uiState: OfflinePackagesUiStateWire,
    @SerialName("effective_now_epoch_ms")
    val effectiveNowEpochMs: Long,
    val plan: PackageManagementPlanWire,
    val bundle: BundleManifestWire,
)

@Serializable
private data class CurrentArtifactsManifestWire(
    @SerialName("as_of_utc")
    val asOfUtc: String? = null,
    val bundles: List<CurrentArtifactsBundleRefWire> = emptyList(),
)

@Serializable
private data class CurrentArtifactsBundleRefWire(
    val filename: String,
    @SerialName("bundle_type")
    val bundleType: String,
)

@Serializable
private data class OfflinePackagesControllerUiStateWire(
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
private sealed interface OfflinePackagesControllerCommandWire {
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
        val plan: PackageManagementPlanWire,
        val bundle: BundleManifestWire,
        @SerialName("max_parallel_fetches")
        val maxParallelFetches: Int = 4,
    ) : OfflinePackagesControllerCommandWire
}

@Serializable
private data class OfflinePackagesControllerResultWire(
    @SerialName("packages_state_json")
    val packagesStateJson: String? = null,
    @SerialName("ui_state")
    val uiState: OfflinePackagesControllerUiStateWire,
    val command: OfflinePackagesControllerCommandWire? = null,
)

@Serializable
private data class OfflinePackagesInitInputWire(
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
private data class OfflinePackagesEventWire(
    val kind: String,
    val id: String? = null,
    @SerialName("epoch_ms")
    val epochMs: Long? = null,
)

@Serializable
private data class OfflinePackagesReduceInputWire(
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
private sealed interface OfflinePackagesControllerEventWire {
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
private data class OfflinePackagesControllerInputWire(
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
private data class OfflinePackagesStateWire(
    val preferences: OfflinePackagePreferencesWire = OfflinePackagePreferencesWire(),
    @SerialName("now_override_epoch_ms")
    val nowOverrideEpochMs: Long? = null,
)

private data class HomeGridButton(
    val key: String,
    val label: String,
    val targetPage: AppPage? = null,
    val enabled: Boolean = false,
    @DrawableRes val iconResId: Int? = null,
)

private data class MenuDockOption(
    val key: String,
    val label: String,
    val active: Boolean = false,
    val enabled: Boolean = true,
    val accentColor: Color? = null,
    val toggleState: UiMapLayerToggleState? = null,
    @DrawableRes val iconResId: Int? = null,
    val onSelect: () -> Unit,
)

private enum class MenuDockStyle(
    val buttonWidth: androidx.compose.ui.unit.Dp,
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
        buttonWidth = ThumbSize * 3f,
        trayWidth = PlatePageTrayWidth,
        launcherMaxLines = 2,
    ),
    Layers(
        buttonWidth = ThumbSize,
        trayWidth = ThumbSize * 4f,
        launcherMaxLines = 2,
    ),
}

private val PageOptions = listOf(
    PageTrayOption(AppPage.Map, "CHART", "CHART", R.drawable.page_chart_icon),
    PageTrayOption(AppPage.Charts, "PLATE", "PLATE", R.drawable.page_plate_icon),
    PageTrayOption(AppPage.Plan, "FLIGHT PLAN", "PLAN", R.drawable.page_plan1_icon),
    PageTrayOption(AppPage.Home, "HOME", "HOME"),
)

private val OfflineProductOptions = listOf(
    OfflinePackageDimension("sec", "Sectional"),
    OfflinePackageDimension("tac", "TAC"),
    OfflinePackageDimension("shaded-relief", "Shaded Relief"),
    OfflinePackageDimension("enr-l", "IFR-L"),
    OfflinePackageDimension("enr-h", "IFR-H"),
    OfflinePackageDimension("tpp", "TPP"),
    OfflinePackageDimension("csup", "CSUP"),
)

private val HomeGridButtons = listOf(
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

private data class ChartTrayOption(
    val id: String,
    val label: String,
    val launcherLabel: String,
    val available: Boolean,
    @DrawableRes val iconResId: Int? = null,
    val select: (() -> Unit)?,
)

private data class TileRect(
    val leftPx: Int,
    val topPx: Int,
    val widthPx: Int,
    val heightPx: Int,
)

private data class LoadedTileBitmap(
    val key: net.jonh.aerobag.prototype.domain.RenderTileKey,
    val bitmap: androidx.compose.ui.graphics.ImageBitmap?,
    val bytes: Int,
    val decodedBytes: Long,
    val readMs: Long,
    val decodeMs: Long,
)

private data class LoadedRenderTileBitmap(
    val tile: net.jonh.aerobag.prototype.domain.RenderTile,
    val result: LoadedTileBitmap,
)

private data class TileLoadWork(
    val generationId: Long,
    val mapId: String,
    val tile: net.jonh.aerobag.prototype.domain.RenderTile,
    val result: CompletableDeferred<LoadedRenderTileBitmap?>,
)

private data class TileBitmapFallback(
    val bitmap: androidx.compose.ui.graphics.ImageBitmap,
    val sourceLevelDelta: Int,
    val sourceColumn: Int,
    val sourceRow: Int,
)

private data class ChildTileBitmapFallback(
    val bitmap: androidx.compose.ui.graphics.ImageBitmap,
    val targetLevelDelta: Int,
    val targetColumn: Int,
    val targetRow: Int,
)

private data class DecodedTileCacheEntry(
    val bitmap: androidx.compose.ui.graphics.ImageBitmap,
    val decodedBytes: Long,
)

private class DecodedTileBitmapCache(
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

private data class DecodedTileCacheStats(
    val entries: Int,
    val bytes: Long,
)

private data class OverlaySurfaceUnits(
    val width: Float,
    val height: Float,
)

private fun initialMapId(fixture: net.jonh.aerobag.prototype.domain.ContentFixture): String {
    return fixture.mapViews.firstOrNull {
        it.mapView.chartFamily == MapChartFamily.Tac
    }?.id ?: fixture.mapViews.first().id
}

private fun mergeRecentAirportIds(
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
private fun chartFamilyIconResId(chartFamily: MapChartFamily): Int = when (chartFamily) {
    MapChartFamily.Sec -> R.drawable.sectional_icon
    MapChartFamily.Tac -> R.drawable.tac_icon
    MapChartFamily.EnrL -> R.drawable.ifr_l_icon
    MapChartFamily.EnrH -> R.drawable.ifr_h_icon
    MapChartFamily.ShadedRelief -> R.drawable.shaded_relief_icon
}

@DrawableRes
private fun mapLayerIconResId(layerId: MapLayerId): Int = when (layerId) {
    MapLayerId.Vectors -> R.drawable.layer_vectors_icon
    MapLayerId.Metars -> R.drawable.layer_nexrad_icon
    MapLayerId.Nexrad -> R.drawable.layer_nexrad_icon
    MapLayerId.TerrainWarning -> R.drawable.layer_terrain_warning_icon
}

private fun moveAirportToFront(
    currentIds: List<String>,
    airportId: String,
    airports: List<ChartAirport>,
): List<String> = mergeRecentAirportIds(airports, listOf(airportId) + currentIds.filterNot { it == airportId })

private fun boundedHistory(history: List<AppViewSnapshot>): List<AppViewSnapshot> =
    if (history.size <= MaxViewHistoryDepth) history else history.takeLast(MaxViewHistoryDepth)

private fun resolveAirportId(
    airports: List<ChartAirport>,
    candidateAirportId: String?,
    recentAirportIds: List<String>,
): String {
    if (candidateAirportId != null && airports.any { it.id == candidateAirportId }) {
        return candidateAirportId
    }
    return recentAirportIds.firstOrNull() ?: airports.firstOrNull()?.id.orEmpty()
}

private fun resolveChartId(
    airports: List<ChartAirport>,
    airportId: String,
    candidateChartId: String?,
): String {
    val airport = airports.firstOrNull { it.id == airportId }
    if (candidateChartId != null && airport?.charts?.any { it.id == candidateChartId } == true) {
        return candidateChartId
    }
    return airport?.charts?.firstOrNull()?.id.orEmpty()
}

private fun routeSegmentColor(status: RouteSegmentStatus): Color =
    when (status) {
        RouteSegmentStatus.Completed -> Color(0xFF8C9DAD)
        RouteSegmentStatus.Active -> Color(0xFFFF4FCF)
        RouteSegmentStatus.Remaining -> Color.White
    }

private fun latLonToScreenPoint(
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

private fun plateFolderColor(uiTheme: UiTheme, category: String): Color =
    uiTheme.plateFolder.labelColors[category] ?: uiTheme.plateFolder.labelColors["other"] ?: Color(0xFF52656D)

private fun buildSeededDevPlan(
    adapter: NativeAppCoreAdapter,
    plan: net.jonh.aerobag.prototype.domain.FlightPlan,
): FlightPlanUiMutation {
    return runCatching {
        val waypoints = listOf(
            NavRef.Airport("KPAO"),
            NavRef.Fix("VPDUB"),
            NavRef.Airport("KVCB"),
            NavRef.Airport("KWLW"),
        )
        val resolvedLegs =
            waypoints.zipWithNext().mapIndexed { index, (from, to) ->
                ResolvedLeg(
                    id = "component-$index-${index + 1}",
                    from = from,
                    to = to,
                    source = ResolvedLegSource.RouteComponent(componentIndex = index),
                )
            }
        val seededPlan =
            plan.copy(
                id = "dev-kpao-vpdub-kvcb-kwlw",
                name = "KPAO VPDUB KVCB KWLW",
                legs = resolvedLegs.map { leg -> net.jonh.aerobag.prototype.domain.FlightPlanLeg(leg.from, leg.to, null) },
                routeComponents = waypoints.map { waypoint -> RouteComponent.Waypoint(waypoint) },
                resolvedLegs = resolvedLegs,
                guidance = GuidanceState(
                    activeLegIndex = 0,
                    sequencingMode = SequencingMode.FollowPlan,
                    directTo = null,
                ),
                departure = "KPAO",
                destination = "KWLW",
                updatedAtEpochMs = System.currentTimeMillis(),
                version = plan.version + 1,
            )
        adapter.activateLegUi(seededPlan, 0)
    }.getOrElse {
        Log.e("AerobagSeed", "buildSeededDevPlan fell back to sample plan", it)
        FlightPlanUiMutation(
            plan = plan,
            uiState = FlightPlanUiState(
                components = emptyList(),
                resolvedLegs = emptyList(),
                displayRows = emptyList(),
                guidance = null,
            ),
        )
    }
}

private fun createInitialSituationViewport(mapView: MapView): MapViewportState {
    val center = latLonToWorld(VampsPosition.lat, VampsPosition.lon)
    return MapViewportState(
        centerWorldX = center.x,
        centerWorldY = center.y,
        zoom = 10.0,
    )
}

private fun mapViewportFromCore(viewport: CoreMapViewport): MapViewportState {
    val center = latLonToWorld(viewport.center.lat, viewport.center.lon)
    return MapViewportState(
        centerWorldX = center.x,
        centerWorldY = center.y,
        zoom = viewport.zoom,
    )
}

private fun sameMapViewport(left: MapViewportState, right: MapViewportState): Boolean =
    abs(left.centerWorldX - right.centerWorldX) < 1e-9 &&
        abs(left.centerWorldY - right.centerWorldY) < 1e-9 &&
        abs(left.zoom - right.zoom) < 1e-9

@Composable
private fun SituationStatusBadge(
    ownship: OwnshipRenderState,
    modifier: Modifier = Modifier,
) {
    val tone = when (ownship.mode) {
        OwnshipMode.None -> Triple(ownship.bannerText, Color(0xFFB3261E), "unknown")
        OwnshipMode.Simulated -> Triple(ownship.bannerText, Color(0xFFB1591A), "simulated")
        OwnshipMode.Live,
        OwnshipMode.Replay,
        -> Triple(ownship.bannerText, Color(0xFF2A4F66), "live")
    }
    Surface(
        modifier = modifier,
        shape = RoundedCornerShape(ThumbSize * 0.22f),
        color = Color(0xE6FCF8F1),
        shadowElevation = 4.dp,
    ) {
        Text(
            text = tone.first,
            color = tone.second,
            style = MaterialTheme.typography.labelSmall,
            fontWeight = FontWeight.Bold,
            modifier = Modifier.padding(horizontal = ThumbSize * 0.18f, vertical = ThumbSize * 0.12f),
        )
    }
}

private fun resolveSituationOverlay(
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
        ring = selectSituationRing(position, viewport, widthUnits, heightUnits, ringCandidates),
    )
}

private fun latLonToScreen(
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

private fun mercatorMetersToWorld(xMeters: Double, yMeters: Double): Offset {
    val worldSpanMeters = WebMercatorHalfWorldM * 2.0
    return Offset(
        x = (((xMeters + WebMercatorHalfWorldM) / worldSpanMeters) * WebMercatorWorldSize).toFloat(),
        y = (((WebMercatorHalfWorldM - yMeters) / worldSpanMeters) * WebMercatorWorldSize).toFloat(),
    )
}

private fun worldToScreen(
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

private fun screenToWorldOffset(
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

private fun terrainAltitudeBucketForOwnship(ownship: OwnshipRenderState): Double? = null

private class RasterTileBitmapLoader(
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
                        queueSignal.receive()
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
        queueSignal.close()
        workerScope.coroutineContext[Job]?.cancel()
        workerDispatcher.close()
    }

    suspend fun loadVisibleTileBitmaps(
        mapId: String,
        generationId: Long,
        missingTiles: List<net.jonh.aerobag.prototype.domain.RenderTile>,
        onTileLoaded: suspend (LoadedRenderTileBitmap) -> Unit = {},
    ): List<LoadedRenderTileBitmap> {
        if (missingTiles.isEmpty()) {
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
                queueSignal.send(Unit)
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

private suspend fun loadOneVisibleTileBitmap(
    context: Context,
    mapId: String,
    generationId: Long,
    tile: net.jonh.aerobag.prototype.domain.RenderTile,
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

private fun packTerrainTileBytes(tileBytesList: List<ByteArray>): ByteArray {
    val totalBytes = 4 + tileBytesList.sumOf { 4 + it.size }
    val buffer = ByteBuffer.allocate(totalBytes).order(java.nio.ByteOrder.LITTLE_ENDIAN)
    buffer.putInt(tileBytesList.size)
    tileBytesList.forEach { bytes ->
        buffer.putInt(bytes.size)
        buffer.put(bytes)
    }
    return buffer.array()
}

private fun parseTerrainRawRgba(bytes: ByteArray): androidx.compose.ui.graphics.ImageBitmap {
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

private fun formatNexradObservedTime(value: String): String =
    runCatching {
        java.time.Instant.parse(value).toString().substring(11, 16)
    }.getOrElse { value }

private fun formatTileBudgetSummary(
    tiles: List<net.jonh.aerobag.prototype.domain.RenderTile>,
): String {
    val counts = linkedMapOf<String, Int>()
    tiles.forEach { tile ->
        val packageLabel = tile.mapView.packageName ?: tile.mapViewId
        val key = "$packageLabel@z${tile.zoom}"
        counts[key] = (counts[key] ?: 0) + 1
    }
    return counts.entries
        .sortedBy { it.key }
        .joinToString(", ") { entry -> "${entry.key}=${entry.value}" }
}

private fun formatTileRef(tile: net.jonh.aerobag.prototype.domain.RenderTile): String =
    "package=${tile.mapView.packageName ?: tile.mapViewId} storage=${tile.mapView.storageKind} z=${tile.zoom} x=${tile.x} y=${tile.yTms} candidates=${tile.candidateMapViews.size}"

private fun decodedTileCacheKey(tile: net.jonh.aerobag.prototype.domain.RenderTile): String {
    val candidates = tile.candidateMapViews
        .distinctBy { "${it.packageName}:${it.tileRoot}:${it.chartIndex}" }
        .joinToString("|") { mapView ->
            "${mapView.packageName}:${mapView.storageKind}:${tileRelativePath(tile, mapView)}"
        }
    return "${tile.zoom}:${tile.x}:${tile.yTms}:$candidates"
}

private fun findParentTileFallback(
    tile: net.jonh.aerobag.prototype.domain.RenderTile,
    decodedTileBitmapCache: DecodedTileBitmapCache,
    maxLevelDelta: Int = 4,
): TileBitmapFallback? {
    for (levelDelta in 1..maxLevelDelta) {
        val factor = 1 shl levelDelta
        val parentZoom = tile.zoom - levelDelta
        if (parentZoom < 0) {
            break
        }
        val parentTile = tile.copy(
            x = tile.x / factor,
            yTms = tile.yTms / factor,
            zoom = parentZoom,
        )
        val bitmap = decodedTileBitmapCache.get(decodedTileCacheKey(parentTile)) ?: continue
        return TileBitmapFallback(
            bitmap = bitmap,
            sourceLevelDelta = levelDelta,
            sourceColumn = tile.x % factor,
            sourceRow = factor - 1 - (tile.yTms % factor),
        )
    }
    return null
}

private fun findChildTileFallbacks(
    tile: net.jonh.aerobag.prototype.domain.RenderTile,
    decodedTileBitmapCache: DecodedTileBitmapCache,
    maxLevelDelta: Int = 3,
): List<ChildTileBitmapFallback> {
    for (levelDelta in 1..maxLevelDelta) {
        val factor = 1 shl levelDelta
        val childZoom = tile.zoom + levelDelta
        val fallbacks = mutableListOf<ChildTileBitmapFallback>()
        for (row in 0 until factor) {
            for (column in 0 until factor) {
                val childTile = tile.copy(
                    x = tile.x * factor + column,
                    yTms = tile.yTms * factor + row,
                    zoom = childZoom,
                )
                val bitmap = decodedTileBitmapCache.get(decodedTileCacheKey(childTile)) ?: continue
                fallbacks += ChildTileBitmapFallback(
                    bitmap = bitmap,
                    targetLevelDelta = levelDelta,
                    targetColumn = column,
                    targetRow = factor - 1 - row,
                )
            }
        }
        if (fallbacks.isNotEmpty()) {
            return fallbacks
        }
    }
    return emptyList()
}

private fun projectAhead(lat: Double, lon: Double, bearingDeg: Double, distanceNm: Double): LatLon {
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

private fun selectSituationRing(
    position: LatLon,
    viewport: MapViewportState,
    widthUnits: Float,
    heightUnits: Float,
    ringCandidates: List<SituationRingCandidate>,
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
        tickMarks = buildSituationTickMarks(center, best.second),
        cardinalLabels = buildSituationCardinalLabels(center, best.second),
        labelPointUnits = labelPoint,
        labelRotationDeg = 45f,
        labelText = best.first.label,
    )
}

private fun buildSituationTickMarks(center: Offset, radiusUnits: Float): List<SituationTickMark> =
    List(12) { index ->
        val angle = index * 30f
        SituationTickMark(
            innerUnits = pointOnCircle(center, radiusUnits - 14f, angle),
            outerUnits = pointOnCircle(center, radiusUnits, angle),
        )
    }

private fun buildSituationCardinalLabels(center: Offset, radiusUnits: Float): List<SituationCardinalLabel> {
    val labelRadius = maxOf(0f, radiusUnits - 30f)
    return listOf(
        Triple("N", -90f, 0f),
        Triple("E", 0f, 90f),
        Triple("S", 90f, 0f),
        Triple("W", 180f, -90f),
    ).map { (text, angleDeg, rotationDeg) ->
        SituationCardinalLabel(
            text = text,
            pointUnits = pointOnCircle(center, labelRadius, angleDeg),
            rotationDeg = rotationDeg,
        )
    }
}

private fun pointOnCircle(center: Offset, radiusUnits: Float, angleDeg: Float): Offset {
    val radians = Math.toRadians(angleDeg.toDouble())
    return Offset(
        x = center.x + (radiusUnits * cos(radians)).toFloat(),
        y = center.y + (radiusUnits * sin(radians)).toFloat(),
    )
}

private fun arrowShaftEndPoint(from: Offset, to: Offset): Offset {
    val angle = atan2(to.y - from.y, to.x - from.x)
    val headLength = 14f
    return Offset(
        x = to.x - headLength * cos(angle),
        y = to.y - headLength * sin(angle),
    )
}

private fun arrowHeadPath(from: Offset, to: Offset): Path {
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

private fun fixTrianglePath(center: Offset, radius: Float): Path =
    Path().apply {
        moveTo(center.x, center.y - radius)
        lineTo(center.x + radius * 0.875f, center.y + radius * 0.75f)
        lineTo(center.x - radius * 0.875f, center.y + radius * 0.75f)
        close()
    }

private fun vorHexPoints(center: Offset, radius: Float): List<Offset> =
    List(6) { index ->
        val angle = Math.toRadians((-90 + index * 60).toDouble())
        Offset(
            x = center.x + (radius * cos(angle)).toFloat(),
            y = center.y + (radius * sin(angle)).toFloat(),
        )
    }

private fun polygonSignedArea(points: List<Offset>): Float {
    var area = 0f
    points.forEachIndexed { index, point ->
        val next = points[(index + 1) % points.size]
        area += point.x * next.y - next.x * point.y
    }
    return area / 2f
}

private fun intersectLines(originA: Offset, directionA: Offset, originB: Offset, directionB: Offset): Offset {
    val cross = directionA.x * directionB.y - directionA.y * directionB.x
    if (kotlin.math.abs(cross) < 1e-6f) {
        return originA
    }
    val delta = originB - originA
    val t = (delta.x * directionB.y - delta.y * directionB.x) / cross
    return originA + directionA * t
}

private fun offsetPolygonByEdgeDistances(points: List<Offset>, edgeDistances: List<Float>): List<Offset> {
    val signedArea = polygonSignedArea(points)
    fun inwardNormal(from: Offset, to: Offset, distance: Float): Offset {
        val dx = to.x - from.x
        val dy = to.y - from.y
        val length = kotlin.math.hypot(dx, dy).takeIf { it > 0f } ?: 1f
        return if (signedArea > 0f) {
            Offset((dy / length) * distance, (-dx / length) * distance)
        } else {
            Offset((-dy / length) * distance, (dx / length) * distance)
        }
    }

    return points.mapIndexed { index, point ->
        val prevIndex = (index + points.size - 1) % points.size
        val nextIndex = (index + 1) % points.size
        val prevPoint = points[prevIndex]
        val nextPoint = points[nextIndex]
        val prevShift = inwardNormal(prevPoint, point, edgeDistances[prevIndex])
        val nextShift = inwardNormal(point, nextPoint, edgeDistances[index])
        val prevOrigin = prevPoint + prevShift
        val nextOrigin = point + nextShift
        intersectLines(
            prevOrigin,
            point - prevPoint,
            nextOrigin,
            nextPoint - point,
        )
    }
}

private fun polygonPath(points: List<Offset>): Path =
    Path().apply {
        if (points.isNotEmpty()) {
            moveTo(points.first().x, points.first().y)
            points.drop(1).forEach { point -> lineTo(point.x, point.y) }
            close()
        }
    }

private fun vorBandPath(center: Offset, radius: Float): Path {
    val outer = vorHexPoints(center, radius)
    val inner = offsetPolygonByEdgeDistances(outer, listOf(
        radius * 0.47f,
        radius * 0.24f,
        radius * 0.47f,
        radius * 0.24f,
        radius * 0.47f,
        radius * 0.24f,
    ))
    return Path().apply {
        fillType = PathFillType.EvenOdd
        addPath(polygonPath(outer))
        addPath(polygonPath(inner))
    }
}

private fun transformVisibleFeature(
    feature: net.jonh.aerobag.prototype.domain.VisibleMapFeature,
    fromViewport: MapViewportState,
    fromSurface: OverlaySurfaceUnits,
    toViewport: MapViewportState,
    toSurface: OverlaySurfaceUnits,
): net.jonh.aerobag.prototype.domain.VisibleMapFeature {
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

private fun transformMapOverlayForDisplay(
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

private fun transformVisibleMetarFeature(
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

private fun transformAirspaceDisplayLabel(
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

private fun transformAirspaceDisplayPath(
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

private fun transformAirspaceDisplayDecoration(
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
    )

private fun transformAirspaceDisplaySubpath(
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

private fun transformScreenPoint(
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

private fun formatPlanDistance(distanceNm: Double?): String =
    when {
        distanceNm == null -> "—"
        distanceNm < 10.0 -> "%.1f".format(distanceNm)
        else -> "%.0f".format(distanceNm)
    }

private fun formatPlanCourse(courseDeg: Double?): String {
    if (courseDeg == null) {
        return "—"
    }
    val rounded = ((courseDeg.roundToInt() % 360) + 360) % 360
    return if (rounded == 0) "360" else rounded.toString().padStart(3, '0')
}

private fun readRecentAirportIds(context: Context): List<String> =
    context.getSharedPreferences(UiPrefsName, Context.MODE_PRIVATE)
        .getString(UiPrefsRecentAirportsKey, "")
        .orEmpty()
        .split('\n')
        .map(String::trim)
        .filter(String::isNotEmpty)

private fun writeUiPrefs(
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

private fun readStoredPage(prefs: SharedPreferences): AppPage {
    val stored = prefs.getString(UiPrefsPageKey, AppPage.Map.name) ?: AppPage.Map.name
    return if (stored == "Settings") {
        AppPage.Home
    } else {
        runCatching { AppPage.valueOf(stored) }.getOrDefault(AppPage.Map)
    }
}

private fun summarizeRuntimeBootstrapFailure(error: Throwable): String {
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

    private val gpsPermissionLauncher =
        registerForActivityResult(ActivityResultContracts.RequestMultiplePermissions()) { grants ->
            if (grants[Manifest.permission.ACCESS_FINE_LOCATION] == true) {
                startAndroidGpsService()
            } else {
                AndroidGpsSource.publishStatus(AndroidGpsSource.unavailableStatus("Precise location required"))
            }
        }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        requestAndroidGps()
        setContent {
            MaterialTheme {
                Surface(
                    modifier = Modifier.fillMaxSize(),
                    color = Color(0xFFF3EFE4),
                ) {
                    AerobagApp()
                }
            }
        }
    }

    override fun dispatchKeyEvent(event: AndroidKeyEvent): Boolean {
        if (event.action == AndroidKeyEvent.ACTION_DOWN) {
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

@Composable
private fun AerobagApp() {
    val context = LocalContext.current
    val prefs = remember(context) { context.applicationContext.getSharedPreferences(UiPrefsName, Context.MODE_PRIVATE) }
    val bootstrap = remember(context) { SampleData.loadBootstrap(context.applicationContext) }
    var runtimeReloadToken by remember { mutableStateOf(0) }
    val offlinePackagesControllerHandle = remember(prefs) { initialOfflinePackagesControllerHandle(prefs) }
    DisposableEffect(offlinePackagesControllerHandle) {
        onDispose { NativeBindings.destroyOfflinePackagesController(offlinePackagesControllerHandle) }
    }
    val uiTheme = remember(context) { UiThemeLoader.load(context.applicationContext) }
    val runtimeFixture by produceState<Result<net.jonh.aerobag.prototype.domain.ContentFixture>?>(initialValue = null, context, bootstrap, runtimeReloadToken) {
        value = withContext(Dispatchers.IO) {
            runCatching { SampleData.loadRuntime(context.applicationContext, bootstrap) }
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
    val appCore = remember(fixture.vectorManifestJson, fixture.navKvStore) {
        NativeAppCoreAdapter(
            fixture.vectorManifestJson,
            navKvStore = fixture.navKvStore,
        )
    }
    val situationRingCandidates = remember(appCore) { appCore.situationRingCandidates() }
    val initialPlanMutation = remember(appCore, bootstrap.samplePlan) {
        buildSeededDevPlan(appCore, bootstrap.samplePlan)
    }
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
    var selectedMapId by remember { mutableStateOf(initialMapId(fixture)) }
    val uiSession = remember(appCore) {
        appCore.createUiSession(
            initialPlanMutation.plan,
            storedRecentAirportIds,
            storedSelectedAirportId.ifBlank { null },
            storedSelectedChartId.ifBlank { null },
        )
    }
    DisposableEffect(uiSession) {
        onDispose { uiSession.destroy() }
    }
    var sessionSnapshot by remember(uiSession) { mutableStateOf(uiSession.snapshot) }
    val appState = sessionSnapshot.appState
    val appUiState = sessionSnapshot.appUiState
    val currentPlan = appState.activePlan ?: initialPlanMutation.plan
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
    val selectedMap = remember(selectedMapId, fixture.mapViews) {
        fixture.mapViews.find { it.id == selectedMapId } ?: fixture.mapViews.first()
    }
    var mapViewport by remember { mutableStateOf(createInitialSituationViewport(selectedMap.mapView)) }
    var chartViewport by remember { mutableStateOf<net.jonh.aerobag.prototype.domain.ImageViewportState?>(null) }
    var chartFolderOpen by remember { mutableStateOf(false) }
    var pageTilePaintTiming by remember { mutableStateOf<PageTilePaintTiming?>(null) }
    var nextPageTilePaintTimingId by remember { mutableStateOf(1L) }
    var debugFastTiles by remember { mutableStateOf(false) }
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

    LaunchedEffect(selectedMap.id) {
        mapViewport = preserveViewportForMap(mapViewport, selectedMap.mapView)
    }

    fun currentSnapshot(): AppViewSnapshot = AppViewSnapshot(
        page = page,
        selectedMapId = selectedMapId,
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
            uiSession.installRasterMapCatalogForSelection(snapshot.selectedMapId)
            uiSession.selectMap(snapshot.selectedMapId)
        }
        selectedMapId = snapshot.selectedMapId
        mapViewport = snapshot.mapViewport
        chartViewport = snapshot.chartViewport
        chartFolderOpen = snapshot.chartFolderOpen
    }

    fun navigateToPage(nextPage: AppPage) {
        if (nextPage == page) {
            return
        }
        if (nextPage == AppPage.Map) {
            pageTilePaintTiming = PageTilePaintTiming(
                id = nextPageTilePaintTimingId++,
                fromPage = page,
                startedMs = SystemClock.elapsedRealtime(),
            )
            Log.i(TileBudgetLogTag, "page-to-map-start id=${pageTilePaintTiming?.id} from=$page")
        }
        pageHistory = boundedHistory(pageHistory + currentSnapshot())
        page = nextPage
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
                    fixture = fixture,
                    uiSession = uiSession,
                    sessionSnapshot = sessionSnapshot,
                    uiTheme = uiTheme,
                    ownship = appUiState.ownship.render,
                    playbackUiState = sessionSnapshot.playbackUiState,
                    playbackSourcePath = playbackSourcePath,
                    mapFollowUiState = sessionSnapshot.mapFollowUiState,
                    mapFollowTargetViewport = sessionSnapshot.mapFollowTargetViewport,
                    situationRingCandidates = situationRingCandidates,
                    selectedMapId = selectedMapId,
                    viewport = mapViewport,
                    decodedTileBitmapCache = decodedTileBitmapCache,
                    debugFastTiles = debugFastTiles,
                    onDebugFastTilesChange = { debugFastTiles = it },
                    pageTilePaintTiming = pageTilePaintTiming,
                    onPageTilePaintTimingComplete = { completedId ->
                        if (pageTilePaintTiming?.id == completedId) {
                            pageTilePaintTiming = null
                        }
                    },
                    onViewportChange = { mapViewport = it },
                    onSessionSnapshotChange = { sessionSnapshot = it },
                    onPlaybackSourcePathChange = { playbackSourcePath = it },
                    onSelectMapId = {
                        restoreSnapshot(
                            currentSnapshot().copy(
                                page = AppPage.Map,
                                selectedMapId = it,
                            ),
                            boundedHistory(pageHistory + currentSnapshot()),
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
                    page = page,
                    pageHistory = pageHistory,
                    uptimeLabel = uptimeLabel,
                    navElement = navElement,
                    samplePlan = currentPlan,
                    planUiState = sessionPlanUiState,
                    planListState = planListState,
                    uiTheme = uiTheme,
                    onSelectPage = ::navigateToPage,
                    onOpenPlan = { navigateToPage(AppPage.Plan) },
                    onOpenCharts = { airportId -> if (airportId != null) openChartsForAirport(airportId) },
                    onApplyMutation = { mutation ->
                        sessionSnapshot = uiSession.replaceFlightPlan(mutation.plan)
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
                    uiTheme = uiTheme,
                    ownship = appUiState.ownship.render,
                    navElement = navElement,
                    folderOpen = chartFolderOpen,
                    viewport = chartViewport,
                    onViewportChange = { chartViewport = it },
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
                HomePage(
                    page = page,
                    pageHistory = pageHistory,
                    uptimeLabel = uptimeLabel,
                    bootstrap = bootstrap,
                    navElement = navElement,
                    onSelectPage = ::navigateToPage,
                    onOpenPlan = { navigateToPage(AppPage.Plan) },
                    initialOfflinePackagesOpen = keepOfflinePackagesVisible,
                    offlinePackagesControllerHandle = offlinePackagesControllerHandle,
                    onOfflinePackagesClosed = { keepOfflinePackagesVisible = false },
                    onRuntimeMaybeAvailable = { runtimeReloadToken += 1 },
                )
            }
        }
    }
    }
}

@Composable
private fun HomePage(
    page: AppPage,
    pageHistory: List<AppViewSnapshot>,
    uptimeLabel: String,
    bootstrap: net.jonh.aerobag.prototype.domain.BootstrapFixture,
    navElement: NavElementUiView?,
    onSelectPage: (AppPage) -> Unit,
    onOpenPlan: () -> Unit,
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
    var packageSourceBaseUrl by remember(context, prefs) {
        mutableStateOf(readPackageSourceBaseUrl(context.applicationContext, prefs))
    }
    var offlinePackagesOpen by remember { mutableStateOf(forceOfflinePackagesOpen || initialOfflinePackagesOpen) }
    var debugPanelOpen by remember { mutableStateOf(false) }
    var debugOfflineSimulatedClockButtons by remember { mutableStateOf(false) }
    val regionOptions = remember { offlineRegionOptions() }
    val regionIds = remember(regionOptions) { regionOptions.map { it.id } }
    val productIds = remember { OfflineProductOptions.map { it.id } }
    var offlinePackagesControllerResult by remember { mutableStateOf<OfflinePackagesControllerResultWire?>(null) }
    var offlinePackageOperationJob by remember { mutableStateOf<Job?>(null) }
    var offlinePackageCancelRequested by remember { mutableStateOf(false) }
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
            discoveryFilenames = bootstrap.packageManagementDiscoveryFilenames,
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
        val result = PackageManagementJson.decodeFromString<OfflinePackagesControllerResultWire>(
            NativeBindings.dispatchOfflinePackagesControllerJson(offlinePackagesControllerHandle, inputJson),
        )
        writeOfflinePackagesStateJson(prefs, result.packagesStateJson)
        offlinePackagesControllerResult = result
        when (val command = result.command) {
            is OfflinePackagesControllerCommandWire.RefreshLibrary -> {
                val refreshResult: Result<OfflinePackagesControllerEventWire.LibraryRefreshSucceeded> = runCatching {
                    withContext(Dispatchers.IO) {
                        refreshOfflinePackageLibrary(
                            packageSourceBaseUrl = command.packageSourceBaseUrl,
                            discoveryFilenames = command.discoveryFilenames,
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
            }
            is OfflinePackagesControllerCommandWire.Sync -> {
                val summary = try {
                    withContext(Dispatchers.IO) {
                        syncOfflinePackages(
                            context = context.applicationContext,
                            plan = command.plan,
                            bundle = command.bundle,
                            packageSourceBaseUrl = command.packageSourceBaseUrl,
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
        LazyVerticalGrid(
            columns = GridCells.Fixed(3),
            modifier = Modifier
                .align(Alignment.TopStart)
                .padding(
                    start = ThumbGap + (ThumbSize * 0.5f),
                    top = ThumbGap + (ThumbSize * 0.5f),
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
                        if (button.targetPage != null) {
                            onSelectPage(button.targetPage)
                        } else if (button.key == "offline-packages") {
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

        DebugDock(
            open = debugPanelOpen,
            onToggle = { debugPanelOpen = !debugPanelOpen },
            expandAbove = true,
            modifier = Modifier
                .align(Alignment.BottomEnd)
                .padding(end = ThumbGap, bottom = ThumbGap),
        ) {
            Text("page ${pageLabel(page)}", style = MaterialTheme.typography.labelSmall, color = Color(0xFF52656D))
            Text("up $uptimeLabel", style = MaterialTheme.typography.labelSmall, color = Color(0xFF52656D))
            Text(
                "stack ${formatPageStack(pageHistory, page)}",
                style = MaterialTheme.typography.labelSmall,
                color = Color(0xFF52656D),
            )
            Row(
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(4.dp),
            ) {
                Checkbox(
                    checked = debugOfflineSimulatedClockButtons,
                    onCheckedChange = { debugOfflineSimulatedClockButtons = it },
                    modifier = Modifier.size(ThumbSize * 0.36f),
                )
                Text(
                    "offline simulated clock buttons",
                    style = MaterialTheme.typography.labelSmall,
                    color = Color(0xFF52656D),
                )
            }
        }

        if (offlinePackagesOpen || forceOfflinePackagesOpen) {
            if (!forceOfflinePackagesOpen) {
                Scrim { offlinePackagesOpen = false }
            }
            val controllerUiState = offlinePackagesControllerResult?.uiState
            val navDbStatus by produceState<net.jonh.aerobag.prototype.domain.NavDbStatus?>(initialValue = null, context, controllerUiState?.syncMessage, offlinePackagesOpen, forceOfflinePackagesOpen) {
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
                    showSimulatedClockButtons = debugOfflineSimulatedClockButtons,
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

@Composable
private fun OfflinePackagesErrorPanel(
    message: String,
    closeEnabled: Boolean,
    onClose: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val uiTheme = LocalAerobagUiTheme.current
    Surface(
        modifier = modifier.fillMaxSize(),
        shape = RoundedCornerShape(ThumbRadius + 4.dp),
        color = uiTheme.controls.panelBg,
        contentColor = uiTheme.controls.panelFg,
        border = BorderStroke(2.dp, uiTheme.controls.panelBorder),
        shadowElevation = 12.dp,
    ) {
        Column(
            modifier = Modifier
                .fillMaxSize()
                .padding(ThumbGap),
            verticalArrangement = Arrangement.spacedBy(ThumbGap),
        ) {
            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.SpaceBetween,
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Text(
                    text = "OFFLINE PACKAGES ERROR",
                    style = MaterialTheme.typography.titleMedium,
                    fontWeight = FontWeight.ExtraBold,
                    color = uiTheme.controls.panelFg,
                )
                CompactSquareButton(
                    label = "X",
                    modifier = Modifier.size(ThumbSize * 0.72f),
                    enabled = closeEnabled,
                    onClick = onClose,
                )
            }
            Text(
                text = message,
                style = MaterialTheme.typography.bodyMedium,
                color = uiTheme.controls.panelFg,
            )
        }
    }
}

@Composable
private fun OfflinePackagesLibraryPanel(
    message: String,
    packageSourceBaseUrl: String,
    onPackageSourceBaseUrlChange: (String) -> Unit,
    refreshInFlight: Boolean,
    sourceEditable: Boolean,
    refreshEnabled: Boolean,
    refreshCancelEnabled: Boolean,
    cancelRequested: Boolean,
    onRefresh: () -> Unit,
    onCancelRefresh: () -> Unit,
    closeEnabled: Boolean,
    onClose: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val uiTheme = LocalAerobagUiTheme.current
    Surface(
        modifier = modifier.fillMaxSize(),
        shape = RoundedCornerShape(ThumbRadius + 4.dp),
        color = uiTheme.controls.panelBg,
        contentColor = uiTheme.controls.panelFg,
        border = BorderStroke(2.dp, uiTheme.controls.panelBorder),
        shadowElevation = 12.dp,
    ) {
        Column(
            modifier = Modifier
                .fillMaxSize()
                .padding(ThumbGap),
            verticalArrangement = Arrangement.spacedBy(ThumbGap),
        ) {
            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.SpaceBetween,
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Text(
                    text = "OFFLINE PACKAGES",
                    style = MaterialTheme.typography.titleMedium,
                    fontWeight = FontWeight.ExtraBold,
                    color = uiTheme.controls.panelFg,
                )
                CompactSquareButton(
                    label = if (refreshInFlight) {
                        if (cancelRequested) "CANCELING..." else "REFRESHING\n(cancel)"
                    } else {
                        "REFRESH"
                    },
                    modifier = Modifier
                        .width(if (refreshInFlight) ThumbSize * 2.24f else ThumbSize * 1.8f)
                        .height(ThumbSize * 0.72f),
                    maxLines = if (refreshInFlight) 2 else 1,
                    enabled = !cancelRequested && if (refreshInFlight) refreshCancelEnabled else refreshEnabled,
                    onClick = if (refreshInFlight) onCancelRefresh else onRefresh,
                )
                CompactSquareButton(
                    label = "X",
                    modifier = Modifier.size(ThumbSize * 0.72f),
                    enabled = closeEnabled,
                    onClick = onClose,
                )
            }
            Text(
                text = message,
                style = MaterialTheme.typography.bodyMedium,
                color = uiTheme.controls.panelFg,
            )
            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.spacedBy(ThumbGap * 0.5f),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Text(
                    text = "SOURCE",
                    style = MaterialTheme.typography.labelMedium,
                    fontWeight = FontWeight.ExtraBold,
                    color = uiTheme.controls.panelMuted,
                )
                BasicTextField(
                    value = packageSourceBaseUrl,
                    onValueChange = onPackageSourceBaseUrlChange,
                    enabled = sourceEditable,
                    singleLine = true,
                    textStyle = MaterialTheme.typography.labelSmall.copy(
                        color = uiTheme.controls.panelFg,
                        fontWeight = FontWeight.Bold,
                    ),
                    modifier = Modifier
                        .weight(1f)
                        .height(ThumbSize * 0.72f)
                        .clip(RoundedCornerShape(ThumbRadius))
                        .background(Color.White)
                        .border(1.dp, uiTheme.controls.panelBorder, RoundedCornerShape(ThumbRadius))
                        .padding(horizontal = ThumbGap * 0.7f, vertical = ThumbGap * 0.55f),
                )
            }
        }
    }
}

@Composable
private fun OfflinePackagesPanel(
    regionOptions: List<OfflinePackageDimension>,
    productOptions: List<OfflinePackageDimension>,
    uiState: OfflinePackagesUiStateWire,
    navDbStatusText: String?,
    syncMessage: String?,
    cancelRequested: Boolean,
    showSimulatedClockButtons: Boolean,
    packageSourceBaseUrl: String,
    onPackageSourceBaseUrlChange: (String) -> Unit,
    onRefreshLibrary: () -> Unit,
    libraryRefreshInFlight: Boolean,
    packageSourceEditable: Boolean,
    refreshEnabled: Boolean,
    refreshCancelEnabled: Boolean,
    syncEnabled: Boolean,
    syncCancelEnabled: Boolean,
    plannerInteractionsEnabled: Boolean,
    onCancelRefresh: () -> Unit,
    onRowClick: (OfflinePackagesEventWire) -> Unit,
    onClockClick: (String) -> Unit,
    onSync: () -> Unit,
    onCancelOperation: () -> Unit,
    syncInFlight: Boolean,
    closeEnabled: Boolean,
    onClose: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val uiTheme = LocalAerobagUiTheme.current
    Surface(
        modifier = modifier.fillMaxSize(),
        shape = RoundedCornerShape(ThumbRadius + 4.dp),
        color = uiTheme.controls.panelBg,
        contentColor = uiTheme.controls.panelFg,
        border = BorderStroke(2.dp, uiTheme.controls.panelBorder),
        shadowElevation = 12.dp,
    ) {
        Column(
            modifier = Modifier
                .fillMaxSize()
                .padding(ThumbGap),
            verticalArrangement = Arrangement.spacedBy(ThumbGap),
        ) {
            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.SpaceBetween,
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Column(modifier = Modifier.weight(1f)) {
                    Text(
                        text = "OFFLINE PACKAGES",
                        style = MaterialTheme.typography.titleMedium,
                        fontWeight = FontWeight.ExtraBold,
                        color = uiTheme.controls.panelFg,
                    )
                }
                CompactSquareButton(
                    label = if (libraryRefreshInFlight) {
                        if (cancelRequested) "CANCELING..." else "REFRESHING\n(cancel)"
                    } else {
                        "REFRESH"
                    },
                    modifier = Modifier
                        .width(if (libraryRefreshInFlight) ThumbSize * 2.24f else ThumbSize * 1.8f)
                        .height(ThumbSize * 0.72f),
                    maxLines = if (libraryRefreshInFlight) 2 else 1,
                    enabled = !cancelRequested && if (libraryRefreshInFlight) refreshCancelEnabled else refreshEnabled,
                    onClick = if (libraryRefreshInFlight) onCancelRefresh else onRefreshLibrary,
                )
                CompactSquareButton(
                    label = if (syncInFlight) {
                        if (cancelRequested) "CANCELING..." else "SYNCING\n(cancel)"
                    } else {
                        "SYNC"
                    },
                    modifier = Modifier
                        .width(if (syncInFlight) ThumbSize * 2.05f else ThumbSize * 1.4f)
                        .height(ThumbSize * 0.72f),
                    maxLines = if (syncInFlight) 2 else 1,
                    enabled = !cancelRequested && if (syncInFlight) syncCancelEnabled else syncEnabled,
                    onClick = if (syncInFlight) onCancelOperation else onSync,
                )
                CompactSquareButton(
                    label = "X",
                    modifier = Modifier.size(ThumbSize * 0.72f),
                    enabled = closeEnabled,
                    onClick = onClose,
                )
            }

            syncMessage?.let { message ->
                Text(
                    text = message,
                    style = MaterialTheme.typography.labelMedium,
                    fontWeight = FontWeight.Bold,
                    color = Color(0xFFD98B38),
                )
            }
            Text(
                text = navDbStatusText ?: "NAVDB unknown",
                style = MaterialTheme.typography.labelMedium,
                fontWeight = FontWeight.Bold,
                color = uiTheme.controls.panelFg,
            )
            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.spacedBy(ThumbGap * 0.5f),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Text(
                    text = "SOURCE",
                    style = MaterialTheme.typography.labelMedium,
                    fontWeight = FontWeight.ExtraBold,
                    color = uiTheme.controls.panelMuted,
                )
                BasicTextField(
                    value = packageSourceBaseUrl,
                    onValueChange = onPackageSourceBaseUrlChange,
                    enabled = packageSourceEditable,
                    singleLine = true,
                    textStyle = MaterialTheme.typography.labelSmall.copy(
                        color = uiTheme.controls.panelFg,
                        fontWeight = FontWeight.Bold,
                    ),
                    modifier = Modifier
                        .weight(1f)
                        .height(ThumbSize * 0.72f)
                        .clip(RoundedCornerShape(ThumbRadius))
                        .background(Color.White)
                        .border(1.dp, uiTheme.controls.panelBorder, RoundedCornerShape(ThumbRadius))
                        .padding(horizontal = ThumbGap * 0.7f, vertical = ThumbGap * 0.55f),
                )
            }
            if (showSimulatedClockButtons && uiState.clockOptions.isNotEmpty()) {
                Row(
                    modifier = Modifier.fillMaxWidth(),
                    horizontalArrangement = Arrangement.spacedBy(ThumbGap * 0.5f),
                ) {
                    uiState.clockOptions.forEach { option ->
                        CompactSquareButton(
                            label = option.label,
                            modifier = Modifier
                                .weight(1f)
                                .height(ThumbSize * 0.72f),
                            maxLines = 1,
                            enabled = plannerInteractionsEnabled,
                            selected = option.active,
                            onClick = { onClockClick(option.id) },
                        )
                    }
                }
            }

            OfflinePackageAllSection(row = uiState.allPackages)

            LazyColumn(
                modifier = Modifier.weight(1f),
                verticalArrangement = Arrangement.spacedBy(ThumbGap),
            ) {
                if (uiState.coreProducts.isNotEmpty()) {
                    item("core-products") {
                        OfflinePackageCoreSection(rows = uiState.coreProducts)
                    }
                }
                item("regions") {
                    OfflinePackageSection(
                        title = "REGIONS",
                        options = regionOptions,
                        rows = uiState.regions,
                        enabled = plannerInteractionsEnabled,
                        onRowClick = { id ->
                            onRowClick(OfflinePackagesEventWire(kind = "cycle_region", id = id))
                        },
                    )
                }
                item("products") {
                    OfflinePackageSection(
                        title = "PRODUCTS",
                        options = productOptions,
                        rows = uiState.products,
                        enabled = plannerInteractionsEnabled,
                        onRowClick = { id ->
                            onRowClick(OfflinePackagesEventWire(kind = "cycle_product", id = id))
                        },
                    )
                }
            }
        }
    }
}

@Composable
private fun OfflinePackageAllSection(
    row: OfflinePackagesUiRowWire,
) {
    val uiTheme = LocalAerobagUiTheme.current
    MenuPanel(modifier = Modifier.fillMaxWidth()) {
        OfflinePackagePlanRow(
            label = "All packages",
            row = row,
            enabled = false,
            onCycleClick = null,
            showSelectionIcon = false,
            backgroundOverride = lerp(uiTheme.controls.buttonBg, Color.Gray, 0.34f),
        )
    }
}

@Composable
private fun OfflinePackageSection(
    title: String,
    options: List<OfflinePackageDimension>,
    rows: List<OfflinePackagesUiRowWire>,
    enabled: Boolean,
    onRowClick: (String) -> Unit,
) {
    val uiTheme = LocalAerobagUiTheme.current
    val rowsById = rows.associateBy { it.id }
    MenuPanel(modifier = Modifier.fillMaxWidth()) {
        Text(
            text = title,
            modifier = Modifier.padding(horizontal = 9.dp, vertical = 5.dp),
            style = MaterialTheme.typography.labelMedium,
            fontWeight = FontWeight.ExtraBold,
            color = uiTheme.controls.panelMuted,
        )
        options.forEach { option ->
            val row = rowsById[option.id] ?: OfflinePackagesUiRowWire(
                id = option.id,
                selection = OfflinePackageSelection.Play,
            )
            OfflinePackageSelectionRow(
                label = option.label,
                row = row,
                enabled = enabled,
                onClick = { onRowClick(option.id) },
            )
        }
    }
}

@Composable
private fun OfflinePackageCoreSection(
    rows: List<OfflinePackagesUiRowWire>,
) {
    val labelById = mapOf(
        "nav-db" to "NAV DB",
        "vectors" to "VECTORS",
        "geo" to "GEO",
        "terrain" to "TERRAIN",
    )
    val uiTheme = LocalAerobagUiTheme.current
    MenuPanel(modifier = Modifier.fillMaxWidth()) {
        Text(
            text = "CORE",
            modifier = Modifier.padding(horizontal = 9.dp, vertical = 5.dp),
            style = MaterialTheme.typography.labelMedium,
            fontWeight = FontWeight.ExtraBold,
            color = uiTheme.controls.panelMuted,
        )
        rows.forEach { row ->
            OfflinePackagePlanRow(
                label = labelById[row.id] ?: row.id.uppercase(),
                row = row,
                enabled = false,
                onCycleClick = null,
            )
        }
    }
}

@Composable
private fun OfflinePackageSelectionRow(
    label: String,
    row: OfflinePackagesUiRowWire,
    enabled: Boolean,
    onClick: () -> Unit,
) {
    OfflinePackagePlanRow(
        label = label,
        row = row,
        enabled = enabled,
        onCycleClick = onClick,
    )
}

@Composable
private fun OfflinePackagePlanRow(
    label: String,
    row: OfflinePackagesUiRowWire,
    enabled: Boolean,
    onCycleClick: (() -> Unit)?,
    showSelectionIcon: Boolean = true,
    backgroundOverride: Color? = null,
) {
    val uiTheme = LocalAerobagUiTheme.current
    val background = backgroundOverride ?: when (row.selection) {
            OfflinePackageSelection.Play -> lerp(uiTheme.controls.buttonBg, Color.White, 0.14f)
            OfflinePackageSelection.Pause -> lerp(uiTheme.controls.buttonBg, Color(0xFFFFC166), 0.18f)
            OfflinePackageSelection.Unselected -> uiTheme.controls.buttonBg
        }
    val progressFraction = row.syncProgressPerMille?.coerceIn(0, 1000)?.toFloat()?.div(1000f)
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .height(ThumbSize)
            .clip(RoundedCornerShape(ThumbRadius))
            .background(background)
            .drawBehind {
                if (progressFraction != null) {
                    drawRect(
                        color = OfflinePackageMagenta.copy(alpha = 0.22f),
                        size = Size(size.width * progressFraction, size.height),
                    )
                }
            }
            .padding(horizontal = 6.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        if (showSelectionIcon) {
            Box(
                modifier = Modifier
                    .size(ThumbSize * 0.46f)
                    .clip(CircleShape)
                    .then(
                        if (enabled && onCycleClick != null) {
                            Modifier.clickable(
                                indication = null,
                                interactionSource = remember { MutableInteractionSource() },
                            ) { onCycleClick() }
                        } else {
                            Modifier.alpha(0.58f)
                        },
                    ),
                contentAlignment = Alignment.Center,
            ) {
                OfflinePackageSelectionIcon(selection = row.selection, modifier = Modifier.fillMaxSize())
            }
        }
        Text(
            text = label,
            modifier = Modifier.width(ThumbSize * 1.72f),
            style = MaterialTheme.typography.labelLarge,
            color = uiTheme.controls.buttonFg,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
        )
        OfflinePackagePlanSummary(
            entries = row.planEntries,
            modifier = Modifier.weight(1f),
        )
        OfflinePackageSizeSummary(
            row = row,
            modifier = Modifier.width(ThumbSize * 0.88f),
        )
    }
}

@Composable
private fun OfflinePackagePlanSummary(
    entries: List<OfflinePackagesUiPlanEntryWire>,
    modifier: Modifier = Modifier,
) {
    val visibleEntries = entries.filter { it.count > 0 }.ifEmpty {
        listOf(OfflinePackagesUiPlanEntryWire(OfflinePackagesUiPlanActionWire.Keep, 0, emptyList()))
    }
    Column(
        modifier = modifier.fillMaxHeight(),
        verticalArrangement = Arrangement.Center,
    ) {
        val visibleLines = offlinePackagePlanLines(visibleEntries).let { lines ->
            lines + List(2 - lines.size) { emptyList() }
        }
        visibleLines.forEachIndexed { lineIndex, lineEntries ->
            Row(
                modifier = Modifier
                    .fillMaxWidth()
                    .weight(1f),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(4.dp),
            ) {
                lineEntries.forEach { entry ->
                    OfflinePackagePlanActionIcon(
                        action = entry.action,
                        modifier = Modifier.size(15.dp),
                    )
                    Text(
                        text = if (entry.count > 0) {
                            "${entry.count} ${entry.cycles.joinToString(", ")}"
                        } else {
                            "ready"
                        },
                        modifier = Modifier.weight(1f),
                        style = MaterialTheme.typography.labelSmall,
                        fontWeight = FontWeight.ExtraBold,
                        color = offlinePackagePlanActionColor(entry.action),
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                    )
                }
                if (lineIndex == 1 && visibleEntries.size > 4) {
                    Text(
                        text = "...",
                        style = MaterialTheme.typography.labelSmall,
                        fontWeight = FontWeight.ExtraBold,
                        color = LocalAerobagUiTheme.current.controls.panelMuted,
                    )
                }
            }
        }
    }
}

private fun offlinePackagePlanLines(
    entries: List<OfflinePackagesUiPlanEntryWire>,
): List<List<OfflinePackagesUiPlanEntryWire>> =
    if (entries.size <= 2) {
        entries.map { listOf(it) }
    } else {
        entries.chunked(2).take(2)
    }

@Composable
private fun OfflinePackageSizeSummary(
    row: OfflinePackagesUiRowWire,
    modifier: Modifier = Modifier,
) {
    Column(
        modifier = modifier,
        verticalArrangement = Arrangement.Center,
        horizontalAlignment = Alignment.End,
    ) {
        Text(
            text = row.installedSizeLabel,
            style = MaterialTheme.typography.labelSmall,
            color = Color.White,
            fontWeight = FontWeight.ExtraBold,
            maxLines = 1,
        )
        if (row.plannedSizeChangeVisible) {
            Text(
                text = row.plannedDeltaLabel,
                style = MaterialTheme.typography.labelSmall,
                color = OfflinePackageMagenta,
                fontWeight = FontWeight.ExtraBold,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
            Text(
                text = "=${row.plannedTotalSizeLabel}",
                style = MaterialTheme.typography.labelSmall,
                color = OfflinePackageMagenta,
                fontWeight = FontWeight.ExtraBold,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
        }
    }
}

@Composable
private fun OfflinePackageSelectionIcon(selection: OfflinePackageSelection, modifier: Modifier = Modifier) {
    val action = when (selection) {
        OfflinePackageSelection.Play -> OfflinePackagesUiPlanActionWire.Fetch
        OfflinePackageSelection.Pause -> OfflinePackagesUiPlanActionWire.Pause
        OfflinePackageSelection.Unselected -> OfflinePackagesUiPlanActionWire.Delete
    }
    OfflinePackagePlanActionIcon(action = action, modifier = modifier)
}

@Composable
private fun OfflinePackagePlanActionIcon(
    action: OfflinePackagesUiPlanActionWire,
    modifier: Modifier = Modifier,
) {
    val color = offlinePackagePlanActionColor(action)
    Canvas(modifier = modifier) {
        val w = size.width
        val h = size.height
        when (action) {
            OfflinePackagesUiPlanActionWire.Fetch -> {
                val path = Path().apply {
                    moveTo(w * 0.28f, h * 0.18f)
                    lineTo(w * 0.28f, h * 0.82f)
                    lineTo(w * 0.82f, h * 0.5f)
                    close()
                }
                drawPath(path, color)
            }
            OfflinePackagesUiPlanActionWire.Pause -> {
                drawRect(color, topLeft = Offset(w * 0.25f, h * 0.18f), size = Size(w * 0.16f, h * 0.64f))
                drawRect(color, topLeft = Offset(w * 0.59f, h * 0.18f), size = Size(w * 0.16f, h * 0.64f))
            }
            OfflinePackagesUiPlanActionWire.Delete -> {
                drawCircle(color, radius = minOf(w, h) * 0.36f, center = Offset(w * 0.5f, h * 0.5f), style = Stroke(width = minOf(w, h) * 0.11f))
                drawLine(color, Offset(w * 0.26f, h * 0.74f), Offset(w * 0.74f, h * 0.26f), strokeWidth = minOf(w, h) * 0.12f, cap = StrokeCap.Round)
            }
            OfflinePackagesUiPlanActionWire.Keep -> {
                drawCircle(color, radius = minOf(w, h) * 0.32f, center = Offset(w * 0.5f, h * 0.5f))
            }
        }
    }
}

private val OfflinePackageMagenta = Color(0xFFFF3DCE)
private val OfflinePackageOrange = Color(0xFFFFA12B)
private val OfflinePackageRed = Color(0xFFFF4D5E)

private fun offlinePackagePlanActionColor(action: OfflinePackagesUiPlanActionWire): Color = when (action) {
    OfflinePackagesUiPlanActionWire.Delete -> OfflinePackageRed
    OfflinePackagesUiPlanActionWire.Keep -> Color.White
    OfflinePackagesUiPlanActionWire.Pause -> OfflinePackageOrange
    OfflinePackagesUiPlanActionWire.Fetch -> OfflinePackageMagenta
}

private fun readPackageSourceBaseUrl(
    context: Context,
    prefs: android.content.SharedPreferences,
): String =
    prefs.getString(UiPrefsPackageSourceBaseUrlKey, null)
        ?.trim()
        ?.trimEnd('/')
        ?.takeIf { it.isNotBlank() }
        ?: loadAndroidPackageSourceBaseUrl(context)

private fun initialOfflinePackagesControllerHandle(
    prefs: android.content.SharedPreferences,
): Long = NativeBindings.createOfflinePackagesController(readOfflinePackagesStateJson(prefs))

private fun writePackageSourceBaseUrl(
    prefs: android.content.SharedPreferences,
    value: String,
) {
    prefs.edit()
        .putString(UiPrefsPackageSourceBaseUrlKey, value.trim())
        .apply()
}

private fun writeOfflinePackagesStateJson(
    prefs: android.content.SharedPreferences,
    stateJson: String?,
) {
    prefs.edit()
        .putString(UiPrefsOfflinePackagePreferencesKey, stateJson)
        .apply()
}

private fun listInstalledPackageArtifacts(context: Context): List<InstalledArtifactWire> {
    return InstalledPackageKind.entries
        .asSequence()
        .flatMap { kind -> InstalledPackages.listInstalledArtifacts(context, kind).asSequence() }
        .sortedWith(compareBy({ it.artifactId }, { it.filename }))
        .map {
            InstalledArtifactWire(
                artifactId = it.artifactId,
                filename = it.filename,
                sizeBytes = it.sizeBytes,
                checksumSha256 = it.checksumSha256,
            )
        }
        .toList()
}

private suspend fun syncOfflinePackages(
    context: Context,
    plan: PackageManagementPlanWire,
    bundle: BundleManifestWire,
    packageSourceBaseUrl: String,
    maxParallelFetches: Int,
    activeConnections: ActivePackageConnections,
    onProgress: suspend (String, OfflinePackagesSyncProgressWire?) -> Unit = { _, _ -> },
): OfflinePackagesSyncSummary {
    val syncStartMs = SystemClock.elapsedRealtime()
    val packagesById = bundle.packages.associateBy { it.id }
    val installedByFilename = listInstalledPackageArtifacts(context).associateBy { it.filename }
    val warnings = mutableListOf<OfflinePackagesWarning>()
    val remotePoisonedFilenameMessages = linkedMapOf<String, String>()
    val totalFetchBytes = plan.fetch.sumOf { artifactId -> packagesById[artifactId]?.sizeBytes ?: 0L }
    val completedFetchArtifactIds = linkedSetOf<String>()
    val activeFetchBytesByArtifactId = linkedMapOf<String, Long>()
    val progressMutex = Mutex()
    var completedFetchBytes = 0L
    var fetchedCount = 0
    var gcCount = 0
    fun activeFetchBytes(): Long = activeFetchBytesByArtifactId.values.sum()
    suspend fun reportProgress(
        message: String,
        currentArtifactId: String? = null,
        currentBytes: Long? = null,
    ) {
        progressMutex.withLock {
            onProgress(
                message,
                OfflinePackagesSyncProgressWire(
                    completedFetchArtifactIds = completedFetchArtifactIds.toSet(),
                    currentFetchArtifactId = currentArtifactId,
                    currentFetchBytes = currentBytes ?: currentArtifactId?.let { activeFetchBytesByArtifactId[it] } ?: 0L,
                ),
            )
        }
    }
    reportProgress(syncProgressText(fetchedCount, plan.fetch.size, completedFetchBytes, totalFetchBytes))
    if (plan.fetch.isNotEmpty()) {
        val fetchWorkerCount = maxParallelFetches.coerceIn(1, 8).coerceAtMost(plan.fetch.size)
        val fetchQueue = Channel<IndexedValue<String>>(Channel.UNLIMITED)
        coroutineScope {
            launch {
                plan.fetch.withIndex().forEach { work ->
                    fetchQueue.send(work)
                }
                fetchQueue.close()
            }
            repeat(fetchWorkerCount) { workerIndex ->
                launch {
                    for ((index, artifactId) in fetchQueue) {
                        currentCoroutineContext().ensureActive()
                        val pkg = packagesById[artifactId]
                        if (pkg == null) {
                            progressMutex.withLock {
                                warnings += OfflinePackagesWarning(
                                    artifactId = artifactId,
                                    familyId = null,
                                    regionId = null,
                                    message = "missing bundle metadata for fetch artifact $artifactId",
                                )
                            }
                            continue
                        }
                        runCatching {
                            val fetchStartMs = SystemClock.elapsedRealtime()
                            reportProgress("Fetching package ${index + 1}/${plan.fetch.size}: ${pkg.filename}", artifactId)
                            check(packageSourceBaseUrl.isNotBlank()) { "package source URL is blank" }
                            val sourceUrl = resolvePackageSourceUrl(pkg.relativePath, packageSourceBaseUrl)
                            val kind = installedPackageKindForFamilyId(pkg.familyId)
                            var packageDownloadedBytes = 0L
                            var lastReportedPackageBytes = 0L
                            progressMutex.withLock {
                                activeFetchBytesByArtifactId[artifactId] = 0L
                            }
                            val tempFile = downloadPackageToTempFile(
                                context = context,
                                kind = kind,
                                filename = pkg.filename,
                                sourceUrl = sourceUrl,
                                expectedSizeBytes = pkg.sizeBytes,
                                expectedSha256 = pkg.checksumSha256,
                                activeConnections = activeConnections,
                                onBytesRead = { bytesRead ->
                                    packageDownloadedBytes += bytesRead
                                    var shouldReport = false
                                    progressMutex.withLock {
                                        activeFetchBytesByArtifactId[artifactId] = packageDownloadedBytes
                                        shouldReport = packageDownloadedBytes - lastReportedPackageBytes >= 10_000_000L ||
                                            packageDownloadedBytes == pkg.sizeBytes
                                    }
                                    if (shouldReport) {
                                        lastReportedPackageBytes = packageDownloadedBytes
                                        val aggregateFetchBytes = progressMutex.withLock {
                                            completedFetchBytes + activeFetchBytes()
                                        }
                                        reportProgress(
                                            syncProgressText(
                                                fetchedCount,
                                                plan.fetch.size,
                                                aggregateFetchBytes,
                                                totalFetchBytes,
                                            ),
                                            artifactId,
                                            packageDownloadedBytes,
                                        )
                                    }
                                },
                            )
                            installDownloadedPackage(
                                context = context,
                                kind = kind,
                                artifactId = pkg.id,
                                filename = pkg.filename,
                                tempFile = tempFile,
                                sizeBytes = pkg.sizeBytes,
                                checksumSha256 = pkg.checksumSha256,
                            )
                            val validationError = validateInstalledPackageOrNull(
                                context = context,
                                kind = kind,
                                pkg = pkg,
                            )
                            progressMutex.withLock {
                                activeFetchBytesByArtifactId.remove(artifactId)
                                completedFetchBytes += packageDownloadedBytes
                                completedFetchArtifactIds += artifactId
                                if (validationError != null) {
                                    remotePoisonedFilenameMessages[pkg.filename] = validationError
                                    warnings += OfflinePackagesWarning(
                                        artifactId = artifactId,
                                        familyId = pkg.familyId,
                                        regionId = pkg.regionId,
                                        message = validationError,
                                    )
                                }
                                fetchedCount += 1
                            }
                            val aggregateFetchBytes = progressMutex.withLock {
                                completedFetchBytes + activeFetchBytes()
                            }
                            reportProgress(syncProgressText(fetchedCount, plan.fetch.size, aggregateFetchBytes, totalFetchBytes))
                            Log.i(
                                "OfflinePackages",
                                "fetch installed $artifactId worker=$workerIndex in ${SystemClock.elapsedRealtime() - fetchStartMs}ms from $sourceUrl" +
                                    if (validationError != null) " poison=${pkg.filename}" else "",
                            )
                        }.onFailure {
                            progressMutex.withLock {
                                activeFetchBytesByArtifactId.remove(artifactId)
                                warnings += OfflinePackagesWarning(
                                    artifactId = artifactId,
                                    familyId = pkg.familyId,
                                    regionId = pkg.regionId,
                                    message = it.message ?: it::class.simpleName ?: "fetch failed",
                                )
                            }
                            Log.e("OfflinePackages", "fetch failed for $artifactId", it)
                            val aggregateFetchBytes = progressMutex.withLock {
                                completedFetchBytes + activeFetchBytes()
                            }
                            reportProgress(
                                syncProgressText(
                                    fetchedCount,
                                    plan.fetch.size,
                                    aggregateFetchBytes,
                                    totalFetchBytes,
                                ),
                            )
                        }
                    }
                }
            }
        }
        reportProgress(
            syncProgressText(
                fetchedCount,
                plan.fetch.size,
                completedFetchBytes,
                totalFetchBytes,
            ),
        )
    }
    plan.gc.forEachIndexed { index, filename ->
        currentCoroutineContext().ensureActive()
        runCatching {
            val gcStartMs = SystemClock.elapsedRealtime()
            reportProgress("Removing package ${index + 1}/${plan.gc.size}: $filename")
            val installedArtifact = installedByFilename[filename]
                ?: error("missing installed metadata for gc filename $filename")
            val keepFilename = packagesById[installedArtifact.artifactId]?.filename
                ?.takeIf { plan.fetch.contains(installedArtifact.artifactId) }
            deleteInstalledArtifact(context, installedArtifact.artifactId, filename, keepFilename)
            gcCount += 1
            Log.i(
                "OfflinePackages",
                "gc removed $filename in ${SystemClock.elapsedRealtime() - gcStartMs}ms keep=$keepFilename",
            )
        }.onFailure {
            Log.e("OfflinePackages", "gc failed for $filename", it)
            val installedArtifact = installedByFilename[filename]
            warnings += OfflinePackagesWarning(
                artifactId = installedArtifact?.artifactId ?: filename,
                familyId = installedArtifact?.artifactId?.let { packagesById[it]?.familyId },
                regionId = installedArtifact?.artifactId?.let { packagesById[it]?.regionId },
                message = it.message ?: it::class.simpleName ?: "gc failed",
            )
        }
    }
    reportProgress("Sync complete: fetched $fetchedCount, GC $gcCount")
    return OfflinePackagesSyncSummary(
        fetchedCount = fetchedCount,
        gcCount = gcCount,
        warnings = warnings,
        remotePoisonedFilenameMessages = remotePoisonedFilenameMessages,
    ).also {
        Log.i(
            "OfflinePackages",
            "sync completed in ${SystemClock.elapsedRealtime() - syncStartMs}ms " +
                "(fetch=${plan.fetch.size}, gc=${plan.gc.size}, warnings=${warnings.size})",
        )
    }
}

private fun syncProgressText(
    fetchedCount: Int,
    fetchCount: Int,
    fetchedBytes: Long,
    totalBytes: Long,
): String =
    if (totalBytes > 0L) {
        "Fetched ${formatProgressMegabytes(fetchedBytes)} / ${formatProgressMegabytes(totalBytes)} ($fetchedCount/$fetchCount)"
    } else {
        "Fetching package $fetchedCount/$fetchCount"
    }

private fun formatProgressMegabytes(bytes: Long): String = "${bytes / 1_000_000L}MB"

private const val PackageHttpConnectTimeoutMs = 5_000
private const val PackageHttpReadTimeoutMs = 5_000

private class ActivePackageConnections {
    private val connections = linkedSetOf<HttpURLConnection>()

    @Synchronized
    fun add(connection: HttpURLConnection) {
        connections += connection
    }

    @Synchronized
    fun remove(connection: HttpURLConnection) {
        connections -= connection
    }

    @Synchronized
    fun disconnectAll() {
        val snapshot = connections.toList()
        connections.clear()
        snapshot.forEach { connection ->
            runCatching { connection.disconnect() }
        }
    }
}

private suspend fun refreshOfflinePackageLibrary(
    packageSourceBaseUrl: String,
    discoveryFilenames: List<String>,
    activeConnections: ActivePackageConnections,
): OfflinePackagesControllerEventWire.LibraryRefreshSucceeded {
    check(packageSourceBaseUrl.isNotBlank()) { "package source URL is blank" }
    val discoveryNames = buildList {
        add("current_artifacts.json")
        addAll(discoveryFilenames)
    }.distinct()
    val discoveryJsons = discoveryNames.map { filename ->
        currentCoroutineContext().ensureActive()
        readPackageSourceText(
            resolvePackageSourceUrl(filename, packageSourceBaseUrl),
            activeConnections,
        )
    }
    val bundleNames = discoveryJsons
        .map { PackageManagementJson.decodeFromString<CurrentArtifactsManifestWire>(it) }
        .flatMap { manifest ->
            manifest.bundles
                .filter { it.bundleType == "cycle" }
                .map { it.filename }
        }
        .distinct()
        .sorted()
    val bundleJsonsByFilename = bundleNames.associateWith { filename ->
        currentCoroutineContext().ensureActive()
        readPackageSourceText(
            resolvePackageSourceUrl(filename, packageSourceBaseUrl),
            activeConnections,
        )
    }
    return OfflinePackagesControllerEventWire.LibraryRefreshSucceeded(
        fetchedAtEpochMs = System.currentTimeMillis(),
        discoveryJsons = discoveryJsons,
        bundleJsonsByFilename = bundleJsonsByFilename,
    )
}

private suspend fun readPackageSourceText(
    sourceUrl: String,
    activeConnections: ActivePackageConnections,
): String =
    readPackageSourceBytes(
        sourceUrl = sourceUrl,
        expectedSizeBytes = null,
        activeConnections = activeConnections,
        onBytesRead = {},
    ).decodeToString()

private suspend fun readPackageSourceBytes(
    sourceUrl: String,
    expectedSizeBytes: Long?,
    activeConnections: ActivePackageConnections,
    onBytesRead: suspend (Long) -> Unit,
): ByteArray {
    val startMs = SystemClock.elapsedRealtime()
    var totalBytesRead = 0L
    val connection = openCancellablePackageConnection(sourceUrl)
    activeConnections.add(connection)
    val completionHandle = currentCoroutineContext()[Job]?.invokeOnCompletion { error ->
        if (error is CancellationException) {
            Log.i("OfflinePackages", "cancel disconnect $sourceUrl")
            connection.disconnect()
        }
    }
    return try {
        Log.i("OfflinePackages", "http read start $sourceUrl")
        connection.inputStream.buffered().use { input ->
            val buffer = ByteArray(64 * 1024)
            val output = expectedSizeBytes
                ?.takeIf { it in 0..Int.MAX_VALUE.toLong() }
                ?.let { java.io.ByteArrayOutputStream(it.toInt()) }
                ?: java.io.ByteArrayOutputStream()
            output.use { bytes ->
                while (true) {
                    currentCoroutineContext().ensureActive()
                    val read = input.read(buffer)
                    currentCoroutineContext().ensureActive()
                    if (read < 0) {
                        break
                    }
                    bytes.write(buffer, 0, read)
                    totalBytesRead += read.toLong()
                    onBytesRead(read.toLong())
                }
                bytes.toByteArray()
            }
        }
    } finally {
        completionHandle?.dispose()
        activeConnections.remove(connection)
        connection.disconnect()
        Log.i(
            "OfflinePackages",
            "http read end bytes=$totalBytesRead elapsedMs=${SystemClock.elapsedRealtime() - startMs} url=$sourceUrl",
        )
    }
}

private fun openCancellablePackageConnection(sourceUrl: String): HttpURLConnection =
    (URL(sourceUrl).openConnection() as HttpURLConnection).apply {
        connectTimeout = PackageHttpConnectTimeoutMs
        readTimeout = PackageHttpReadTimeoutMs
        instanceFollowRedirects = true
        useCaches = false
    }

private suspend fun downloadPackageToTempFile(
    context: Context,
    kind: InstalledPackageKind,
    filename: String,
    sourceUrl: String,
    expectedSizeBytes: Long?,
    expectedSha256: String?,
    activeConnections: ActivePackageConnections,
    onBytesRead: suspend (Long) -> Unit = {},
): File {
    val target = File(File(context.filesDir, kind.directoryName), filename)
    target.parentFile?.mkdirs()
    val temp = File(target.parentFile, "${target.name}.download")
    if (temp.exists()) {
        temp.delete()
    }
    val digest = MessageDigest.getInstance("SHA-256")
    var sizeBytes = 0L
    var complete = false
    val connection = openCancellablePackageConnection(sourceUrl)
    activeConnections.add(connection)
    val completionHandle = currentCoroutineContext()[Job]?.invokeOnCompletion { error ->
        if (error is CancellationException) {
            Log.i("OfflinePackages", "cancel disconnect $sourceUrl")
            connection.disconnect()
        }
    }
    try {
        Log.i("OfflinePackages", "http download start $sourceUrl")
        connection.inputStream.buffered().use { input ->
            BufferedOutputStream(temp.outputStream()).use { output ->
                val buffer = ByteArray(64 * 1024)
                while (true) {
                    currentCoroutineContext().ensureActive()
                    val read = input.read(buffer)
                    currentCoroutineContext().ensureActive()
                    if (read < 0) {
                        break
                    }
                    output.write(buffer, 0, read)
                    digest.update(buffer, 0, read)
                    sizeBytes += read.toLong()
                    onBytesRead(read.toLong())
                }
            }
        }
        complete = true
    } finally {
        completionHandle?.dispose()
        activeConnections.remove(connection)
        connection.disconnect()
        Log.i("OfflinePackages", "http download end $sourceUrl complete=$complete")
        if (!complete) {
            temp.delete()
        }
    }
    expectedSizeBytes?.let { expected ->
        check(sizeBytes == expected) {
            "size mismatch for $filename: expected $expected got $sizeBytes"
        }
    }
    expectedSha256?.let { expected ->
        val actual = digest.digest().joinToString("") { "%02x".format(it) }
        check(actual.equals(expected, ignoreCase = true)) {
            "checksum mismatch for $filename: expected $expected got $actual"
        }
    }
    return temp
}

private fun installDownloadedPackage(
    context: Context,
    kind: InstalledPackageKind,
    artifactId: String,
    filename: String,
    tempFile: File,
    sizeBytes: Long?,
    checksumSha256: String?,
) {
    tempFile.inputStream().buffered().use { source ->
        InstalledPackages.replaceInstalledFileFromStream(
            context = context,
            kind = kind,
            artifactId = artifactId,
            filename = filename,
            source = source,
            sizeBytes = sizeBytes,
            checksumSha256 = checksumSha256,
        )
    }
    tempFile.delete()
}

private fun validateInstalledPackageOrNull(
    context: Context,
    kind: InstalledPackageKind,
    pkg: BundlePackageArtifactWire,
): String? {
    return when (pkg.familyId) {
        "nav-db" -> runCatching {
            val installedFile = InstalledPackages.existingInstalledArtifacts(
                context,
                kind,
                pkg.id,
            ).firstOrNull { it.filename == pkg.filename }?.file
                ?: error("installed file missing after fetch")
            NavKvStore.open(navDbZip = installedFile).use { }
        }.exceptionOrNull()?.let { error ->
            "installed validation failed for ${pkg.filename}: ${error.message ?: error::class.simpleName ?: "unreadable"}"
        }
        else -> null
    }
}

private fun resolvePackageSourceUrl(relativePath: String, packageSourceBaseUrl: String): String =
    when {
        relativePath.startsWith("http://") || relativePath.startsWith("https://") -> relativePath
        packageSourceBaseUrl.endsWith("/") -> "$packageSourceBaseUrl$relativePath"
        else -> "$packageSourceBaseUrl/$relativePath"
    }

private fun formatNavDbStatusLine(status: net.jonh.aerobag.prototype.domain.NavDbStatus): String {
    if (status.installed.isEmpty()) {
        return "NAVDB none installed"
    }
    val parts = status.installed.map { artifact ->
        val cycle = artifact.packageId.split('_').getOrNull(2) ?: artifact.packageId
        if (artifact.readable) "$cycle ok" else "$cycle bad"
    }
    return "NAVDB ${status.installed.size}: ${parts.joinToString(", ")}"
}

private fun installedPackageKindForFamilyId(familyId: String): InstalledPackageKind = when (familyId) {
    "sec", "tac", "shaded-relief", "enr-l", "enr-h" -> InstalledPackageKind.Charts
    "tpp", "csup" -> InstalledPackageKind.Plates
    "nav-db", "vectors", "geo", "terrain" -> InstalledPackageKind.Data
    else -> error("unsupported package family for install: $familyId")
}

private fun deleteInstalledArtifact(
    context: Context,
    artifactId: String,
    filename: String,
    keepFilename: String? = null,
) {
    InstalledPackageKind.entries.forEach { kind ->
        InstalledPackages.deleteInstalledArtifact(context, kind, artifactId, filename, keepFilename)
    }
}

private fun sha256Hex(bytes: ByteArray): String =
    MessageDigest.getInstance("SHA-256")
        .digest(bytes)
        .joinToString("") { "%02x".format(it) }

private fun offlineRegionOptions(): List<OfflinePackageDimension> {
    val labelById = mapOf(
        "ak" to "Alaska",
        "ec" to "East Central",
        "nc" to "North Central",
        "ne" to "Northeast",
        "nw" to "Northwest",
        "pac" to "Pacific",
        "sc" to "South Central",
        "se" to "Southeast",
        "sw" to "Southwest",
    )
    val sortOrder = labelById.keys.withIndex().associate { it.value to it.index }
    return labelById.keys
        .sortedWith(compareBy({ sortOrder[it] ?: Int.MAX_VALUE }, { it }))
        .map { id -> OfflinePackageDimension(id, labelById[id] ?: id.uppercase()) }
}

@OptIn(ExperimentalComposeUiApi::class)
@Composable
private fun MapExplorerPage(
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
    selectedMapId: String,
    viewport: MapViewportState,
    decodedTileBitmapCache: DecodedTileBitmapCache,
    debugFastTiles: Boolean,
    onDebugFastTilesChange: (Boolean) -> Unit,
    pageTilePaintTiming: PageTilePaintTiming?,
    onPageTilePaintTimingComplete: (Long) -> Unit,
    onViewportChange: (MapViewportState) -> Unit,
    onSessionSnapshotChange: (UiSessionSnapshot) -> Unit,
    onPlaybackSourcePathChange: (String) -> Unit,
    onSelectMapId: (String) -> Unit,
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
    var mapSelection by remember { mutableStateOf<MapSelectionUiState?>(null) }
    var debugPanelOpen by remember { mutableStateOf(false) }
    var debugTileLabels by remember { mutableStateOf(false) }
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
                airspacePaths = emptyList(),
                tfrPaths = emptyList(),
                airspaceLabels = emptyList(),
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
    val selectedMap = remember(selectedMapId, fixture.mapViews) {
        fixture.mapViews.find { it.id == selectedMapId } ?: fixture.mapViews.first()
    }
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
    val center = remember(currentViewport) { viewportCenterLatLon(currentViewport) }
    val surfaceWidthPx = surfaceSize.width.toFloat()
    val surfaceHeightPx = surfaceSize.height.toFloat()
    val surfaceWidthDp = remember(surfaceSize, density) { with(density) { surfaceSize.width.toDp().value } }
    val surfaceHeightDp = remember(surfaceSize, density) { with(density) { surfaceSize.height.toDp().value } }
    val tileDisplayMultiplier = if (debugFastTiles) 2.0 else 1.0
    val tiles = remember(selectedMap.id, currentViewport, surfaceSize, fixture.mapViews, uiSession, tileDisplayMultiplier) {
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
                    tileDisplayMultiplier,
                ),
            )
            val planMs = SystemClock.elapsedRealtime() - planStartMs
            pageTilePaintTiming?.let { timing ->
                Log.i(
                    TileBudgetLogTag,
                    "page-to-map-plan id=${timing.id} from=${timing.fromPage} elapsedMs=${SystemClock.elapsedRealtime() - timing.startedMs} planMs=$planMs tiles=${plan.tiles.size} fastTiles=$debugFastTiles",
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
    val trayOptions = remember(selectedMap.id, fixture.mapViews) {
        val secTarget = fixture.mapViews.firstOrNull { it.mapView.chartFamily == MapChartFamily.Sec }
        val tacTarget = fixture.mapViews.firstOrNull { it.mapView.chartFamily == MapChartFamily.Tac }
        val enrLTarget = fixture.mapViews.firstOrNull { it.mapView.chartFamily == MapChartFamily.EnrL }
        val enrHTarget = fixture.mapViews.firstOrNull { it.mapView.chartFamily == MapChartFamily.EnrH }
        val shadedReliefTarget = fixture.mapViews.firstOrNull { it.mapView.chartFamily == MapChartFamily.ShadedRelief }
        listOf(
            ChartTrayOption("sec", "SECTIONAL", "SEC", secTarget != null, R.drawable.sectional_icon) { secTarget?.let { onSelectMapId(it.id) } },
            ChartTrayOption("tac", "TAC", "TAC", tacTarget != null, R.drawable.tac_icon) { tacTarget?.let { onSelectMapId(it.id) } },
            ChartTrayOption("enr-l", "IFR LOW", "IFR LOW", enrLTarget != null, R.drawable.ifr_l_icon) { enrLTarget?.let { onSelectMapId(it.id) } },
            ChartTrayOption("enr-h", "IFR HIGH", "IFR HIGH", enrHTarget != null, R.drawable.ifr_h_icon) { enrHTarget?.let { onSelectMapId(it.id) } },
            ChartTrayOption("shaded-relief", "SHADED RELIEF", "RELIEF", shadedReliefTarget != null, R.drawable.shaded_relief_icon) { shadedReliefTarget?.let { onSelectMapId(it.id) } },
        )
    }
    val layerTrayOptions = remember(mapLayerState) {
        listOf(
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
                key = "metars",
                label = "METARs",
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
    val tileBitmapCache = remember(selectedMap.id, installRevision, tileDisplayMultiplier) {
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
    LaunchedEffect(tiles, selectedMap.id, installRevision, tileDisplayMultiplier) {
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
            "visible map=${selectedMap.id} total=${tiles.size} missing=${missingTiles.size} localCache=${tileBitmapCache.size} decodedLru=${decodedCacheStats.entries}/${decodedCacheStats.bytes}B lruHits=$decodedCacheHits fastTiles=$debugFastTiles groups=[${formatTileBudgetSummary(tiles)}]",
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
    val fixMarkerFillColor = Color(0xFF39D9FF)
    val airportMarkerStrokeColor = Color(0xB3081218)
    val airportToweredFillColor = Color(0xFF4AA3FF)
    val airportUntoweredFillColor = Color(0xFFFF4FD8)
    val vorMarkerColor = Color(0xFF4AA3FF)
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
    LaunchedEffect(uiSession, viewport, surfaceSize, mapLayerState.vectors.visible, mapLayerState.metars.visible, devServerBaseUrl) {
        if (surfaceSize.width <= 0 || surfaceSize.height <= 0) {
            mapOverlayError = null
            return@LaunchedEffect
        }
        val overlayWidthPx = surfaceSize.width.toFloat()
        val overlayHeightPx = surfaceSize.height.toFloat()
        if (!mapLayerState.vectors.visible && !mapLayerState.metars.visible) {
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
                airspacePaths = emptyList(),
                tfrPaths = emptyList(),
                airspaceLabels = emptyList(),
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
            var overlay = uiSession.queryMapOverlay(viewport, overlayWidthPx.toDouble(), overlayHeightPx.toDouble())
            repeat(8) {
                var ingested = false
                if (overlay.neededMetars) {
                    val payloadJson = withContext(Dispatchers.IO) {
                        fetchJsonOrEmpty(
                            resolvePlaybackTraceUrl("/fast-products/metars/metars.json", devServerBaseUrl),
                            """{"schema_version":1,"version_label":"unavailable","metars_by_station":{}}""",
                        )
                    }
                    currentCoroutineContext().ensureActive()
                    uiSession.ingestMetarsJson(payloadJson)
                    ingested = true
                }
                if (overlay.neededMetarTiles.isNotEmpty()) {
                    val tilesJson = withContext(Dispatchers.IO) {
                        overlay.neededMetarTiles.map { tile ->
                            fetchJsonOrEmpty(
                                resolvePlaybackTraceUrl("/fast-products/metars/points/metars/${tile.z}/${tile.x}/${tile.y}.json", devServerBaseUrl),
                                """{"schema_version":1,"layer":"metars","z":${tile.z},"x":${tile.x},"y":${tile.y},"records":[]}""",
                            )
                        }.joinToString(prefix = "[", postfix = "]")
                    }
                    currentCoroutineContext().ensureActive()
                    uiSession.ingestMetarTilesJson(tilesJson)
                    ingested = true
                }
                if (overlay.neededTfrs) {
                    val payloadJson = withContext(Dispatchers.IO) {
                        fetchJsonOrEmpty(
                            resolvePlaybackTraceUrl("/fast-products/tfrs/tfrs.json", devServerBaseUrl),
                            """{"schema_version":1,"version_label":"unavailable","notam_count":0,"area_group_count":0,"areas":[]}""",
                        )
                    }
                    currentCoroutineContext().ensureActive()
                    uiSession.ingestTfrsJson(payloadJson)
                    ingested = true
                }
                if (!ingested) {
                    return@repeat
                }
                currentCoroutineContext().ensureActive()
                overlay = uiSession.queryMapOverlay(viewport, overlayWidthPx.toDouble(), overlayHeightPx.toDouble())
            }
            val (centerLat, centerLon) = viewportCenterLatLon(viewport)
            Log.i(
                MapLayerLogTag,
                "overlay center=${"%.3f".format(centerLat)},${"%.3f".format(centerLon)} zoom=${"%.2f".format(viewport.zoom)} size=${surfaceSize.width}x${surfaceSize.height} vectorsVisible=${mapLayerState.vectors.visible} metarsVisible=${mapLayerState.metars.visible} neededMetars=${overlay.neededMetars} features=${overlay.visibleFeatures.size} airspace=${overlay.airspacePaths.size} airspaceLabels=${overlay.airspaceLabels.size} metars=${overlay.visibleMetars.size} neededPoints=${overlay.neededPointTiles.size} neededAirspaceRefs=${overlay.neededAirspaceRefTiles.size} neededAirspaceFeatures=${overlay.neededAirspaceFeatures.size} neededAirspaceLabels=${overlay.neededAirspaceLabelTiles.size} warnings=${overlay.warnings.size} elapsedMs=${SystemClock.elapsedRealtime() - overlayStartMs}",
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

    Box(
        modifier = Modifier
            .fillMaxSize()
            .background(uiTheme.controls.chartSurfaceBg)
            .onSizeChanged { surfaceSize = it }
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
            .pointerInput(selectedMap.mapView, surfaceSize) {
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
                            if (topLeftTrayOpen) {
                                pressed.forEach { it.consume() }
                                continue
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
                if (debugTileLabels) {
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
        if (displayedMapOverlay.airspacePaths.isNotEmpty() || displayedMapOverlay.tfrPaths.isNotEmpty() || displayedMapOverlay.airspaceLabels.isNotEmpty()) {
            Canvas(modifier = Modifier.fillMaxSize()) {
                (displayedMapOverlay.airspacePaths + displayedMapOverlay.tfrPaths).forEach { feature ->
                    drawAirspaceDisplayPath(feature)
                }
                displayedMapOverlay.airspaceLabels.forEach { label ->
                    drawAirspaceLimitGlyph(
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
                    if (isAirport) {
                        val airportFillColor = if (feature.towered) airportToweredFillColor else airportUntoweredFillColor
                        val airportLabelPaint = if (feature.towered) airportToweredLabelFillPaint else airportUntoweredLabelFillPaint
                        val airportRadius = 12f * densityScale
                        if (feature.fuelAvailable) {
                            val markerPath = airportFuelMarkerPath(center, densityScale)
                            drawPath(markerPath, airportFillColor)
                            drawPath(markerPath, airportMarkerStrokeColor, style = Stroke(width = 2f * densityScale))
                        } else {
                            drawCircle(airportFillColor, radius = airportRadius, center = center)
                            drawCircle(airportMarkerStrokeColor, radius = airportRadius, center = center, style = Stroke(width = 2f * densityScale))
                        }
                        feature.longestRunwayHeadingTrueDeg?.let { headingDeg ->
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
                        val outerHex = polygonPath(vorHexPoints(center, radius))
                        val band = vorBandPath(center, radius)
                        drawPath(band, vorMarkerColor)
                        drawPath(band, vorMarkerStrokeColor, style = Stroke(width = 1.6f * densityScale))
                        drawPath(outerHex, vorMarkerStrokeColor, style = Stroke(width = 1.6f * densityScale))
                        drawContext.canvas.nativeCanvas.apply {
                            val textY = center.y - 24f * densityScale
                            drawText(feature.label, center.x, textY, fixLabelStrokePaint)
                            drawText(feature.label, center.x, textY, vorLabelFillPaint)
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
        }
        if (displayedMapOverlay.visibleMetars.isNotEmpty()) {
            Canvas(modifier = Modifier.fillMaxSize()) {
                displayedMapOverlay.visibleMetars.forEach { feature ->
                    drawMetarSymbol(feature, Offset(feature.screenX.toFloat(), feature.screenY.toFloat()), density.density)
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
                            drawAirspaceDisplayPath(path)
                        }
                    }
                    is MapSelectionHighlight.Metar -> {
                        val feature = displayedMapOverlay.visibleMetars.firstOrNull { it.stationId == highlight.stationId } ?: item.metarFeature
                        if (feature != null) {
                            drawCircle(Color.White, radius = 16f * density.density, center = Offset(feature.screenX.toFloat(), feature.screenY.toFloat()), style = Stroke(width = 4f * density.density))
                            drawMetarSymbol(feature, Offset(feature.screenX.toFloat(), feature.screenY.toFloat()), density.density)
                        }
                    }
                    is MapSelectionHighlight.Spot -> {
                        val point = latLonToScreen(highlight.lat, highlight.lon, currentViewport, surfaceWidthPx, surfaceHeightPx)
                        drawLine(Color(0xE6FFFFFF), point, Offset(point.x, point.y + 32f * density.density), strokeWidth = 6f * density.density, cap = StrokeCap.Round)
                        drawCircle(Color(0xFFFF4FD8), radius = 7f * density.density, center = point)
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
            ownship = ownship,
            modifier = Modifier
                .align(Alignment.TopEnd)
                .padding(top = ThumbGap, end = ThumbGap),
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
        )

        val playbackLeftRoomUnits = surfaceWidthDp / 2f - (ThumbSize.value * 1.5f) - (ThumbGap.value * 2f)
        val playbackBottomPadding =
            if (playbackLeftRoomUnits < ThumbSize.value * 2.8f) {
                ThumbGap + (ThumbSize * 0.67f) + ThumbGap
            } else {
                ThumbGap
            }
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
                        "insert" -> {
                            item.navRef?.let { navRef ->
                                runCatching { uiSession.insertWaypointBestPosition(navRef) }
                                    .onSuccess(onSessionSnapshotChange)
                                    .onFailure { Log.w("AerobagSelection", "insert waypoint failed", it) }
                                mapSelection = null
                            }
                        }
                        "remove_from_flight_plan" -> {
                            item.navRef?.let { navRef ->
                                runCatching { uiSession.removeTopLevelWaypointByNavRef(navRef) }
                                    .onSuccess(onSessionSnapshotChange)
                                    .onFailure { Log.w("AerobagSelection", "remove waypoint failed", it) }
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

        DebugDock(
            open = debugPanelOpen,
            onToggle = { debugPanelOpen = !debugPanelOpen },
            highlight = committedMapOverlay.warnings.isNotEmpty() || mapOverlayError != null,
            expandAbove = true,
            modifier = Modifier
                .align(Alignment.BottomEnd)
                .padding(end = ThumbSize + (ThumbGap * 2f)),
        ) {
            Text("up: $uptimeLabel", style = MaterialTheme.typography.labelSmall, color = Color(0xFF52656D))
            Text("${String.format("%.3f", center.first)}/${String.format("%.3f", center.second)} z${String.format("%.2f", viewport.zoom)}", style = MaterialTheme.typography.labelSmall, color = Color(0xFF52656D))
            Row(
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(4.dp),
            ) {
                Checkbox(
                    checked = debugTileLabels,
                    onCheckedChange = { debugTileLabels = it },
                    modifier = Modifier.size(ThumbSize * 0.36f),
                )
                Text("tile labels", style = MaterialTheme.typography.labelSmall, color = Color(0xFF52656D))
            }
            Row(
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(4.dp),
            ) {
                Checkbox(
                    checked = debugFastTiles,
                    onCheckedChange = onDebugFastTilesChange,
                    modifier = Modifier.size(ThumbSize * 0.36f),
                )
                Text("fast tiles", style = MaterialTheme.typography.labelSmall, color = Color(0xFF52656D))
            }
        }
    }
}

@Composable
private fun MapSelectionTray(
    state: MapSelectionUiState,
    modifier: Modifier,
    onSelectItem: (MapSelectionItem) -> Unit,
    onSelectAction: (MapSelectionItem, MapSelectionAction) -> Unit,
) {
    val uiTheme = LocalAerobagUiTheme.current
    val selectedItem = state.selectedItem
    val actionSlots = selectedItem?.actions.orEmpty()
    val visibleActions = if (selectedItem?.detailText != null) actionSlots.take(3) else actionSlots.take(6)
    Surface(
        modifier = modifier.width(ThumbSize * 4.4f),
        shape = RoundedCornerShape(ThumbRadius),
        color = uiTheme.controls.panelBg.copy(alpha = 0.96f),
        contentColor = uiTheme.controls.panelFg,
        shadowElevation = 8.dp,
        border = BorderStroke(1.dp, uiTheme.controls.panelBorder.copy(alpha = 0.85f)),
    ) {
        Column(modifier = Modifier.padding(ThumbGap * 0.7f), verticalArrangement = Arrangement.spacedBy(ThumbGap * 0.55f)) {
            state.result.categories.forEach { category ->
                Row(horizontalArrangement = Arrangement.spacedBy(ThumbGap * 0.45f)) {
                    if (category.items.isEmpty()) {
                        Text(
                            text = "no ${category.label.lowercase()}s",
                            modifier = Modifier.height(ThumbSize).width(ThumbSize * 1.28f).wrapContentSize(),
                            style = MaterialTheme.typography.labelSmall,
                            color = Color(0xFF697780),
                            textAlign = TextAlign.Center,
                        )
                    } else {
                        category.items.take(3).forEach { item ->
                            MapSelectionItemButton(
                                item = item,
                                selected = item.id == selectedItem?.id,
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
                    Row(horizontalArrangement = Arrangement.spacedBy(ThumbGap * 0.45f)) {
                        visibleActions.forEach { action ->
                            MapSelectionActionButton(
                                action = action,
                                enabled = action.enabled && !action.displayOnly,
                                onClick = {
                                    if (selectedItem != null) onSelectAction(selectedItem, action)
                                },
                            )
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
private fun MapSelectionItemButton(
    item: MapSelectionItem,
    selected: Boolean,
    onClick: () -> Unit,
) {
    val uiTheme = LocalAerobagUiTheme.current
    Surface(
        modifier = Modifier
            .size(ThumbSize)
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
private fun MapSelectionItemIcon(item: MapSelectionItem, modifier: Modifier) {
    when {
        item.symbolFeature != null -> PlanWaypointSymbol(item.symbolFeature, modifier)
        item.metarFeature != null -> Canvas(modifier = modifier) {
            drawMetarSymbol(item.metarFeature, Offset(size.width / 2f, size.height / 2f), density)
        }
        item.highlight is MapSelectionHighlight.Spot -> Canvas(modifier = modifier) {
            val center = Offset(size.width / 2f, size.height * 0.45f)
            drawLine(Color(0xD0081218), center, Offset(center.x, size.height * 0.9f), strokeWidth = 5f, cap = StrokeCap.Round)
            drawLine(Color(0xFFFF4FD8), center, Offset(center.x, size.height * 0.9f), strokeWidth = 2.4f, cap = StrokeCap.Round)
            drawCircle(Color(0xFFFF4FD8), radius = 5f, center = center)
        }
        item.airspaceIcon != null -> Canvas(modifier = modifier) {
            drawAirspaceDisplayPath(item.airspaceIcon)
        }
        else -> Box(modifier = modifier, contentAlignment = Alignment.Center) {
            Text(item.sublabel.ifBlank { item.label }, style = MaterialTheme.typography.labelSmall, textAlign = TextAlign.Center)
        }
    }
}

@Composable
private fun MapSelectionActionButton(
    action: MapSelectionAction,
    enabled: Boolean,
    onClick: () -> Unit,
) {
    val uiTheme = LocalAerobagUiTheme.current
    Surface(
        modifier = Modifier
            .width(ThumbSize * 1.2f)
            .height(ThumbSize)
            .alpha(if (action.label.isBlank()) 0f else 1f)
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
                    drawAirspaceLimitGlyph(action.airspaceLimit, Offset(size.width / 2f, size.height / 2f), 1.45f)
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
private fun AirportInsertPanel(
    state: AndroidAirportInsertState,
    modifier: Modifier,
    onTextChange: (String) -> Unit,
    onSubmit: () -> Unit,
    onSuggestionClick: (WaypointIdentifierSuggestion) -> Unit,
) {
    val focusRequester = remember { FocusRequester() }
    val keyboardController = LocalSoftwareKeyboardController.current
    LaunchedEffect(state.componentIndex, state.before) {
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

@Composable
private fun FlightPlanPage(
    appCore: NativeAppCoreAdapter,
    page: AppPage,
    pageHistory: List<AppViewSnapshot>,
    uptimeLabel: String,
    navElement: NavElementUiView?,
    samplePlan: net.jonh.aerobag.prototype.domain.FlightPlan,
    planUiState: FlightPlanUiState?,
    planListState: LazyListState,
    uiTheme: UiTheme,
    onSelectPage: (AppPage) -> Unit,
    onOpenPlan: () -> Unit,
    onOpenCharts: (String?) -> Unit,
    onApplyMutation: (FlightPlanUiMutation) -> Unit,
) {
    val planWaypointTrayStart = ThumbGap + PlanArrowLane + ThumbSize * 2.5f + PlanGridGap
    val density = LocalDensity.current
    var selectedWaypointIndex by remember { mutableStateOf<Int?>(null) }
    var selectedWaypointTrayAnchor by remember { mutableStateOf<Dp?>(null) }
    var pendingSelectedRowKey by remember { mutableStateOf<String?>(null) }
    var reorderOpen by remember { mutableStateOf(false) }
    var debugPanelOpen by remember { mutableStateOf(false) }
    var airwayPicker by remember { mutableStateOf<AndroidAirwayPickerState?>(null) }
    var procedurePicker by remember { mutableStateOf<AndroidProcedurePickerState?>(null) }
    var airportInsert by remember { mutableStateOf<AndroidAirportInsertState?>(null) }
    var trayOpenedAtMs by remember { mutableStateOf(0L) }
    val projectedPlanUiState = requireNotNull(planUiState) { "FlightPlanPage requires core-projected FlightPlanUiState" }
    val guidance = projectedPlanUiState.guidance
    val componentViews = remember(projectedPlanUiState.components) { projectedPlanUiState.components }
    val topLevelOrderSummary = remember(componentViews) {
        componentViews.joinToString(" | ") { component ->
            val label =
                when (component.kind) {
                    RouteComponentViewKind.Waypoint -> navRefLabel(component.items.filterIsInstance<ConcretizedNavItem.Waypoint>().firstOrNull()?.navRef ?: component.precedingWaypoint ?: component.followingWaypoint ?: NavRef.Fix("?"))
                    RouteComponentViewKind.Airway,
                    RouteComponentViewKind.Procedure,
                    -> structuredComponentLabel(component)
                }
            "${component.componentIndex}:$label"
        }
    }
    val rows = remember(projectedPlanUiState.displayRows) {
        buildFlightPlanDisplayRows(projectedPlanUiState)
    }
    val blocks = remember(rows) {
        buildFlightPlanDisplayBlocks(rows)
    }
    var structuredSurfaceBounds by remember { mutableStateOf<Rect?>(null) }
    val structuredRowBounds = remember { mutableStateMapOf<String, Rect>() }
    val selectedRow = selectedWaypointIndex?.let(rows::getOrNull)
    val selectedRowBounds = selectedRow?.let { structuredRowBounds[it.id] }
    val waypointTrayStart = planWaypointTrayStart
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
                        reorderOpen -> 2
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
                        else -> selectedRow?.actions?.size ?: 1
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
                        reorderOpen -> 2
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
                        else -> selectedRow.actions.size
                    }.coerceAtLeast(1)
                val estimatedHeight = estimateTrayHeightDp(estimatedRows)
                val paneBottom = with(density) { (surfaceBounds.bottom - surfaceBounds.top).toDp() } + defaultTop
                val maxTop = (paneBottom - estimatedHeight - ThumbSize * 0.1f).coerceAtLeast(paneTop)
                desiredTop.coerceIn(paneTop, maxTop)
            }
        }
    val waypointTrayWidth = ThumbSize * 2.35f
    val structuredArrow =
        remember(rows, guidance?.activeLeg, structuredSurfaceBounds, structuredRowBounds.toMap(), density) {
            val surfaceBounds = structuredSurfaceBounds ?: return@remember null
            val activeLeg = guidance?.activeLeg ?: return@remember null
            val visibleIndices =
                rows.mapIndexedNotNull { index, row ->
                    if (structuredRowBounds.containsKey(row.id)) index else null
                }
            val firstVisibleIndex = visibleIndices.minOrNull()
            val lastVisibleIndex = visibleIndices.maxOrNull()
            val fromIndex =
                rows.indexOfFirst { row ->
                    row.rowKind == "waypoint" && navRefsEqual(row.navRef, activeLeg.from)
                }
            if (fromIndex < 0) {
                return@remember null
            }
            var toIndex = -1
            for (index in (fromIndex + 1) until rows.size) {
                val row = rows[index]
                if (row.rowKind == "waypoint" && navRefsEqual(row.navRef, activeLeg.to)) {
                    toIndex = index
                    break
                }
            }
            val lanePx = with(density) { PlanArrowLane.toPx() }
            val headLength = with(density) { 12.dp.toPx() }
            val textInsetPx = with(density) { PlanArrowButtonInset.toPx() }
            val surfaceHeight = surfaceBounds.height
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
            val fromEndpoint = rowPoint(fromIndex, preferBelow = false) ?: return@remember null
            val toEndpoint =
                if (toIndex >= 0) {
                    rowPoint(toIndex, preferBelow = toIndex > fromIndex)
                } else {
                    StructuredArrowEndpoint(
                        point = Offset(x = fromEndpoint.point.x, y = surfaceHeight),
                        clipped = true,
                        clippedAbove = false,
                        clippedBelow = true,
                    )
                } ?: return@remember null
            if (fromEndpoint.clipped && toEndpoint.clipped) {
                return@remember null
            }
            if (toEndpoint.clippedAbove) {
                return@remember null
            }
            val fromPoint = fromEndpoint.point
            val toPoint = toEndpoint.point
            val elbowX = lanePx * 0.25f
            StructuredArrowSpec(
                fromPoint = fromPoint,
                toPoint = toPoint,
                toClipped = toEndpoint.clipped,
                fromClippedAbove = fromEndpoint.clippedAbove,
                elbowX = elbowX,
                shaftEndX = maxOf(elbowX, toPoint.x - headLength + with(density) { 1.5.dp.toPx() }),
                headLength = headLength,
            )
        }

    fun closePanels() {
        selectedWaypointIndex = null
        selectedWaypointTrayAnchor = null
        pendingSelectedRowKey = null
        reorderOpen = false
        airwayPicker = null
        procedurePicker = null
        airportInsert = null
    }

    LaunchedEffect(airportInsert?.componentIndex, airportInsert?.before, airportInsert?.airportId, samplePlan) {
        val editor = airportInsert ?: return@LaunchedEffect
        val prefix = editor.airportId.trim().uppercase()
        if (prefix.isEmpty()) {
            airportInsert = editor.copy(loading = false, suggestions = emptyList())
            return@LaunchedEffect
        }
        airportInsert = editor.copy(loading = true)
        runCatching {
            withContext(Dispatchers.IO) {
                appCore.suggestWaypointIdentifiers(samplePlan, editor.componentIndex, editor.before, prefix, 8)
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
            reorderOpen = false
        }
        pendingSelectedRowKey = null
    }

    LaunchedEffect(topLevelOrderSummary) {
        Log.d("AerobagReorder", "topLevelOrder $topLevelOrderSummary")
    }

    Box(
        modifier = Modifier
            .fillMaxSize()
            .background(uiTheme.controls.chartSurfaceBg),
    ) {
        CompactSquareButton(
            label = "HOME",
            modifier = Modifier
                .align(Alignment.TopStart)
                .padding(ThumbGap)
                .size(ThumbSize),
            selected = page == AppPage.Home,
            onClick = { onSelectPage(AppPage.Home) },
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
                        modifier = Modifier.weight(1f),
                        verticalArrangement = Arrangement.spacedBy(PlanGridGap),
                    ) {
                        items(blocks.size) { blockIndex ->
                            when (val block = blocks[blockIndex]) {
                                is FlightPlanDisplayBlock.Single -> {
                                    FlightPlanDataRow(
                                        row = block.row,
                                        selected = selectedWaypointIndex == block.index,
                                        reorderOpen = reorderOpen,
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
                                            reorderOpen = false
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
                                        reorderOpen = reorderOpen,
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
                                            reorderOpen = false
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
                                            reorderOpen = false
                                            airwayPicker = null
                                            procedurePicker = null
                                            airportInsert = null
                                        },
                                    )
                                }
                            }
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
                    onClick = { onApplyMutation(appCore.activateNextLegUi(samplePlan)) },
                )
                CompactSquareButton(
                    label = "Sequence",
                    modifier = Modifier.width(ThumbSize * 1.8f).height(ThumbSize),
                    enabled = guidance?.canSequenceActiveLeg == true,
                    onClick = { onApplyMutation(appCore.sequenceActiveLegUi(samplePlan)) },
                )
                CompactSquareButton(
                    label = "Suspend",
                    modifier = Modifier.width(ThumbSize * 1.8f).height(ThumbSize),
                    enabled = guidance?.canSuspend == true,
                    onClick = { onApplyMutation(appCore.suspendSequencingUi(samplePlan)) },
                )
                CompactSquareButton(
                    label = "Unsusp",
                    modifier = Modifier.width(ThumbSize * 1.8f).height(ThumbSize),
                    enabled = guidance?.canUnsuspend == true,
                    onClick = { onApplyMutation(appCore.unsuspendSequencingUi(samplePlan)) },
                )
            }
            NavElementDock(
                navElement = navElement,
                onClick = onOpenPlan,
                modifier = Modifier.align(Alignment.BottomCenter),
            )
        }

        DebugDock(
            open = debugPanelOpen,
            onToggle = { debugPanelOpen = !debugPanelOpen },
            modifier = Modifier.align(Alignment.BottomStart),
        ) {
            Text("up: $uptimeLabel", style = MaterialTheme.typography.labelSmall, color = Color(0xFF52656D))
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
                                appCore.insertWaypointUi(samplePlan, editor.componentIndex, editor.before, waypoint)
                            }.onSuccess { mutation ->
                                onApplyMutation(mutation)
                                closePanels()
                            }.onFailure { error ->
                                airportInsert = editor.copy(error = error.message ?: error.toString())
                            }
                        },
                        onSuggestionClick = { suggestion ->
                            runCatching {
                                appCore.insertWaypointUi(samplePlan, editor.componentIndex, editor.before, suggestion.navRef)
                            }.onSuccess { mutation ->
                                onApplyMutation(mutation)
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
                                        appCore.materializeProcedureSelection(
                                            picker.airportId,
                                            picker.selectedProcedureId,
                                            ProcedureKind.Approach,
                                            null,
                                            choice.enrouteTransition,
                                            picker.startComponentIndex + 1,
                                        )
                                    }.map { built ->
                                        appCore.insertProcedureMaterializedUi(
                                            samplePlan,
                                            picker.startComponentIndex,
                                            picker.endComponentIndex,
                                            built,
                                        )
                                    }.onSuccess { mutation ->
                                        onApplyMutation(mutation)
                                        closePanels()
                                    }.onFailure { error ->
                                        Log.e(
                                            "AerobagProcedure",
                                            "materialize/insert procedure failed airport=${picker.airportId} procedure=${picker.selectedProcedureId} enroute=${choice.enrouteTransition}",
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
                                        val startComponentIndex = if (picker.mode == "replace" && picker.componentIndex != null) {
                                            picker.componentIndex
                                        } else {
                                            picker.startComponentIndex ?: error("missing insertion start")
                                        }
                                        appCore.materializeAirwayPresentationSelection(
                                            startComponentIndex,
                                            presentation,
                                            picker.selectedEntryIndex,
                                            exitIndex,
                                            picker.originAnchor,
                                            picker.destinationAnchor,
                                        )
                                    }.map { built ->
                                        if (picker.mode == "replace" && picker.componentIndex != null) {
                                            appCore.replaceAirwayMaterializedUi(
                                                samplePlan,
                                                picker.componentIndex,
                                                built.selection,
                                                built.airway,
                                                built.resolvedLegs,
                                            )
                                        } else {
                                            appCore.insertAirwayMaterializedUi(
                                                samplePlan,
                                                picker.startComponentIndex ?: error("missing insertion start"),
                                                picker.endComponentIndex,
                                                built.selection,
                                                built.airway,
                                                built.resolvedLegs,
                                            )
                                        }
                                    }.onSuccess { mutation ->
                                        onApplyMutation(mutation)
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
            } else if (reorderOpen) {
                BoxWithConstraints(
                    modifier = Modifier
                        .fillMaxSize()
                        .zIndex(5f),
                ) {
                    val trayWidth = minOf(ThumbSize * 4f, maxWidth - waypointTrayStart - ThumbGap)
                    MenuPanel(
                        modifier = Modifier
                            .align(Alignment.TopStart)
                            .padding(top = waypointTrayTop, start = waypointTrayStart, end = ThumbGap),
                        width = trayWidth,
                    ) {
                        Column(
                            modifier = Modifier.fillMaxWidth(),
                            verticalArrangement = Arrangement.spacedBy(ThumbGap),
                            horizontalAlignment = Alignment.CenterHorizontally,
                        ) {
                            CompactSquareButton(
                                label = "Up",
                                modifier = Modifier.size(ThumbSize),
                                enabled = selectedRow.componentIndex != null && selectedRow.canReorderUp,
                                onClick = {
                                    selectedRow.componentIndex?.let {
                                        Log.d(
                                            "AerobagReorder",
                                            "request move dir=-1 selectedIndex=$selectedWaypointIndex key=${selectedRow.selectionKey} component=$it row=${selectedRow.label} orderBefore=$topLevelOrderSummary",
                                        )
                                        pendingSelectedRowKey = selectedRow.selectionKey
                                        onApplyMutation(appCore.moveComponentUi(samplePlan, it, -1))
                                    }
                                },
                            )
                            CompactSquareButton(
                                label = "Down",
                                modifier = Modifier.size(ThumbSize),
                                enabled = selectedRow.componentIndex != null && selectedRow.canReorderDown,
                                onClick = {
                                    selectedRow.componentIndex?.let {
                                        Log.d(
                                            "AerobagReorder",
                                            "request move dir=1 selectedIndex=$selectedWaypointIndex key=${selectedRow.selectionKey} component=$it row=${selectedRow.label} orderBefore=$topLevelOrderSummary",
                                        )
                                        pendingSelectedRowKey = selectedRow.selectionKey
                                        onApplyMutation(appCore.moveComponentUi(samplePlan, it, 1))
                                    }
                                },
                            )
                        }
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
                    selectedRow.actions.forEach { action ->
                        MenuPanelRow(
                            label = action.label,
                            active = false,
                            enabled = action.enabled,
                            onSelect = {
                                if (!action.enabled) {
                                    return@MenuPanelRow
                                }
                                when (action.id) {
                                    "activate_leg" -> {
                                        selectedRow.legIndex?.let {
                                            onApplyMutation(appCore.activateLegUi(samplePlan, it))
                                        }
                                        closePanels()
                                    }
                                    "remove",
                                    "remove_all_above",
                                    "remove_airway",
                                    "remove_procedure",
                                    -> {
                                        selectedRow.componentIndex?.let {
                                            onApplyMutation(
                                                if (action.id == "remove_all_above") {
                                                    appCore.removeAllAboveUi(samplePlan, it)
                                                } else {
                                                    appCore.deleteComponentUi(samplePlan, it)
                                                },
                                            )
                                        }
                                        closePanels()
                                    }
                                    "reorder" -> {
                                        reorderOpen = true
                                    }
                                    "insert_before",
                                    "insert_after",
                                    -> {
                                        val componentIndex = selectedRow.componentIndex ?: return@MenuPanelRow
                                        airportInsert =
                                            AndroidAirportInsertState(
                                                componentIndex = componentIndex,
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
                                                mode = "insert",
                                                componentIndex = null,
                                                startComponentIndex = selectedRow.startComponentIndex,
                                                endComponentIndex = selectedRow.endComponentIndex,
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
                                    "change_airway" -> {
                                        val componentIndex = selectedRow.componentIndex ?: return@MenuPanelRow
                                        airwayPicker =
                                            AndroidAirwayPickerState(
                                                loading = true,
                                                error = null,
                                                mode = "replace",
                                                componentIndex = componentIndex,
                                                startComponentIndex = null,
                                                endComponentIndex = null,
                                                originAnchor = selectedRow.originAnchor ?: return@MenuPanelRow,
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
                                        val componentIndex = selectedRow.componentIndex ?: return@MenuPanelRow
                                        procedurePicker =
                                            AndroidProcedurePickerState(
                                                loading = true,
                                                error = null,
                                                airportId = airportId,
                                                startComponentIndex = componentIndex - 1,
                                                endComponentIndex = componentIndex,
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
                            }
                        )
                    }
                }
            }
        }
    }
}

@OptIn(ExperimentalComposeUiApi::class)
@Composable
private fun ChartsPage(
    page: AppPage,
    pageHistory: List<AppViewSnapshot>,
    uptimeLabel: String,
    airports: List<ChartAirport>,
    selectedAirport: ChartAirport?,
    selectedChart: ChartAsset?,
    uiTheme: UiTheme,
    ownship: OwnshipRenderState,
    navElement: NavElementUiView?,
    folderOpen: Boolean,
    viewport: net.jonh.aerobag.prototype.domain.ImageViewportState?,
    onViewportChange: (net.jonh.aerobag.prototype.domain.ImageViewportState?) -> Unit,
    onFolderOpenChange: (Boolean) -> Unit,
    onSelectPage: (AppPage) -> Unit,
    onOpenPlan: () -> Unit,
    onSelectAirport: (String) -> Unit,
    onSelectChart: (String) -> Unit,
) {
    val context = LocalContext.current
    val activity = context as? MainActivity
    val density = LocalDensity.current
    val focusRequester = remember { FocusRequester() }
    val chartLabelsById = remember(airports) {
        airports.flatMap { airport -> airport.charts }.associate { chart -> chart.id to chart.label }
    }
    var airportTrayOpen by remember { mutableStateOf(false) }
    var chartTrayOpen by remember { mutableStateOf(false) }
    var debugPanelOpen by remember { mutableStateOf(false) }
    var surfaceSize by remember { mutableStateOf(IntSize.Zero) }
    val sortedCharts = selectedAirport?.charts ?: emptyList()
    val overscrollPx = with(density) { ThumbSize.toPx() }
    val bitmap by produceState<androidx.compose.ui.graphics.ImageBitmap?>(initialValue = null, selectedChart?.id, selectedChart?.assetPath) {
        val chart = selectedChart
        val path = chart?.assetPath
        value = if (path == null) {
            null
        } else {
            withContext(Dispatchers.IO) {
                runCatching {
                    val localFile = java.io.File(context.filesDir, path)
                    val inputStream =
                        if (localFile.isFile) {
                            localFile.inputStream()
                        } else {
                            val chartBytes = ChartPackages.loadChartBytes(context, chart) ?: context.assets.open(path).use { it.readBytes() }
                            chartBytes.inputStream()
                        }
                    inputStream.use { stream ->
                        BitmapFactory.decodeStream(stream)?.asImageBitmap()
                    }
                }.getOrNull()
            }
        }
    }
    val viewportState = rememberUpdatedState(viewport)
    val imageWidthPx = bitmap?.width?.toFloat() ?: 0f
    val imageHeightPx = bitmap?.height?.toFloat() ?: 0f
    val trayOpen = airportTrayOpen || chartTrayOpen

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
                uiTheme = uiTheme,
                onSelectChart = {
                    onSelectChart(it)
                },
            )
        } else {
            Canvas(modifier = Modifier.fillMaxSize()) {
                val currentViewport = viewport
                val currentBitmap = bitmap
                if (currentViewport != null && currentBitmap != null) {
                    val displaySize = imageDisplaySize(
                        imageWidthPx = currentBitmap.width.toFloat(),
                        imageHeightPx = currentBitmap.height.toFloat(),
                        viewportWidthPx = surfaceSize.width.toFloat(),
                        viewportHeightPx = surfaceSize.height.toFloat(),
                        zoom = currentViewport.zoom,
                    )
                    drawImage(
                        image = currentBitmap,
                        dstOffset = IntOffset(currentViewport.leftPx.roundToInt(), currentViewport.topPx.roundToInt()),
                        dstSize = IntSize(displaySize.widthPx.roundToInt(), displaySize.heightPx.roundToInt()),
                    )
                    drawRect(
                        color = Color(0x14000000),
                        topLeft = Offset(currentViewport.leftPx, currentViewport.topPx),
                        size = Size(displaySize.widthPx, displaySize.heightPx),
                        style = Stroke(width = 1.dp.toPx()),
                    )
                }
            }
        }

        SituationStatusBadge(
            ownship = ownship,
            modifier = Modifier
                .align(Alignment.TopEnd)
                .padding(top = ThumbGap, end = ThumbGap),
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
            onSelectPage = {
                onSelectPage(it)
                airportTrayOpen = false
                chartTrayOpen = false
            },
            onToggleAirportTray = {
                airportTrayOpen = !airportTrayOpen
                chartTrayOpen = false
            },
            onToggleChartTray = {
                chartTrayOpen = !chartTrayOpen
                airportTrayOpen = false
            },
            onToggleFolder = {
                onFolderOpenChange(!folderOpen)
                airportTrayOpen = false
                chartTrayOpen = false
            },
            onSelectAirport = {
                onSelectAirport(it)
                airportTrayOpen = false
            },
            onSelectChart = {
                onSelectChart(it)
                chartTrayOpen = false
            },
        )

        if (trayOpen) {
            Scrim {
                airportTrayOpen = false
                chartTrayOpen = false
            }
        }

        NavElementDock(
            navElement = navElement,
            onClick = onOpenPlan,
            modifier = Modifier
                .align(Alignment.BottomCenter)
                .padding(bottom = ThumbGap),
        )

        DebugDock(
            open = debugPanelOpen,
            onToggle = { debugPanelOpen = !debugPanelOpen },
            modifier = Modifier.align(Alignment.BottomStart),
        ) {
            Text("up: $uptimeLabel", style = MaterialTheme.typography.labelSmall, color = Color(0xFF52656D))
            Text(viewport?.let { "z${String.format("%.2f", it.zoom)}" } ?: "viewport (none)", style = MaterialTheme.typography.labelSmall, color = Color(0xFF52656D))
        }

    }
}

@Composable
private fun ChartPlateToggleButton(
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
private fun PageToggleIndicator(
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
private fun MapTopLeftControls(
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
) {
    Row(
        modifier = modifier.padding(ThumbGap),
        horizontalArrangement = Arrangement.spacedBy(ThumbGap),
        verticalAlignment = Alignment.Top,
    ) {
        CompactSquareButton(
            label = "HOME",
            modifier = Modifier.size(ThumbSize),
            selected = currentPage == AppPage.Home,
            onClick = { onSelectPage(AppPage.Home) },
        )
        ChartPlateToggleButton(
            currentPage = currentPage,
            onSelectPage = onSelectPage,
        )
        MenuDock(
            launcherLabel = selectedLabel,
            launcherIconResId = trayOptions.firstOrNull { it.launcherLabel == selectedLabel }?.iconResId,
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
            open = layerTrayOpen,
            onToggle = onToggleLayerTray,
            style = MenuDockStyle.Layers,
            options = layerOptions,
        )
    }
}

@Composable
private fun ChartViewerSelectors(
    modifier: Modifier = Modifier,
    currentPage: AppPage,
    airports: List<ChartAirport>,
    selectedAirport: ChartAirport?,
    selectedChart: ChartAsset?,
    folderOpen: Boolean,
    airportTrayOpen: Boolean,
    chartTrayOpen: Boolean,
    onSelectPage: (AppPage) -> Unit,
    onToggleAirportTray: () -> Unit,
    onToggleChartTray: () -> Unit,
    onToggleFolder: () -> Unit,
    onSelectAirport: (String) -> Unit,
    onSelectChart: (String) -> Unit,
) {
    val uiTheme = LocalAerobagUiTheme.current
    val trayOpen = airportTrayOpen || chartTrayOpen
    Row(
        modifier = modifier.padding(ThumbGap),
        horizontalArrangement = Arrangement.spacedBy(ThumbGap),
        verticalAlignment = Alignment.Top,
    ) {
        CompactSquareButton(
            label = "HOME",
            modifier = Modifier.size(ThumbSize),
            selected = currentPage == AppPage.Home,
            onClick = { onSelectPage(AppPage.Home) },
        )

        ChartPlateToggleButton(
            currentPage = currentPage,
            onSelectPage = onSelectPage,
        )

        MenuDock(
            launcherLabel = selectedAirport?.id ?: "---",
            open = airportTrayOpen,
            onToggle = onToggleAirportTray,
            style = MenuDockStyle.PlateAirport,
            options = airports.map { airport ->
                MenuDockOption(airport.id, airport.id, active = airport.id == selectedAirport?.id) { onSelectAirport(airport.id) }
            },
        )

        MenuDock(
            launcherLabel = selectedChart?.label ?: "---",
            open = chartTrayOpen,
            onToggle = onToggleChartTray,
            style = MenuDockStyle.PlateWide,
            options = (selectedAirport?.charts ?: emptyList()).map { chart ->
                MenuDockOption(
                    chart.id,
                    chart.label,
                    active = chart.id == selectedChart?.id,
                    accentColor = plateFolderColor(uiTheme, chart.folderCategory),
                ) { onSelectChart(chart.id) }
            },
        )

        CompactSquareButton(
            label = "FLDR",
            modifier = Modifier.size(ThumbSize),
            enabled = !trayOpen && !folderOpen,
            onClick = onToggleFolder,
        )
    }
}

@Composable
private fun PlateFolderGrid(
    modifier: Modifier = Modifier,
    charts: List<ChartAsset>,
    selectedChartId: String?,
    uiTheme: UiTheme,
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
            val thumbnail by produceState<androidx.compose.ui.graphics.ImageBitmap?>(initialValue = null, chart.id, chart.thumbnailSourceAssetPath) {
                value = chart.thumbnailSourceAssetPath?.let {
                    withContext(Dispatchers.IO) {
                        runCatching {
                            val bytes = ChartPackages.loadThumbnailBytes(context, chart) ?: return@runCatching null
                            BitmapFactory.decodeByteArray(bytes, 0, bytes.size)?.asImageBitmap()
                        }.getOrNull()
                    }
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
private fun MenuDock(
    modifier: Modifier = Modifier,
    launcherLabel: String,
    @DrawableRes launcherIconResId: Int? = null,
    open: Boolean,
    onToggle: () -> Unit,
    style: MenuDockStyle,
    options: List<MenuDockOption>,
) {
    val density = LocalDensity.current
    val configuration = LocalConfiguration.current
    var anchorTopPx by remember { mutableStateOf(0f) }
    val screenHeightPx = with(density) { configuration.screenHeightDp.dp.toPx() }
    val trayOffsetPx = with(density) { (ThumbSize + ThumbGap).toPx() }
    val trayBottomMarginPx = with(density) { ThumbGap.toPx() }
    val trayMaxHeight = with(density) {
        ((screenHeightPx - anchorTopPx - trayOffsetPx - trayBottomMarginPx).coerceAtLeast(ThumbSize.toPx())).toDp()
    }
    val launcherAccentColor = options.firstOrNull { it.active }?.accentColor
    Box(
        modifier = modifier
            .width(style.buttonWidth)
            .height(ThumbSize)
            .wrapContentSize(unbounded = true, align = Alignment.TopStart),
    ) {
        CompactSquareButton(
            label = launcherLabel,
            iconResId = launcherIconResId,
            maxLines = style.launcherMaxLines,
            enabled = true,
            accentColor = launcherAccentColor,
            wide = style != MenuDockStyle.Compact,
            modifier = Modifier
                .width(style.buttonWidth)
                .height(ThumbSize)
                .align(Alignment.TopStart)
                .onGloballyPositioned { coordinates ->
                    anchorTopPx = coordinates.boundsInWindow().top
                },
            onClick = onToggle,
        )
        if (open) {
            Popup(
                offset = IntOffset(0, trayOffsetPx.roundToInt()),
            ) {
                MenuPanel(
                    modifier = Modifier
                        .width(style.trayWidth)
                        .heightIn(max = trayMaxHeight),
                ) {
                    LazyColumn(verticalArrangement = Arrangement.spacedBy(3.dp)) {
                        lazyColumnItems(options) { option ->
                            MenuPanelRow(
                                label = option.label,
                                active = option.active,
                                enabled = option.enabled,
                                accentColor = option.accentColor,
                                toggleState = option.toggleState,
                                iconResId = option.iconResId,
                                width = style.trayWidth,
                                onSelect = option.onSelect,
                            )
                        }
                    }
                }
            }
        }
    }
}

@Composable
private fun MenuPanel(
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
private fun MenuPanelRow(
    label: String,
    active: Boolean,
    enabled: Boolean,
    accentColor: Color? = null,
    toggleState: UiMapLayerToggleState? = null,
    @DrawableRes iconResId: Int? = null,
    width: Dp = Dp.Unspecified,
    onSelect: () -> Unit,
) {
    val uiTheme = LocalAerobagUiTheme.current
    val rowShape = RoundedCornerShape(ThumbRadius)
    val isOn = toggleState?.enabled == true && toggleState.visible
    val isOff = toggleState?.enabled == true && !toggleState.visible
    val rowBackground = when {
        !enabled -> uiTheme.controls.panelBg
        isOn -> lerp(uiTheme.controls.buttonBg, Color.White, 0.16f)
        isOff -> lerp(uiTheme.controls.buttonBg, Color.Black, 0.12f)
        active -> lerp(uiTheme.controls.buttonBg, Color.White, 0.18f)
        else -> uiTheme.controls.buttonBg
    }
    val rowTextColor = when {
        !enabled -> uiTheme.controls.panelMuted.copy(alpha = 0.7f)
        else -> uiTheme.controls.buttonFg
    }
    Box(
        modifier = Modifier
            .then(if (width != Dp.Unspecified) Modifier.width(width) else Modifier.fillMaxWidth())
            .height(ThumbSize)
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
                    text = label,
                    modifier = Modifier.weight(1f),
                    style = MaterialTheme.typography.labelLarge,
                    maxLines = 2,
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
                text = label,
                modifier = Modifier.padding(horizontal = 12.dp),
                style = MaterialTheme.typography.labelLarge,
                maxLines = 2,
                overflow = TextOverflow.Ellipsis,
                color = rowTextColor,
            )
        }
    }
}

@Composable
private fun NavElementDock(
    navElement: NavElementUiView?,
    modifier: Modifier = Modifier,
    onClick: (() -> Unit)? = null,
) {
    val uiTheme = LocalAerobagUiTheme.current
    val shape = RoundedCornerShape(ThumbRadius * 0.9f)
    Surface(
        modifier =
            modifier
                .width(ThumbSize * 3f)
                .height(ThumbSize * 0.67f)
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
                    text = navElement?.activeLegSummary.orEmpty(),
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                    style = MaterialTheme.typography.labelSmall,
                    fontWeight = FontWeight.Bold,
                    color = Color.White,
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

@Composable
private fun PlaybackWidget(
    uiSession: NativeUiSession,
    playbackUiState: PlaybackUiState,
    sourcePath: String,
    onSourcePathChange: (String) -> Unit,
    onSnapshotChange: (UiSessionSnapshot) -> Unit,
    modifier: Modifier = Modifier,
) {
    val context = LocalContext.current
    val devServerBaseUrl = remember(context) {
        loadAndroidDevServerBaseUrl(context.applicationContext)
    }
    val scope = rememberCoroutineScope()
    var isBusy by remember { mutableStateOf(false) }
    var scrubCursorSeconds by remember { mutableStateOf<Double?>(null) }
    var seekJob by remember { mutableStateOf<Job?>(null) }
    val durationSeconds = playbackUiState.durationSeconds.coerceAtLeast(0.0)
    val committedCursorSeconds = playbackUiState.cursorSeconds.coerceIn(0.0, durationSeconds.takeIf { it > 0.0 } ?: 0.0)
    val cursorSeconds = (scrubCursorSeconds ?: committedCursorSeconds).coerceIn(0.0, durationSeconds.takeIf { it > 0.0 } ?: 0.0)
    val summary = playbackUiState.titleLabel
    val panelShape = RoundedCornerShape(ThumbRadius * 0.9f)
    Surface(
        modifier =
            modifier
                .widthIn(min = ThumbSize * 2.8f, max = ThumbSize * 5.2f),
        shape = panelShape,
        color = Color(0xF0FCF8F1),
        contentColor = Color(0xFF132129),
        border = BorderStroke(1.dp, Color(0x334E626C)),
        shadowElevation = 6.dp,
    ) {
        Box {
            Box(
                modifier =
                    Modifier
                        .matchParentSize()
                        .consumePointerGestures(),
            )
            Column(
                modifier = Modifier.padding(ThumbSize * 0.12f),
                verticalArrangement = Arrangement.spacedBy(ThumbSize * 0.08f),
            ) {
                Row(
                    modifier = Modifier.fillMaxWidth(),
                    horizontalArrangement = Arrangement.spacedBy(ThumbSize * 0.08f),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Text(
                        text = summary,
                        modifier = Modifier.weight(1f),
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                        style = MaterialTheme.typography.labelSmall,
                        fontWeight = FontWeight.Bold,
                        color = Color(0xFF132129),
                    )
                    Text(
                        text = "${String.format("%.1f", playbackUiState.rate)}x",
                        style = MaterialTheme.typography.labelSmall,
                        fontWeight = FontWeight.Bold,
                        color = Color(0xFF52656D),
                    )
                }
                Row(
                    modifier = Modifier.fillMaxWidth(),
                    horizontalArrangement = Arrangement.spacedBy(ThumbSize * 0.08f),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    BasicTextField(
                        value = sourcePath,
                        onValueChange = onSourcePathChange,
                        singleLine = true,
                        textStyle =
                            MaterialTheme.typography.labelSmall.copy(
                                color = Color(0xFF132129),
                                fontSize = 11.sp,
                            ),
                        modifier =
                            Modifier
                                .weight(1f)
                                .height(ThumbSize * 0.42f)
                                .clip(RoundedCornerShape(ThumbRadius * 0.55f))
                                .background(Color.White)
                                .border(1.dp, Color(0x24132129), RoundedCornerShape(ThumbRadius * 0.55f))
                                .padding(horizontal = ThumbSize * 0.1f, vertical = ThumbSize * 0.11f),
                    )
                    PlaybackSmallButton(
                        label = "LOAD",
                        enabled = !isBusy && sourcePath.isNotBlank(),
                        onClick = {
                            scope.launch {
                                isBusy = true
                                try {
                                    val traceJson =
                                        withContext(Dispatchers.IO) {
                                            URL(resolvePlaybackTraceUrl(sourcePath, devServerBaseUrl)).readText()
                                        }
                                    onSnapshotChange(uiSession.loadPlaybackTrace(sourcePath, traceJson))
                                } catch (error: Throwable) {
                                    Log.e("AerobagPlayback", "trace load failed: $sourcePath", error)
                                } finally {
                                    isBusy = false
                                }
                            }
                        },
                    )
                }
                Row(
                    modifier = Modifier.fillMaxWidth(),
                    horizontalArrangement = Arrangement.spacedBy(ThumbSize * 0.08f),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    PlaybackSmallButton(
                        label = "",
                        icon = if (playbackUiState.status == PlaybackStatus.Playing) PlaybackButtonIcon.Pause else PlaybackButtonIcon.Play,
                        enabled = playbackUiState.status != PlaybackStatus.Empty,
                        onClick = {
                            scope.launch {
                                runCatching {
                                    if (playbackUiState.status == PlaybackStatus.Playing) {
                                        uiSession.pausePlayback(System.currentTimeMillis().toDouble())
                                    } else {
                                        uiSession.playPlayback(System.currentTimeMillis().toDouble())
                                    }
                                }.onSuccess(onSnapshotChange)
                                    .onFailure { Log.e("AerobagPlayback", "play/pause failed", it) }
                            }
                        },
                    )
                    Text(
                        text = "SPD",
                        style = MaterialTheme.typography.labelSmall,
                        fontWeight = FontWeight.Bold,
                        color = Color(0xFF52656D),
                    )
                    PlaybackRateRail(
                        value = playbackUiState.rate.toFloat().coerceIn(0.25f, 11f),
                        enabled = playbackUiState.status != PlaybackStatus.Empty,
                        modifier = Modifier.weight(1f).height(ThumbSize * 0.42f),
                        onValueChange = { nextRate ->
                            scope.launch {
                                runCatching {
                                    uiSession.setPlaybackRate(nextRate.toDouble(), System.currentTimeMillis().toDouble())
                                }.onSuccess(onSnapshotChange)
                                    .onFailure { Log.e("AerobagPlayback", "rate change failed", it) }
                            }
                        },
                    )
                }
                PlaybackOverview(
                    playbackUiState = playbackUiState,
                    cursorSeconds = cursorSeconds,
                    durationSeconds = durationSeconds,
                    onScrub = { nextCursorSeconds, finished ->
                        scrubCursorSeconds = nextCursorSeconds
                        seekJob?.cancel()
                        seekJob = scope.launch {
                            runCatching {
                                uiSession.seekPlayback(nextCursorSeconds, System.currentTimeMillis().toDouble())
                            }.onSuccess {
                                onSnapshotChange(it)
                                if (finished) {
                                    scrubCursorSeconds = null
                                }
                            }.onFailure {
                                if (it !is CancellationException) {
                                    Log.e("AerobagPlayback", "seek failed", it)
                                }
                            }
                        }
                    },
                )
                Row(
                    modifier = Modifier.fillMaxWidth(),
                    horizontalArrangement = Arrangement.SpaceBetween,
                ) {
                    Text(playbackUiState.cursorLabel, style = MaterialTheme.typography.labelSmall, fontWeight = FontWeight.Bold, color = Color(0xFF52656D))
                    Text(playbackUiState.durationLabel, style = MaterialTheme.typography.labelSmall, fontWeight = FontWeight.Bold, color = Color(0xFF52656D))
                }
            }
        }
    }
}

@Composable
private fun PlaybackSmallButton(
    label: String,
    enabled: Boolean,
    onClick: () -> Unit,
    icon: PlaybackButtonIcon? = null,
) {
    Surface(
        modifier =
            Modifier
                .height(ThumbSize * 0.42f)
                .then(if (icon == null) Modifier.widthIn(min = ThumbSize * 0.86f) else Modifier.width(ThumbSize * 0.42f))
                .then(
                    if (enabled) {
                        Modifier.clickable(
                            indication = null,
                            interactionSource = remember { MutableInteractionSource() },
                        ) { onClick() }
                    } else {
                        Modifier.alpha(0.45f)
                    },
                ),
        shape = RoundedCornerShape(ThumbRadius * 0.55f),
        color = Color(0xFF132129),
        contentColor = Color.White,
        border = BorderStroke(1.dp, Color(0x24132129)),
    ) {
        Box(
            contentAlignment = Alignment.Center,
            modifier = if (icon == null) Modifier.padding(horizontal = ThumbSize * 0.14f) else Modifier.fillMaxSize(),
        ) {
            if (icon == null) {
                Text(
                    text = label,
                    style = MaterialTheme.typography.labelSmall,
                    fontWeight = FontWeight.ExtraBold,
                    color = Color.White,
                )
            } else {
                PlaybackButtonIconCanvas(icon = icon)
            }
        }
    }
}

private enum class PlaybackButtonIcon {
    Play,
    Pause,
}

@Composable
private fun PlaybackButtonIconCanvas(icon: PlaybackButtonIcon) {
    Canvas(modifier = Modifier.fillMaxSize()) {
        when (icon) {
            PlaybackButtonIcon.Play -> {
                val iconWidth = size.width * 0.34f
                val iconHeight = size.height * 0.44f
                val left = size.width * 0.5f - iconWidth * 0.38f
                val top = size.height * 0.5f - iconHeight * 0.5f
                val path =
                    Path().apply {
                        moveTo(left, top)
                        lineTo(left, top + iconHeight)
                        lineTo(left + iconWidth, top + iconHeight * 0.5f)
                        close()
                    }
                drawPath(path = path, color = Color.White)
            }
            PlaybackButtonIcon.Pause -> {
                val barWidth = size.width * 0.12f
                val barHeight = size.height * 0.44f
                val gap = size.width * 0.08f
                val left = size.width * 0.5f - gap * 0.5f - barWidth
                val top = size.height * 0.5f - barHeight * 0.5f
                drawRoundRect(
                    color = Color.White,
                    topLeft = Offset(left, top),
                    size = Size(barWidth, barHeight),
                    cornerRadius = androidx.compose.ui.geometry.CornerRadius(barWidth * 0.28f, barWidth * 0.28f),
                )
                drawRoundRect(
                    color = Color.White,
                    topLeft = Offset(left + barWidth + gap, top),
                    size = Size(barWidth, barHeight),
                    cornerRadius = androidx.compose.ui.geometry.CornerRadius(barWidth * 0.28f, barWidth * 0.28f),
                )
            }
        }
    }
}

private fun Modifier.consumePointerGestures(): Modifier =
    pointerInput(Unit) {
        awaitEachGesture {
            while (true) {
                val event = awaitPointerEvent()
                event.changes.forEach { it.consume() }
                if (event.changes.none { it.pressed }) {
                    break
                }
            }
        }
    }

@Composable
private fun PlaybackRateRail(
    value: Float,
    enabled: Boolean,
    modifier: Modifier = Modifier,
    onValueChange: (Float) -> Unit,
) {
    val shape = RoundedCornerShape(ThumbRadius * 0.55f)
    var railSize by remember { mutableStateOf(IntSize.Zero) }
    fun rateForX(x: Float): Float {
        val width = railSize.width.toFloat().coerceAtLeast(1f)
        val ratio = x.coerceIn(0f, width) / width
        val rawRate = 0.25f + ratio * (11f - 0.25f)
        return (kotlin.math.round(rawRate / 0.25f) * 0.25f).coerceIn(0.25f, 11f)
    }
    Surface(
        modifier =
            modifier
                .clip(shape)
                .background(Color.White)
                .border(1.dp, Color(0x24132129), shape)
                .alpha(if (enabled) 1f else 0.45f)
                .onSizeChanged { railSize = it }
                .pointerInput(enabled, railSize) {
                    awaitEachGesture {
                        var activePointer: PointerId? = null
                        while (true) {
                            val event = awaitPointerEvent()
                            val change =
                                if (activePointer == null) {
                                    event.changes.firstOrNull { it.pressed }?.also { activePointer = it.id }
                                } else {
                                    event.changes.firstOrNull { it.id == activePointer }
                                } ?: break
                            if (enabled && railSize.width > 0) {
                                onValueChange(rateForX(change.position.x))
                            }
                            change.consume()
                            if (!change.pressed) {
                                break
                            }
                        }
                    }
                },
        shape = shape,
        color = Color.White,
        contentColor = Color(0xFF132129),
    ) {
        Canvas(modifier = Modifier.fillMaxSize().padding(horizontal = ThumbSize * 0.09f, vertical = ThumbSize * 0.12f)) {
            val centerY = size.height * 0.5f
            val trackHeight = 3.dp.toPx()
            val knobRadius = 6.dp.toPx()
            val progress = ((value - 0.25f) / (11f - 0.25f)).coerceIn(0f, 1f)
            val knobX = knobRadius + progress * (size.width - knobRadius * 2f).coerceAtLeast(1f)
            drawLine(
                color = Color(0x26132129),
                start = Offset(knobRadius, centerY),
                end = Offset(size.width - knobRadius, centerY),
                strokeWidth = trackHeight,
                cap = StrokeCap.Round,
            )
            drawLine(
                color = Color(0xFF132129),
                start = Offset(knobRadius, centerY),
                end = Offset(knobX, centerY),
                strokeWidth = trackHeight,
                cap = StrokeCap.Round,
            )
            drawCircle(
                color = Color(0xFF132129),
                radius = knobRadius,
                center = Offset(knobX, centerY),
            )
            drawCircle(
                color = Color.White,
                radius = knobRadius * 0.45f,
                center = Offset(knobX, centerY),
            )
        }
    }
}

@Composable
private fun PlaybackOverview(
    playbackUiState: PlaybackUiState,
    cursorSeconds: Double,
    durationSeconds: Double,
    onScrub: (Double, Boolean) -> Unit,
) {
    val shape = RoundedCornerShape(ThumbRadius * 0.45f)
    var overviewSize by remember { mutableStateOf(IntSize.Zero) }
    Box(
        modifier =
            Modifier
                .fillMaxWidth()
                .height(ThumbSize * 0.56f)
                .clip(shape)
                .background(Color(0xD1FFFFFF))
                .border(1.dp, Color(0x1F132129), shape)
                .onSizeChanged { overviewSize = it }
                .pointerInput(durationSeconds, overviewSize) {
                    awaitEachGesture {
                        var activePointer: PointerId? = null
                        val width = overviewSize.width.toFloat().coerceAtLeast(1f)
                        while (true) {
                            val event = awaitPointerEvent()
                            val change =
                                if (activePointer == null) {
                                    event.changes.firstOrNull { it.pressed }?.also { activePointer = it.id }
                                } else {
                                    event.changes.firstOrNull { it.id == activePointer }
                                } ?: break
                            val finished = !change.pressed
                            if (durationSeconds > 0.0 && overviewSize.width > 0) {
                                val nextCursorSeconds = (change.position.x.coerceIn(0f, width) / width.toDouble()) * durationSeconds
                                onScrub(nextCursorSeconds, finished)
                            }
                            change.consume()
                            if (finished) {
                                break
                            }
                        }
                    }
                },
    ) {
        Canvas(modifier = Modifier.fillMaxSize()) {
            val knobRadius = 7.dp.toPx()
            val usableWidth = (size.width - knobRadius * 2f).coerceAtLeast(1f)
            fun drawProfile(samples: List<Double?>, color: Color) {
                val usable = samples.mapIndexedNotNull { index, value -> value?.let { index to it } }
                if (usable.isEmpty()) {
                    return
                }
                val lastIndex = (samples.size - 1).coerceAtLeast(1)
                val path = Path()
                usable.forEachIndexed { pointIndex, (sampleIndex, value) ->
                    val x = knobRadius + (sampleIndex.toFloat() / lastIndex.toFloat()) * usableWidth
                    val y = size.height - (value.toFloat().coerceIn(0f, 1f) * size.height)
                    if (pointIndex == 0) {
                        path.moveTo(x, y)
                    } else {
                        path.lineTo(x, y)
                    }
                }
                drawPath(
                    path = path,
                    color = color,
                    style = Stroke(width = 1.6.dp.toPx(), cap = StrokeCap.Round),
                )
            }
            drawProfile(playbackUiState.altitudeProfileNorm, Color(0xCC0D6F67))
            drawProfile(playbackUiState.speedProfileNorm, Color(0xEBD45A7A))
            if (durationSeconds > 0.0) {
                playbackUiState.gapSpans.forEach { gap ->
                    val startRatio = (gap.startSeconds / durationSeconds).coerceIn(0.0, 1.0).toFloat()
                    val endRatio = (gap.endSeconds / durationSeconds).coerceIn(0.0, 1.0).toFloat()
                    val startX = knobRadius + startRatio * usableWidth
                    val endX = knobRadius + endRatio * usableWidth
                    if (endX > startX) {
                        drawRect(
                            color = Color(0x18132129),
                            topLeft = Offset(startX, 0f),
                            size = Size(endX - startX, size.height),
                        )
                        var hatchX = startX - size.height
                        val hatchSpacing = 6.dp.toPx()
                        clipRect(left = startX, top = 0f, right = endX, bottom = size.height) {
                            while (hatchX < endX) {
                                drawLine(
                                    color = Color(0x66132129),
                                    start = Offset(hatchX, size.height),
                                    end = Offset(hatchX + size.height, 0f),
                                    strokeWidth = 1.dp.toPx(),
                                )
                                hatchX += hatchSpacing
                            }
                        }
                    }
                }
            }
            val ratio = if (durationSeconds > 0.0) (cursorSeconds / durationSeconds).coerceIn(0.0, 1.0).toFloat() else 0f
            val cursorX = knobRadius + ratio * usableWidth
            drawLine(
                color = Color(0x85132129),
                start = Offset(cursorX, 0f),
                end = Offset(cursorX, size.height),
                strokeWidth = 1.dp.toPx(),
            )
            drawCircle(
                color = Color(0xFF132129),
                radius = knobRadius,
                center = Offset(cursorX, size.height - 1.dp.toPx()),
            )
            drawCircle(
                color = Color(0xF0FCF8F1),
                radius = knobRadius,
                center = Offset(cursorX, size.height - 1.dp.toPx()),
                style = Stroke(width = 1.5.dp.toPx()),
            )
        }
    }
}

private fun loadAndroidDevServerBaseUrl(context: Context): String =
    runCatching {
        context.assets.open("fixtures/android-dev-server-base-url.txt")
            .bufferedReader()
            .use { it.readText().trim() }
            .takeIf { it.isNotBlank() }
    }.getOrNull() ?: DefaultAndroidDevServerBaseUrl

private fun loadAndroidPackageSourceBaseUrl(context: Context): String =
    runCatching {
        context.assets.open("fixtures/android-package-source-base-url.txt")
            .bufferedReader()
            .use { it.readText().trim() }
            .trimEnd('/')
            .takeIf { it.isNotBlank() }
    }.getOrNull() ?: DefaultAndroidPackageSourceBaseUrl

private fun readOfflinePackagesStateJson(
    prefs: android.content.SharedPreferences,
): String =
    prefs.getString(UiPrefsOfflinePackagePreferencesKey, null)
        ?.takeIf { it.isNotBlank() }
        ?: ""

private fun resolvePlaybackTraceUrl(sourcePath: String, devServerBaseUrl: String): String =
    when {
        sourcePath.startsWith("http://") || sourcePath.startsWith("https://") -> sourcePath
        sourcePath.startsWith("/") -> "$devServerBaseUrl$sourcePath"
        else -> "$devServerBaseUrl/$sourcePath"
    }

private fun fetchJsonOrNull(url: String): String? =
    runCatching {
        val connection = URL(url).openConnection() as HttpURLConnection
        connection.connectTimeout = 1500
        connection.readTimeout = 2500
        connection.inputStream.bufferedReader().use { it.readText() }
    }.getOrNull()

private fun fetchJsonOrEmpty(url: String, emptyJson: String): String =
    fetchJsonOrNull(url) ?: emptyJson

@Composable
private fun PlanHeaderRow() {
    Row(horizontalArrangement = Arrangement.spacedBy(PlanGridGap)) {
        PlanCell("Waypoint", Modifier.width(ThumbSize * 2.5f), isHeader = true)
        PlanCell("Dist (nm)", Modifier.weight(1f), isHeader = true)
        PlanCell("ETE (h:m)", Modifier.weight(1f), isHeader = true)
        PlanCell("Course (°)", Modifier.weight(1f), isHeader = true)
    }
}

private fun concretizedNavItemLabel(item: ConcretizedNavItem): String = when (item) {
    is ConcretizedNavItem.Waypoint -> navRefLabel(item.navRef)
    is ConcretizedNavItem.Discontinuity -> item.label
}

private fun structuredComponentLabel(component: RouteComponentUiView): String =
    if (component.kind == RouteComponentViewKind.Airway) {
        component.summary.substringBefore("(").trim()
    } else {
        component.summary
    }

private fun componentWaypointNavRef(component: RouteComponentUiView?): NavRef? {
    val item = component?.items?.firstOrNull()
    return if (item is ConcretizedNavItem.Waypoint) item.navRef else null
}

private fun navRefsEqual(left: NavRef?, right: NavRef?): Boolean = when {
    left == null || right == null -> false
    left is NavRef.Airport && right is NavRef.Airport -> left.code == right.code
    left is NavRef.Navaid && right is NavRef.Navaid -> left.code == right.code
    left is NavRef.Fix && right is NavRef.Fix -> left.code == right.code
    left is NavRef.LatLon && right is NavRef.LatLon -> left.lat == right.lat && left.lon == right.lon
    else -> false
}

private fun navRefSelectionKey(navRef: NavRef?): String = when (navRef) {
    is NavRef.Airport -> "airport:${navRef.code}"
    is NavRef.Navaid -> "navaid:${navRef.code}"
    is NavRef.Fix -> "fix:${navRef.code}"
    is NavRef.LatLon -> "latlon:${navRef.lat},${navRef.lon}"
    null -> "none"
}

private fun buildFlightPlanDisplayRows(planUiState: FlightPlanUiState): List<FlightPlanDisplayRow> =
    planUiState.displayRows.mapIndexed { index, row ->
        FlightPlanDisplayRow(
            id = when (row.rowKind) {
                FlightPlanDisplayRowKind.Waypoint -> if (row.depth == 0) "component:${row.componentIndex ?: index}" else "item:${row.componentIndex ?: "x"}:${row.label}:$index"
                FlightPlanDisplayRowKind.Group -> "group:${row.componentIndex ?: index}"
                FlightPlanDisplayRowKind.Discontinuity -> "disc:${row.componentIndex ?: "x"}:$index"
            },
            selectionKey = selectionKeyForDisplayRow(row, index),
            label = row.label,
            rowKind =
                when (row.rowKind) {
                    FlightPlanDisplayRowKind.Waypoint -> "waypoint"
                    FlightPlanDisplayRowKind.Group -> "group"
                    FlightPlanDisplayRowKind.Discontinuity -> "discontinuity"
                },
            componentKind = row.componentKind,
            componentIndex = row.componentIndex,
            legIndex = row.legIndex,
            distanceNm = row.distanceNm,
            courseDeg = row.courseDeg,
            chartAirportId = row.chartAirportId,
            navRef = row.navRef,
            symbolFeature = row.symbolFeature,
            depth = row.depth,
            active = row.active,
            canAddAirwayAfter = row.canAddAirwayAfter,
            canAddProcedureBefore = row.canAddProcedureBefore,
            canChangeAirway = row.canChangeAirway,
            canRemoveComponent = row.canRemoveComponent,
            canReorderComponent = row.canReorderComponent,
            canReorderUp = row.canReorderUp,
            canReorderDown = row.canReorderDown,
            actions = row.actions,
            startComponentIndex = row.startComponentIndex,
            endComponentIndex = row.endComponentIndex,
            originAnchor = row.originAnchor,
            destinationAnchor = row.destinationAnchor,
        )
    }

private fun selectionKeyForDisplayRow(row: FlightPlanDisplayRowUiView, index: Int): String =
    when (row.rowKind) {
        FlightPlanDisplayRowKind.Waypoint ->
            if (row.depth == 0) {
                "waypoint:${navRefSelectionKey(row.navRef)}"
            } else {
                "child:${row.componentKind?.name ?: "row"}:${navRefSelectionKey(row.navRef)}:$index"
            }
        FlightPlanDisplayRowKind.Group ->
            "group:${row.componentKind?.name ?: "group"}:${row.label}:${navRefSelectionKey(row.originAnchor)}:${navRefSelectionKey(row.destinationAnchor)}"
        FlightPlanDisplayRowKind.Discontinuity ->
            "disc:${row.componentKind?.name ?: "row"}:$index"
    }

private fun buildFlightPlanDisplayBlocks(rows: List<FlightPlanDisplayRow>): List<FlightPlanDisplayBlock> {
    val blocks = mutableListOf<FlightPlanDisplayBlock>()
    var index = 0
    while (index < rows.size) {
        val row = rows[index]
        if (row.rowKind == "group") {
            val children = mutableListOf<Pair<Int, FlightPlanDisplayRow>>()
            var childIndex = index + 1
            while (childIndex < rows.size && rows[childIndex].depth > 0) {
                children += childIndex to rows[childIndex]
                childIndex += 1
            }
            blocks += FlightPlanDisplayBlock.Group(
                headerIndex = index,
                header = row,
                children = children,
            )
            index = childIndex
        } else {
            blocks += FlightPlanDisplayBlock.Single(index = index, row = row)
            index += 1
        }
    }
    return blocks
}

private fun navRefLabel(ref: NavRef): String = when (ref) {
    is NavRef.Airport -> ref.code
    is NavRef.Navaid -> ref.code
    is NavRef.Fix -> ref.code
    is NavRef.LatLon -> "${"%.3f".format(ref.lat)},${"%.3f".format(ref.lon)}"
}

private fun airportFuelMarkerPath(center: Offset, scale: Float): Path {
    val circleRadius = 12f * scale
    val tabHalf = 4f * scale
    val tabOuter = 17f * scale
    val arcJoin = 11.314f * scale
    val circleBounds = Rect(
        left = center.x - circleRadius,
        top = center.y - circleRadius,
        right = center.x + circleRadius,
        bottom = center.y + circleRadius,
    )
    return Path().apply {
        moveTo(center.x - tabHalf, center.y - tabOuter)
        lineTo(center.x + tabHalf, center.y - tabOuter)
        lineTo(center.x + tabHalf, center.y - arcJoin)
        arcTo(circleBounds, -70.5288f, 51.0576f, false)
        lineTo(center.x + tabOuter, center.y - tabHalf)
        lineTo(center.x + tabOuter, center.y + tabHalf)
        lineTo(center.x + arcJoin, center.y + tabHalf)
        arcTo(circleBounds, 19.4712f, 51.0576f, false)
        lineTo(center.x + tabHalf, center.y + tabOuter)
        lineTo(center.x - tabHalf, center.y + tabOuter)
        lineTo(center.x - tabHalf, center.y + arcJoin)
        arcTo(circleBounds, 109.4712f, 51.0576f, false)
        lineTo(center.x - tabOuter, center.y + tabHalf)
        lineTo(center.x - tabOuter, center.y - tabHalf)
        lineTo(center.x - arcJoin, center.y - tabHalf)
        arcTo(circleBounds, 199.4712f, 51.0576f, false)
        close()
    }
}

private fun aviationColor(colorKey: String): Color = when (colorKey) {
    "class_c_magenta", "magenta" -> Color(0xFFFF4FD8)
    "class_b_d_blue", "blue" -> Color(0xFF2D8CFF)
    "tfr_red", "red" -> Color(0xFFE03131)
    "dark_gray" -> Color(0xFF26333A)
    else -> Color(0xFF2D8CFF)
}

private fun airspacePath(subpath: net.jonh.aerobag.prototype.domain.AirspaceDisplaySubpath): Path =
    Path().apply {
        val first = subpath.points.firstOrNull() ?: return@apply
        moveTo(first.x.toFloat(), first.y.toFloat())
        subpath.points.drop(1).forEach { point -> lineTo(point.x.toFloat(), point.y.toFloat()) }
        if (subpath.closed) {
            close()
        }
    }

private fun strokeCapFor(lineCap: String): StrokeCap = when (lineCap) {
    "butt" -> StrokeCap.Butt
    "square" -> StrokeCap.Square
    else -> StrokeCap.Round
}

private fun androidx.compose.ui.graphics.drawscope.DrawScope.drawAirspaceDisplayPath(feature: AirspaceDisplayPath) {
    feature.paths.forEach { subpath ->
        val path = airspacePath(subpath)
        if (subpath.closed && feature.style.fillOpacity > 0.0) {
            drawPath(
                path = path,
                color = aviationColor(feature.style.fillColorKey).copy(alpha = feature.style.fillOpacity.toFloat()),
            )
        }
        feature.style.strokes.forEach { stroke ->
            drawPath(
                path = path,
                color = aviationColor(stroke.colorKey),
                style = Stroke(
                    width = stroke.widthPx.toFloat(),
                    cap = strokeCapFor(stroke.lineCap),
                    pathEffect = stroke.dashPx.takeIf { it.isNotEmpty() }?.let { dash ->
                        PathEffect.dashPathEffect(dash.map { it.toFloat() }.toFloatArray())
                    },
                ),
            )
        }
    }
    feature.decorations.forEach { decoration ->
        decoration.paths.forEach { subpath ->
            drawPath(
                path = airspacePath(subpath),
                color = aviationColor(decoration.colorKey),
                style = Stroke(width = decoration.widthPx.toFloat(), cap = strokeCapFor(decoration.lineCap)),
            )
        }
    }
}

private fun androidx.compose.ui.graphics.drawscope.DrawScope.drawAirspaceLimitGlyph(
    glyph: AirspaceLimitGlyph,
    center: Offset,
    scale: Float,
) {
    val color = aviationColor(glyph.colorKey)
    val paint = Paint().apply {
        isAntiAlias = true
        style = Paint.Style.FILL
        this.color = color.toArgb()
        textAlign = Paint.Align.CENTER
        textSize = 14f * scale * density
        typeface = Typeface.create(Typeface.DEFAULT_BOLD, Typeface.BOLD)
    }
    val strokePaint = Paint(paint).apply {
        style = Paint.Style.STROKE
        strokeWidth = 3.5f * scale * density
        this.color = android.graphics.Color.argb(235, 255, 255, 255)
    }
    val dividerContrastPaint = Paint().apply {
        isAntiAlias = true
        style = Paint.Style.STROKE
        strokeWidth = 4f * scale * density
        strokeCap = Paint.Cap.SQUARE
        this.color = android.graphics.Color.argb(235, 255, 255, 255)
    }
    val dividerPaint = Paint(dividerContrastPaint).apply {
        strokeWidth = 1.6f * scale * density
        this.color = color.toArgb()
    }
    drawContext.canvas.nativeCanvas.apply {
        val fontCenterToBaseline = -(paint.fontMetrics.ascent + paint.fontMetrics.descent) / 2f
        val textHeight = paint.fontMetrics.descent - paint.fontMetrics.ascent
        val dividerGap = 2.5f * scale * density
        val upperCenterY = center.y - dividerGap - textHeight / 2f
        val lowerCenterY = center.y + dividerGap + textHeight / 2f
        val upperY = upperCenterY + fontCenterToBaseline
        val lowerY = lowerCenterY + fontCenterToBaseline
        val dividerWidth = max(
            paint.measureText(glyph.upper),
            paint.measureText(glyph.lower),
        ) + 8f * scale * density
        drawText(glyph.upper, center.x, upperY, strokePaint)
        drawText(glyph.upper, center.x, upperY, paint)
        drawLine(center.x - dividerWidth / 2f, center.y, center.x + dividerWidth / 2f, center.y, dividerContrastPaint)
        drawLine(center.x - dividerWidth / 2f, center.y, center.x + dividerWidth / 2f, center.y, dividerPaint)
        drawText(glyph.lower, center.x, lowerY, strokePaint)
        drawText(glyph.lower, center.x, lowerY, paint)
    }
}

private fun metarColor(category: String): Color = when (category.lowercase()) {
    "vfr" -> Color(0xFF26C85A)
    "mvfr" -> Color(0xFF2D8CFF)
    "ifr" -> Color(0xFFE03131)
    "lifr" -> Color(0xFFFF4FD8)
    else -> Color(0xFF9AA6AE)
}

private fun androidx.compose.ui.graphics.drawscope.DrawScope.drawMetarSymbol(
    feature: VisibleMetarFeature,
    center: Offset,
    densityScale: Float,
) {
    val radius = 8f * densityScale
    val fillColor = metarColor(feature.flightCategory)
    when (feature.ceilingAmount.lowercase()) {
        "ovc" -> drawCircle(fillColor, radius = radius, center = center)
        "few" -> drawLine(fillColor, Offset(center.x, center.y - radius), Offset(center.x, center.y + radius), strokeWidth = 3f * densityScale)
        "sct" -> drawArc(fillColor, -90f, 90f, useCenter = true, topLeft = Offset(center.x - radius, center.y - radius), size = Size(radius * 2f, radius * 2f))
        "bkn" -> drawArc(fillColor, -90f, 270f, useCenter = true, topLeft = Offset(center.x - radius, center.y - radius), size = Size(radius * 2f, radius * 2f))
    }
    drawCircle(Color(0xE6081218), radius = radius + 2f * densityScale, center = center, style = Stroke(width = 3f * densityScale))
    drawCircle(fillColor, radius = radius, center = center, style = Stroke(width = 2.2f * densityScale))
}

@Composable
private fun PlanWaypointSymbol(
    feature: net.jonh.aerobag.prototype.domain.NavSymbolFeature?,
    modifier: Modifier = Modifier,
) {
    if (feature == null) {
        return
    }
    Canvas(modifier = modifier.size(ThumbSize * 0.78f)) {
        val scale = size.minDimension / 40f
        val center = Offset(size.width / 2f, size.height / 2f)
        val fixMarkerStrokeColor = Color(0xB3081218)
        val fixMarkerFillColor = Color(0xFF39D9FF)
        val airportMarkerStrokeColor = Color(0xB3081218)
        val airportFillColor = Color(0xFFFF4FD8)
        val vorMarkerColor = Color(0xFF4AA3FF)
        val isAirport = feature.styleClass == "airport" || feature.kind.equals("airport", ignoreCase = true)
        val isVor = feature.styleClass == "nav" || feature.kind.contains("vor", ignoreCase = true)
        when {
            isAirport -> {
                val airportRadius = 12f * scale
                if (feature.fuelAvailable) {
                    val markerPath = airportFuelMarkerPath(center, scale)
                    drawPath(markerPath, airportFillColor)
                    drawPath(markerPath, airportMarkerStrokeColor, style = Stroke(width = 2f * scale))
                } else {
                    drawCircle(airportFillColor, radius = airportRadius, center = center)
                    drawCircle(airportMarkerStrokeColor, radius = airportRadius, center = center, style = Stroke(width = 2f * scale))
                }
                feature.longestRunwayHeadingTrueDeg?.let { heading ->
                    val runwayHalfLength = 8f * feature.runwayLengthRatio.coerceAtLeast(0.2).toFloat() * scale
                    rotate(heading.toFloat(), center) {
                        drawLine(
                            color = airportMarkerStrokeColor,
                            start = Offset(center.x, center.y - runwayHalfLength),
                            end = Offset(center.x, center.y + runwayHalfLength),
                            strokeWidth = 5f * scale,
                            cap = StrokeCap.Round,
                        )
                        drawLine(
                            color = Color.White,
                            start = Offset(center.x, center.y - runwayHalfLength),
                            end = Offset(center.x, center.y + runwayHalfLength),
                            strokeWidth = 3f * scale,
                            cap = StrokeCap.Round,
                        )
                    }
                }
            }

            isVor -> {
                val radius = 8f * scale
                val outerHex = polygonPath(vorHexPoints(center, radius))
                val band = vorBandPath(center, radius)
                drawPath(band, vorMarkerColor)
                drawPath(band, fixMarkerStrokeColor, style = Stroke(width = 1.6f * scale))
                drawPath(outerHex, fixMarkerStrokeColor, style = Stroke(width = 1.6f * scale))
            }

            else -> {
                val triangle = fixTrianglePath(center, 8f * scale)
                drawPath(triangle, fixMarkerFillColor)
                drawPath(triangle, fixMarkerStrokeColor, style = Stroke(width = 2.5f * scale))
            }
        }
    }
}

@Composable
private fun FlightPlanDataRow(
    row: FlightPlanDisplayRow,
    selected: Boolean,
    reorderOpen: Boolean = false,
    modifier: Modifier = Modifier,
    structuredRowBounds: MutableMap<String, Rect>? = null,
    onWaypointClick: () -> Unit,
) {
    val uiTheme = LocalAerobagUiTheme.current
    val childRow = row.depth > 0
    val targetIndent = ThumbSize * (row.depth * 0.5f)
    val indent by animateDpAsState(targetValue = targetIndent, label = "planRowIndent")
    val rowOpacity by animateFloatAsState(
        targetValue = if (reorderOpen && childRow) 0.72f else 1f,
        label = "planRowOpacity",
    )
    val labelScaleY by animateFloatAsState(
        targetValue = if (reorderOpen && childRow) 0.72f else 1f,
        label = "planRowLabelScaleY",
    )
    val cellHeight by animateDpAsState(
        targetValue = if (reorderOpen && childRow) ThumbSize * 0.34f else ThumbSize,
        label = "planRowCellHeight",
    )
    val rowBoundsModifier =
        if (structuredRowBounds != null) {
            rememberStructuredRowBounds(row.id, structuredRowBounds)
        } else {
            Modifier
        }
    val defaultButtonColor =
        when {
            row.rowKind == "group" -> uiTheme.controls.headerButton
            else -> uiTheme.controls.buttonBg
        }
    val selectedButtonColor =
        when {
            row.active -> Color(0xFF9B3A88)
            else -> Color(
                red = uiTheme.controls.buttonBg.red * 0.74f,
                green = uiTheme.controls.buttonBg.green * 0.74f,
                blue = uiTheme.controls.buttonBg.blue * 0.74f,
                alpha = uiTheme.controls.buttonBg.alpha,
            )
        }
    Row(modifier = modifier.then(rowBoundsModifier), horizontalArrangement = Arrangement.spacedBy(PlanGridGap)) {
        Box(modifier = Modifier.width(ThumbSize * 2.5f).height(cellHeight)) {
            CompactSquareButton(
                label = row.label,
                modifier =
                    Modifier
                        .height(cellHeight)
                        .width(ThumbSize * 2.5f - indent)
                        .align(Alignment.CenterEnd)
                        .alpha(rowOpacity),
                centered = false,
                textStartPadding = 10.dp,
                backgroundColor = defaultButtonColor,
                selected = selected,
                selectedColor = selectedButtonColor,
                textModifier =
                    Modifier
                        .padding(end = ThumbSize * 0.78f)
                        .graphicsLayer {
                            scaleY = labelScaleY
                            transformOrigin = TransformOrigin(0f, 0.5f)
                        },
                onClick = onWaypointClick,
            )
            PlanWaypointSymbol(
                feature = row.symbolFeature,
                modifier = Modifier
                    .align(Alignment.CenterEnd)
                    .padding(end = ThumbSize * 0.12f)
                    .alpha(rowOpacity),
            )
        }
        PlanCell(formatPlanDistance(row.distanceNm), Modifier.weight(1f), cellHeight = cellHeight, alpha = rowOpacity)
        PlanCell("—", Modifier.weight(1f), cellHeight = cellHeight, alpha = rowOpacity)
        PlanCell(formatPlanCourse(row.courseDeg), Modifier.weight(1f), cellHeight = cellHeight, alpha = rowOpacity)
    }
}

@Composable
private fun FlightPlanGroupBlock(
    header: FlightPlanDisplayRow,
    headerSelected: Boolean,
    reorderOpen: Boolean,
    structuredRowBounds: MutableMap<String, Rect>? = null,
    onHeaderClick: () -> Unit,
    children: List<Pair<Int, FlightPlanDisplayRow>>,
    selectedWaypointIndex: Int?,
    onChildClick: (Int) -> Unit,
) {
    val uiTheme = LocalAerobagUiTheme.current
    val groupOverhang = 8.dp
    val cornerRadius = ThumbRadius + 2.dp
    Column(
        verticalArrangement = Arrangement.spacedBy(PlanGridGap),
        modifier = Modifier
            .fillMaxWidth()
            .padding(vertical = PlanGridGap / 2)
            .drawBehind {
                val overhangPx = groupOverhang.toPx()
                val radiusPx = cornerRadius.toPx()
                drawRoundRect(
                    color = uiTheme.controls.panelBg.copy(alpha = 0.82f),
                    topLeft = Offset(-overhangPx, 0f),
                    size = Size(size.width + overhangPx, size.height),
                    cornerRadius = androidx.compose.ui.geometry.CornerRadius(radiusPx, radiusPx),
                )
                drawRoundRect(
                    color = uiTheme.controls.panelBorder.copy(alpha = 0.95f),
                    topLeft = Offset(-overhangPx, 0f),
                    size = Size(size.width + overhangPx, size.height),
                    cornerRadius = androidx.compose.ui.geometry.CornerRadius(radiusPx, radiusPx),
                    style = Stroke(width = 2.dp.toPx()),
                )
            }
            .padding(top = 8.dp, end = 8.dp, bottom = 8.dp),
    ) {
        FlightPlanDataRow(
            row = header,
            selected = headerSelected,
            reorderOpen = reorderOpen,
            structuredRowBounds = structuredRowBounds,
            onWaypointClick = onHeaderClick,
        )
        children.forEach { (childIndex, childRow) ->
            FlightPlanDataRow(
                row = childRow,
                selected = selectedWaypointIndex == childIndex,
                reorderOpen = reorderOpen,
                structuredRowBounds = structuredRowBounds,
                onWaypointClick = { onChildClick(childIndex) },
            )
        }
    }
}

@Composable
private fun PlanCell(value: String, modifier: Modifier, isHeader: Boolean = false, cellHeight: Dp? = null, alpha: Float = 1f) {
    val uiTheme = LocalAerobagUiTheme.current
    val resolvedCellHeight = cellHeight ?: if (isHeader) ThumbSize * 0.5f else ThumbSize
    Box(
        modifier = modifier
            .height(resolvedCellHeight)
            .alpha(alpha)
            .background(uiTheme.controls.panelBg, RoundedCornerShape(ThumbRadius))
            .border(1.dp, uiTheme.controls.panelBorder, RoundedCornerShape(ThumbRadius))
            .padding(horizontal = 10.dp),
        contentAlignment = Alignment.CenterStart,
    ) {
        Text(
            value,
            style = if (isHeader) MaterialTheme.typography.labelMedium else MaterialTheme.typography.bodyMedium,
            color = if (isHeader) uiTheme.controls.panelMuted else uiTheme.controls.panelFg,
            fontWeight = if (isHeader) FontWeight.Bold else FontWeight.Medium,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
        )
    }
}

@Composable
private fun DebugDock(
    open: Boolean,
    onToggle: () -> Unit,
    highlight: Boolean = false,
    expandAbove: Boolean = false,
    modifier: Modifier = Modifier,
    content: @Composable ColumnScope.() -> Unit,
) {
    Box(modifier = modifier.padding(ThumbGap)) {
        CompactSquareButton(
            label = "DBG",
            modifier = Modifier
                .align(if (expandAbove) Alignment.BottomEnd else Alignment.BottomStart)
                .size(ThumbSize),
            selected = highlight,
            selectedColor = Color(0xFFB85C00),
            onClick = onToggle,
        )

        AnimatedVisibility(
            visible = open,
            modifier = Modifier
                .align(if (expandAbove) Alignment.BottomEnd else Alignment.BottomStart)
                .padding(
                    bottom = ThumbSize + ThumbGap,
                ),
            enter = slideInVertically(initialOffsetY = { it / 3 }) + fadeIn(),
            exit = slideOutVertically(targetOffsetY = { it / 3 }) + fadeOut(),
        ) {
            Card(modifier = Modifier.width(ThumbSize * 4f)) {
                Column(
                    modifier = Modifier.padding(horizontal = 10.dp, vertical = 8.dp),
                    verticalArrangement = Arrangement.spacedBy(2.dp),
                    content = content,
                )
            }
        }
    }
}

private fun pageLabel(page: AppPage): String = PageOptions.firstOrNull { it.page == page }?.launcherLabel ?: page.name.uppercase()

@Composable
private fun rememberUptimeLabel(sessionStartElapsedMs: Long): String {
    val nowMs by produceState(initialValue = SystemClock.elapsedRealtime(), sessionStartElapsedMs) {
        while (true) {
            value = SystemClock.elapsedRealtime()
            delay(1000)
        }
    }
    return formatUptimeLabel(nowMs - sessionStartElapsedMs)
}

private fun formatUptimeLabel(elapsedMs: Long): String {
    val totalSeconds = (elapsedMs / 1000).coerceAtLeast(0)
    val hours = totalSeconds / 3600
    val minutes = (totalSeconds % 3600) / 60
    val seconds = totalSeconds % 60
    return if (hours > 0) {
        "%d:%02d:%02d".format(hours, minutes, seconds)
    } else {
        "%d:%02d".format(minutes, seconds)
    }
}

private fun formatSnapshot(snapshot: AppViewSnapshot): String {
    return formatSnapshot(snapshot, emptyMap())
}

private fun formatSnapshot(snapshot: AppViewSnapshot, chartLabelsById: Map<String, String>): String {
    val label = pageLabel(snapshot.page)
    if (snapshot.page == AppPage.Map) {
        val family = when (snapshot.selectedMapId.substringBefore(':')) {
            "sec" -> "SEC"
            "tac" -> "TAC"
            "enr-l" -> "IFR L"
            "enr-h" -> "IFR H"
            else -> ""
        }
        return if (family.isBlank()) label else "$label-$family"
    }
    if (snapshot.page != AppPage.Charts) {
        return label
    }
    if (snapshot.chartFolderOpen) {
        return "$label-FLDR"
    }
    val suffixSource = snapshot.selectedChartLabel
        .ifBlank { chartLabelsById[snapshot.selectedChartId].orEmpty() }
        .ifBlank { snapshot.selectedChartId }
    val suffix = suffixSource.takeLast(3).uppercase()
    return if (suffix.isBlank()) label else "$label-$suffix"
}

private fun formatPageStack(
    pageHistory: List<AppViewSnapshot>,
    currentPage: AppPage,
    selectedMapId: String = "",
    selectedAirportId: String = "",
    selectedChartId: String = "",
    selectedChartLabel: String = "",
    chartFolderOpen: Boolean = false,
    chartLabelsById: Map<String, String> = emptyMap(),
): String = (
    listOf(
        AppViewSnapshot(
        page = currentPage,
        selectedMapId = selectedMapId,
        mapViewport = MapViewportState(0.0, 0.0, 0.0),
        selectedAirportId = selectedAirportId,
        selectedChartId = selectedChartId,
        selectedChartLabel = selectedChartLabel,
        recentAirportIds = emptyList(),
        chartViewport = null,
        chartFolderOpen = chartFolderOpen,
    )
    ) + pageHistory.asReversed()
).joinToString(" > ") { formatSnapshot(it, chartLabelsById) }

@Composable
private fun ToolbarButton(label: String, modifier: Modifier = Modifier, onClick: () -> Unit) {
    CompactSquareButton(label = label, modifier = modifier.size(ThumbSize), onClick = onClick)
}

@Composable
private fun IconFrame(
    @DrawableRes iconResId: Int,
    modifier: Modifier = Modifier,
) {
    Box(
        modifier = modifier.clip(RoundedCornerShape(ThumbRadius * 0.72f)),
        contentAlignment = Alignment.Center,
    ) {
        Image(
            painter = painterResource(iconResId),
            contentDescription = null,
            contentScale = ContentScale.FillBounds,
            modifier = Modifier.fillMaxSize(),
        )
    }
}

@Composable
private fun LayerToggle(
    visible: Boolean,
    enabled: Boolean,
    modifier: Modifier = Modifier,
) {
    val uiTheme = LocalAerobagUiTheme.current
    val knobOffset by animateDpAsState(
        targetValue = if (visible) ThumbSize * 0.34f else 0.dp,
        label = "layerToggleOffset",
    )
    val trackColor by animateColorAsState(
        targetValue = if (visible) lerp(uiTheme.controls.buttonBg, Color.White, 0.84f) else lerp(uiTheme.controls.buttonBg, Color.White, 0.48f),
        label = "layerToggleTrack",
    )
    Box(
        modifier = modifier
            .width(ThumbSize * 0.78f)
            .height(ThumbSize * 0.42f)
            .clip(RoundedCornerShape(999.dp))
            .background(trackColor.copy(alpha = if (enabled) 1f else 0.45f))
            .border(2.dp, lerp(uiTheme.controls.buttonBg, Color.Black, 0.22f), RoundedCornerShape(999.dp)),
    ) {
        Box(
            modifier = Modifier
                .padding(start = 2.dp)
                .offset(x = knobOffset)
                .align(Alignment.CenterStart)
                .size(ThumbSize * 0.30f)
                .clip(CircleShape)
                .background(Color(0xFFFFFDF9)),
        )
    }
}

@Composable
private fun OutlinedButtonLabel(
    text: String,
    modifier: Modifier = Modifier,
    style: TextStyle,
    color: Color,
    maxLines: Int,
    textAlign: TextAlign = TextAlign.Center,
) {
    val offsets = listOf(
        IntOffset(-2, -2),
        IntOffset(0, -2),
        IntOffset(2, -2),
        IntOffset(-2, 0),
        IntOffset(2, 0),
        IntOffset(-2, 2),
        IntOffset(0, 2),
        IntOffset(2, 2),
        IntOffset(-1, -2),
        IntOffset(1, -2),
        IntOffset(-2, -1),
        IntOffset(2, -1),
        IntOffset(-2, 1),
        IntOffset(2, 1),
        IntOffset(-1, 2),
        IntOffset(1, 2),
    )
    Box(modifier = modifier, contentAlignment = Alignment.Center) {
        offsets.forEach { offset ->
            Text(
                text = text,
                modifier = Modifier.offset { offset },
                style = style,
                maxLines = maxLines,
                overflow = TextOverflow.Clip,
                textAlign = textAlign,
                color = Color.Black,
            )
        }
        Text(
            text = text,
            style = style,
            maxLines = maxLines,
            overflow = TextOverflow.Clip,
            textAlign = textAlign,
            color = color,
        )
    }
}

@Composable
private fun CompactSquareButton(
    label: String,
    modifier: Modifier = Modifier,
    maxLines: Int = 1,
    enabled: Boolean = true,
    selected: Boolean = false,
    backgroundColor: Color? = null,
    selectedColor: Color? = null,
    accentColor: Color? = null,
    @DrawableRes iconResId: Int? = null,
    wide: Boolean = false,
    centered: Boolean = true,
    textStartPadding: Dp = 0.dp,
    textModifier: Modifier = Modifier,
    onDisabledClick: (() -> Unit)? = null,
    onClick: () -> Unit,
) {
    val uiTheme = LocalAerobagUiTheme.current
    val iconShape = RoundedCornerShape(ThumbRadius)
    Surface(
        modifier = modifier
            .then(
                if (enabled) {
                    Modifier.pointerInput(onClick) {
                        awaitEachGesture {
                            var activePointer: PointerId? = null
                            var moved = false
                            while (true) {
                                val event = awaitPointerEvent()
                                if (activePointer == null) {
                                    val downChange = event.changes.firstOrNull { it.pressed } ?: continue
                                    activePointer = downChange.id
                                    downChange.consume()
                                    continue
                                }
                                val change = event.changes.firstOrNull { it.id == activePointer } ?: break
                                if (change.positionChanged()) {
                                    moved = true
                                }
                                change.consume()
                                if (!change.pressed) {
                                    if (!moved) {
                                        onClick()
                                    }
                                    break
                                }
                            }
                        }
                    }
                } else {
                    Modifier.pointerInput(onDisabledClick) {
                        if (onDisabledClick == null) {
                            return@pointerInput
                        }
                        awaitEachGesture {
                            var activePointer: PointerId? = null
                            var moved = false
                            while (true) {
                                val event = awaitPointerEvent()
                                if (activePointer == null) {
                                    val downChange = event.changes.firstOrNull { it.pressed } ?: continue
                                    activePointer = downChange.id
                                    downChange.consume()
                                    continue
                                }
                                val change = event.changes.firstOrNull { it.id == activePointer } ?: break
                                if (change.positionChanged()) {
                                    moved = true
                                }
                                change.consume()
                                if (!change.pressed) {
                                    if (!moved) {
                                        onDisabledClick()
                                    }
                                    break
                                }
                            }
                        }
                    }
                },
            ),
        shape = iconShape,
        color = if (selected) selectedColor ?: uiTheme.controls.buttonBg.copy(alpha = 0.9f) else backgroundColor ?: uiTheme.controls.buttonBg,
        contentColor = uiTheme.controls.buttonFg,
        shadowElevation = 2.dp,
    ) {
        Box(
            modifier = Modifier.fillMaxSize(),
            contentAlignment = if (centered) Alignment.Center else Alignment.CenterStart,
        ) {
            val heavyFrameThickness = if (iconResId != null) with(LocalDensity.current) { 8f.toDp() } else 0.dp
            if (iconResId != null) {
                Box(
                    modifier = Modifier
                        .matchParentSize()
                        .padding(4.dp)
                        .clip(RoundedCornerShape(ThumbRadius * 0.92f))
                        .background(uiTheme.controls.buttonBg),
                )
            }
            if (accentColor != null) {
                Box(
                    modifier = Modifier
                        .fillMaxWidth()
                        .height(ThumbSize / 2f)
                        .align(Alignment.BottomStart)
                        .background(accentColor.copy(alpha = if (enabled) 1f else 0.45f)),
                )
            }
            if (iconResId != null) {
                IconFrame(
                    iconResId = iconResId,
                    modifier = Modifier
                        .fillMaxSize()
                        .padding(4.dp + heavyFrameThickness),
                )
                OutlinedButtonLabel(
                    text = label,
                    modifier = Modifier
                        .align(Alignment.BottomCenter)
                        .fillMaxWidth()
                        .padding(horizontal = if (wide) 0.dp else 1.dp, vertical = 2.dp)
                        .then(textModifier),
                    style = MaterialTheme.typography.labelSmall.copy(fontSize = 13.sp),
                    maxLines = maxLines,
                    color = LocalAerobagUiTheme.current.controls.buttonFg,
                )
            } else {
                Text(
                    text = label,
                    modifier = (if (centered) Modifier else Modifier.padding(start = textStartPadding, end = 8.dp)).then(textModifier),
                    style = MaterialTheme.typography.labelSmall,
                    maxLines = maxLines,
                    overflow = TextOverflow.Clip,
                )
            }
            if (!enabled) {
                Box(
                    modifier = Modifier
                        .matchParentSize()
                        .background(Color(0x42000000)),
                )
            }
        }
    }
}

@Composable
private fun Scrim(modifier: Modifier = Modifier, onDismiss: () -> Unit) {
    Box(
        modifier = modifier
            .fillMaxSize()
            .background(Color(0x3D0A1014))
            .clickable(
                indication = null,
                interactionSource = remember { MutableInteractionSource() },
            ) { onDismiss() },
    ) {}
}
