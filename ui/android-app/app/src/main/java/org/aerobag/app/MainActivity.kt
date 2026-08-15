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
import android.os.Handler
import android.os.Looper
import android.os.SystemClock
import android.util.Log
import android.view.KeyEvent as AndroidKeyEvent
import android.view.MotionEvent
import android.view.WindowManager
import java.util.LinkedHashMap
import java.net.HttpURLConnection
import kotlin.math.roundToInt
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
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.Checkbox
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Slider
import androidx.compose.material3.SliderDefaults
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableLongStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.foundation.rememberScrollState
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
import androidx.compose.ui.platform.LocalUriHandler
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
import androidx.lifecycle.ViewModel
import androidx.lifecycle.ViewModelProvider
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
import org.aerobag.app.domain.ChartAirportMenuEntry
import org.aerobag.app.domain.ChartAsset
import org.aerobag.app.domain.AirwayPresentationPlan
import org.aerobag.app.domain.AirwaySuggestion
import org.aerobag.app.domain.AirportInfoUiView
import org.aerobag.app.domain.WaypointIdentifierSuggestion
import org.aerobag.app.domain.CoreMapViewport
import org.aerobag.app.domain.DebugFlagId
import org.aerobag.app.domain.DerivedChartPageState
import org.aerobag.app.domain.FlightPlanEntryPreview
import org.aerobag.app.domain.FlightDataCell
import org.aerobag.app.domain.FlightPlanDisplayRowKind
import org.aerobag.app.domain.FlightPlanDisplayRowUiView
import org.aerobag.app.domain.FlightPlanRowActionUiView
import org.aerobag.app.domain.FlightPlanRouteSegment
import org.aerobag.app.domain.FlightPlanUiState
import org.aerobag.app.domain.InstalledPackages
import org.aerobag.app.domain.AirspaceDisplayDecoration
import org.aerobag.app.domain.AirspaceDisplayDecorationSegment
import org.aerobag.app.domain.AirspaceDisplayLabel
import org.aerobag.app.domain.AirspaceDisplayPath
import org.aerobag.app.domain.AirspaceDisplaySubpath
import org.aerobag.app.domain.AirspaceLimitGlyph
import org.aerobag.app.domain.AirspaceScreenPoint
import org.aerobag.app.domain.MapLayerId
import org.aerobag.app.domain.MapFollowUiState
import org.aerobag.app.domain.MapOrientationMode
import org.aerobag.app.domain.MapOverlayQueryResult
import org.aerobag.app.domain.MapSelectionAction
import org.aerobag.app.domain.MapSelectionDetailStatus
import org.aerobag.app.domain.MapSelectionHighlight
import org.aerobag.app.domain.MapSelectionItem
import org.aerobag.app.domain.MapSelectionQueryResult
import org.aerobag.app.domain.MapFamilyOption
import org.aerobag.app.domain.MapViewportState
import org.aerobag.app.domain.NativeAppCoreAdapter
import org.aerobag.app.domain.NativeBindings
import org.aerobag.app.domain.NativeSessionCommandRejectedException
import org.aerobag.app.domain.NativeUiSession
import org.aerobag.app.domain.NavKvStore
import org.aerobag.app.domain.NavRef
import org.aerobag.app.domain.NavElementUiView
import org.aerobag.app.domain.OfflineRegionDisplay
import org.aerobag.app.domain.OwnshipControlModel
import org.aerobag.app.domain.OwnshipLauncherTextTone
import org.aerobag.app.domain.OwnshipMode
import org.aerobag.app.domain.OwnshipRenderState
import org.aerobag.app.domain.OwnshipSelection
import org.aerobag.app.domain.OwnshipSourceKind
import org.aerobag.app.domain.PackageZipStore
import org.aerobag.app.domain.RasterMapUiState
import org.aerobag.app.domain.PlaybackStatus
import org.aerobag.app.domain.PlaybackUiState
import org.aerobag.app.domain.ProcedureKind
import org.aerobag.app.domain.ProcedureLoadOption
import org.aerobag.app.domain.ProcedureOptions
import org.aerobag.app.domain.ProcedureSummary
import org.aerobag.app.domain.RenderTile
import org.aerobag.app.domain.RouteSegmentStatus
import org.aerobag.app.domain.RouteComponentViewKind
import org.aerobag.app.domain.RuntimeContent
import org.aerobag.app.domain.ScreenPoint
import org.aerobag.app.domain.SectionalPackages
import org.aerobag.app.domain.AndroidRuntimeContent
import org.aerobag.app.domain.MapDisplayFrame
import org.aerobag.app.domain.SequencingMode
import org.aerobag.app.domain.SituationControlInput
import org.aerobag.app.domain.SituationRingCandidate
import org.aerobag.app.domain.TileStorageKind
import org.aerobag.app.domain.UiDataStatusPageFact
import org.aerobag.app.domain.UiDataStatusPageRow
import org.aerobag.app.domain.UiDataStatusPageState
import org.aerobag.app.domain.UiDataStatusPageTimeDisplay
import org.aerobag.app.domain.UiDataStatusState
import org.aerobag.app.domain.UiDebugState
import org.aerobag.app.domain.UiDisclaimerState
import org.aerobag.app.domain.UiDisplayPolicy
import org.aerobag.app.domain.UiMapLayerToggleState
import org.aerobag.app.domain.UiSettingsPageRow
import org.aerobag.app.domain.UiSettingsPageState
import org.aerobag.app.domain.UiStatusActionStyle
import org.aerobag.app.domain.UiStatusSeverity
import org.aerobag.app.domain.UiTheme
import org.aerobag.app.domain.UiThemeLoader
import org.aerobag.app.domain.UiSessionSnapshot
import org.aerobag.app.domain.VisibleMapFeature
import org.aerobag.app.domain.VisibleMetarFeature
import org.aerobag.app.domain.VisiblePirepFeature
import org.aerobag.app.domain.WeatherDetailUiView
import org.aerobag.app.domain.WorldPoint
import org.aerobag.app.domain.applyPinchGesture
import org.aerobag.app.domain.clampZoom
import org.aerobag.app.domain.createInitialImageViewport
import org.aerobag.app.domain.createInitialViewport
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
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.atomic.AtomicInteger
import java.util.concurrent.atomic.AtomicLong
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
internal val PrimaryNavigationDockWidth = (ThumbSize * 5f) + (ThumbGap * 2f)
internal val BottomRightControlClearance = ThumbSize + (ThumbGap * 2f)
internal val PlanGridGap = 2.dp
internal const val DefaultPlaybackTracePath = "/gps-captures/black-tablet-flight-2026-06-27-0800-1700-pdt.jsonl"
internal const val DefaultAndroidPackageSourceBaseUrl = "aerobag.org"
internal const val CurrentArtifactsDiscoveryFilename = "current_artifacts.json"
internal const val PublicationPackageRootPath = "packages"
internal const val WebMercatorWorldSize = 256.0
internal const val WebMercatorHalfWorldM = 20037508.342789244
internal const val TerrainAltitudeBucketFt = 200
internal const val MapLayerLogTag = "MapLayers"
internal const val TileBudgetLogTag = "AerobagTileBudget"
internal const val DecodedTileCacheMaxBytes = 96L * 1024L * 1024L
private const val SessionCommandNoticeDurationMs = 4_000L

internal fun shouldRaiseBottomCornerControls(surfaceWidth: Dp): Boolean =
    surfaceWidth > 0.dp &&
        surfaceWidth < PrimaryNavigationDockWidth + (BottomRightControlClearance * 2f)
internal const val MapTileLoadWorkerCount = 4
internal const val SlowTileLoadLogMs = 1000L
internal val TileLoadGenerationIds = AtomicLong()

internal data class PageTilePaintTiming(
    val id: Long,
    val fromPage: AppPage,
    val startedMs: Long,
    val trigger: String,
)

@kotlinx.serialization.Serializable
internal data class WireRasterTilePlan(
    val tiles: List<WireRasterTileDraw> = emptyList(),
    val chart_reference_action: WireChartReferenceAction? = null,
)

@kotlinx.serialization.Serializable
internal data class WireChartReferenceAction(
    val family_id: String,
    val suggested_chart_ids: List<String> = emptyList(),
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
internal const val UiPrefsOfflinePackageLibraryCacheKey = "offline_package_library_cache"
internal const val UiPrefsPackageSourceBaseUrlKey = "package_source_base_url"
internal const val UiPrefsDebugGpsCaptureKey = "debug_gps_capture"
internal const val UiPrefsMapOrientationModeKey = "map_orientation_mode"
internal const val MapViewportLogTag = "MapViewport"
internal const val MaxViewHistoryDepth = 64
internal const val OverlayPlaneControls = 10f
internal const val OverlayPlaneModalScrim = 80f
internal const val OverlayPlaneModal = 90f
internal fun defaultUiDebugState(gpsCapture: Boolean = false) = UiDebugState(
    tileLabels = false,
    nexradTileLabels = false,
    fastTiles = false,
    offlineSimulatedClockButtons = false,
    plateFlightPlan = false,
    badAutopilot = false,
    internetAdsb = false,
    gpsCapture = gpsCapture,
    debugLogToDeveloperServer = false,
)
internal val PackageManagementJson = Json {
    encodeDefaults = true
    ignoreUnknownKeys = true
    classDiscriminator = "kind"
}

internal enum class AppPage {
    Map,
    Plan,
    AltitudePlanner,
    Charts,
    Home,
    DataStatus,
    Settings,
    Cloud,
    OfflinePackages,
}

internal data class AppViewSnapshot(
    val page: AppPage,
    val selectedMapId: String,
    val selectedMapLauncherLabel: String,
    val mapViewport: MapViewportState,
    val plateTargetAirportId: String?,
    val selectedAirportId: String,
    val selectedReferenceFamilyId: String?,
    val selectedChartId: String,
    val selectedChartLabel: String,
    val suggestedChartIds: List<String>,
    val recentAirportIds: List<String>,
    val chartViewport: org.aerobag.app.domain.ImageViewportState?,
    val chartFolderOpen: Boolean,
)

private data class SessionCommandNotice(
    val id: Long,
    val message: String,
)

internal data class MapSelectionUiState(
    val point: Offset,
    val result: MapSelectionQueryResult,
    val selectedItem: MapSelectionItem?,
    val detailModal: MapSelectionDetailModalState? = null,
    val centeredTargetLabel: String? = null,
    val centeredTargetPosition: org.aerobag.app.domain.LatLonPoint? = null,
    val centeredViewport: MapViewportState? = null,
)

internal data class MapSelectionDetailModalState(
    val title: String,
    val text: String? = null,
    val status: MapSelectionDetailStatus? = null,
    val weatherDetail: WeatherDetailUiView? = null,
    val airportInfo: AirportInfoUiView? = null,
)

internal data class FlightPlanDisplayRow(
    val id: String,
    val label: String,
    val rowKind: String,
    val componentKind: RouteComponentViewKind? = null,
    val componentUid: String? = null,
    val procedureId: String? = null,
    val procedureKind: org.aerobag.app.domain.ProcedureKind? = null,
    val dataCells: List<FlightDataCell> = emptyList(),
    val showPlateTargetId: String? = null,
    val chartAirportId: String? = null,
    val navRef: NavRef? = null,
    val symbolFeature: org.aerobag.app.domain.NavSymbolFeature? = null,
    val weatherBadge: org.aerobag.app.domain.FlightPlanWeatherBadgeUiView? = null,
    val depth: Int = 0,
    val active: Boolean = false,
    val enabled: Boolean = true,
    val disabledReason: String? = null,
    val syntheticDirectTo: Boolean = false,
    val canAddAirwayAfter: Boolean = false,
    val canAddProcedureBefore: Boolean = false,
    val canRemoveComponent: Boolean = false,
    val canReorderComponent: Boolean = false,
    val canReorderUp: Boolean = false,
    val canReorderDown: Boolean = false,
    val actionMatrix: List<List<FlightPlanRowActionUiView>> = emptyList(),
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
    val selectedEntryUid: String?,
)

internal data class AndroidProcedurePickerState(
    val loading: Boolean,
    val error: String?,
    val rowUid: String,
    val airportId: String,
    val kind: ProcedureKind,
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
    @SerialName("family_id")
    val familyId: String? = null,
    @SerialName("region_id")
    val regionId: String? = null,
    @SerialName("chart_package_tier")
    val chartPackageTier: String? = null,
)


@Serializable
internal data class OfflinePackagesUiRowWire(
    val id: String,
    val label: String = id,
    val selection: OfflinePackageSelection,
    @SerialName("selection_event")
    val selectionEvent: OfflinePackagesEventWire? = null,
    @SerialName("help_text")
    val helpText: String? = null,
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
    @SerialName("planned_change_label")
    val plannedChangeLabel: String = "+0M",
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
    @SerialName("zoom_levels")
    val zoomLevels: List<OfflinePackagesUiRowWire> = emptyList(),
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
    val metadata: BundlePackageMetadataWire? = null,
)

@Serializable
internal data class BundlePackageMetadataWire(
    @SerialName("chart_package_tier")
    val chartPackageTier: String? = null,
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
)

@Serializable
internal data class OfflinePackagesSyncProgressWire(
    @SerialName("planned_fetch_artifact_ids")
    val plannedFetchArtifactIds: Set<String> = emptySet(),
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
    @SerialName("library_status_message")
    val libraryStatusMessage: String? = null,
    @SerialName("sync_in_flight")
    val syncInFlight: Boolean = false,
    @SerialName("sync_message")
    val syncMessage: String? = null,
    @SerialName("storage_capacity_label")
    val storageCapacityLabel: String? = null,
    @SerialName("package_source_editable")
    val packageSourceEditable: Boolean = true,
    @SerialName("package_source_edit_disabled_reason")
    val packageSourceEditDisabledReason: String? = null,
    @SerialName("refresh_enabled")
    val refreshEnabled: Boolean = false,
    @SerialName("refresh_disabled_reason")
    val refreshDisabledReason: String? = null,
    @SerialName("refresh_cancel_enabled")
    val refreshCancelEnabled: Boolean = false,
    @SerialName("sync_enabled")
    val syncEnabled: Boolean = false,
    @SerialName("sync_disabled_reason")
    val syncDisabledReason: String? = null,
    @SerialName("sync_cancel_enabled")
    val syncCancelEnabled: Boolean = false,
    @SerialName("planner_interactions_enabled")
    val plannerInteractionsEnabled: Boolean = true,
    @SerialName("planner_interactions_disabled_reason")
    val plannerInteractionsDisabledReason: String? = null,
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
    @SerialName("library_cache_json")
    val libraryCacheJson: String? = null,
    @SerialName("ui_state")
    val uiState: OfflinePackagesControllerUiStateWire,
    val command: OfflinePackagesControllerCommandWire? = null,
    @SerialName("preferences_for_cloud_json")
    val preferencesForCloudJson: String? = null,
    @SerialName("installed_metadata_updates")
    val installedMetadataUpdates: List<InstalledArtifactMetadataUpdateWire> = emptyList(),
)

@Serializable
internal data class InstalledArtifactMetadataUpdateWire(
    @SerialName("artifact_id")
    val artifactId: String,
    val filename: String,
    @SerialName("family_id")
    val familyId: String,
    @SerialName("region_id")
    val regionId: String? = null,
    @SerialName("chart_package_tier")
    val chartPackageTier: String? = null,
)

@Serializable
internal data class OfflinePackagesInitInputWire(
    val state: OfflinePackagesStateWire? = null,
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
    @SerialName("packages_event")
    data class PackagesEvent(
        val event: OfflinePackagesEventWire,
    ) : OfflinePackagesControllerEventWire

    @Serializable
    @SerialName("apply_synchronized_preferences")
    data class ApplySynchronizedPreferences(
        @SerialName("preferences_json")
        val preferencesJson: String,
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
    @SerialName("now_epoch_ms")
    val nowEpochMs: Long,
    val installed: List<InstalledArtifactWire>,
    val storage: OfflinePackagesStorageInfoWire? = null,
    val event: OfflinePackagesControllerEventWire,
)

@Serializable
internal data class OfflinePackagesStorageInfoWire(
    @SerialName("available_bytes")
    val availableBytes: Long,
    @SerialName("total_bytes")
    val totalBytes: Long? = null,
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
    val externalUrl: String? = null,
    val enabled: Boolean = false,
    val disabledReason: String? = null,
    @DrawableRes val iconResId: Int? = null,
)

internal data class MenuDockOption(
    val key: String,
    val label: String,
    val separator: Boolean = false,
    val active: Boolean = false,
    val enabled: Boolean = true,
    val disabledReason: String? = null,
    val accentColor: Color? = null,
    val toggleState: UiMapLayerToggleState? = null,
    @DrawableRes val iconResId: Int? = null,
    val accessoryContentDescription: String? = null,
    @DrawableRes val accessoryIconResId: Int? = null,
    val accessoryTestTag: String? = null,
    val onAccessorySelect: (() -> Unit)? = null,
    val dismissTrayOnSelect: Boolean = false,
    val onSelect: () -> Unit = {},
) {
    companion object {
        fun separator(key: String, label: String) = MenuDockOption(
            key = key,
            label = label,
            separator = true,
            enabled = false,
        )
    }
}

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
    AltitudePlanner(
        buttonWidth = ThumbSize * 2.2f,
        trayWidth = ThumbSize * 4f,
        launcherMaxLines = 2,
    ),
}

internal val PageOptions = listOf(
    PageTrayOption(AppPage.Map, "CHART", "CHART", R.drawable.page_chart_icon),
    PageTrayOption(AppPage.Charts, "PLATE", "PLATE", R.drawable.page_plate_icon),
    PageTrayOption(AppPage.Plan, "FLIGHT PLAN", "PLAN", R.drawable.page_plan1_icon),
    PageTrayOption(AppPage.AltitudePlanner, "ALTITUDE PLANNER", "ALT"),
    PageTrayOption(AppPage.Home, "HOME", "HOME", R.drawable.page_home_icon),
    PageTrayOption(AppPage.DataStatus, "STATUS", "STATUS"),
    PageTrayOption(AppPage.Settings, "SETTINGS", "SET"),
    PageTrayOption(AppPage.OfflinePackages, "OFFLINE PACKAGES", "PKG"),
)

internal fun mostRecentChartOrPlatePageFromHistory(pageHistory: List<AppViewSnapshot>): AppPage =
    pageHistory
        .asReversed()
        .firstOrNull { it.page == AppPage.Map || it.page == AppPage.Charts }
        ?.page
        ?: AppPage.Map

internal data class ChartTrayOption(
    val id: String,
    val label: String,
    val launcherLabel: String,
    val available: Boolean,
    val disabledReason: String? = null,
    @DrawableRes val iconResId: Int? = null,
    val select: (() -> Unit)?,
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
internal fun chartFamilyIconResId(chartFamilyId: String): Int = when (chartFamilyId) {
    "sec" -> R.drawable.sectional_icon
    "tac" -> R.drawable.tac_icon
    "flyway" -> R.drawable.flyway_icon
    "enr-l" -> R.drawable.ifr_l_icon
    "enr-h" -> R.drawable.ifr_h_icon
    "shaded-relief" -> R.drawable.shaded_relief_icon
    "world-basemap" -> R.drawable.layer_world_basemap_icon
    else -> R.drawable.page_chart_icon
}

@DrawableRes
internal fun mapLayerIconResId(layerId: MapLayerId): Int = when (layerId) {
    MapLayerId.WorldBasemap -> R.drawable.layer_world_basemap_icon
    MapLayerId.Vectors -> R.drawable.layer_vectors_icon
    MapLayerId.Metars -> R.drawable.layer_observations_icon
    MapLayerId.Nexrad -> R.drawable.layer_nexrad_icon
    MapLayerId.Traffic -> R.drawable.layer_adsb_icon
    MapLayerId.TerrainWarning -> R.drawable.layer_terrain_warning_icon
    MapLayerId.OfflineRegions -> R.drawable.layer_offline_regions_icon
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

internal fun plateFolderColor(uiTheme: UiTheme, category: String): Color =
    uiTheme.plateFolder.labelColors[category] ?: uiTheme.plateFolder.labelColors["other"] ?: Color(0xFF52656D)

internal fun mapViewportFromCore(viewport: CoreMapViewport): MapViewportState {
    val center = latLonToWorld(viewport.center.lat, viewport.center.lon)
    return MapViewportState(
        centerWorldX = center.x,
        centerWorldY = center.y,
        zoom = viewport.zoom,
        rotationDeg = viewport.rotationDeg,
    )
}

@Composable
internal fun SituationStatusBadge(
    controls: OwnshipControlModel,
    modifier: Modifier = Modifier,
    open: Boolean,
    onToggle: () -> Unit,
    onSelectSource: (org.aerobag.app.domain.OwnshipSourceMenuItem) -> Unit = {},
    onSituationControlInput: (SituationControlInput) -> Unit = {},
    onTextAction: (String, String) -> Unit = { _, _ -> },
) {
    val trayColumnCount = max(controls.sources.size, controls.situationControls.size).coerceAtLeast(1)
    val trayWidth = (ThumbSize * trayColumnCount.toFloat()) + (3.dp * (trayColumnCount - 1).toFloat()) + 6.dp
    val uiTheme = LocalAerobagUiTheme.current
    val launcherForegroundColor = when (controls.launcherTextTone) {
        OwnshipLauncherTextTone.Normal -> uiTheme.controls.situationStatusFg
        OwnshipLauncherTextTone.Unavailable -> uiTheme.controls.situationStatusUnavailableFg
    }
    Box(modifier = modifier.wrapContentSize(unbounded = true, align = Alignment.TopEnd)) {
        MenuDock(
            launcherLabel = controls.launcherLabel,
            open = open,
            onToggle = onToggle,
            style = MenuDockStyle.Situation,
            launcherTestTag = "parity:ownship-launcher",
            launcherForegroundColor = launcherForegroundColor,
            trayWidthOverride = trayWidth,
            options = emptyList(),
            body = {
                SituationSourceRow(
                    sources = controls.sources,
                    onSelectSource = { source ->
                        onSelectSource(source)
                    },
                )
            },
            footer = {
                Column(verticalArrangement = Arrangement.spacedBy(3.dp)) {
                    controls.textAction?.let { control ->
                        OwnshipTextActionControl(control, onTextAction)
                    }
                    SituationTransportRow(
                        controls = controls.situationControls,
                        onInput = onSituationControlInput,
                    )
                }
            },
        )
    }
}

@Composable
internal fun OwnshipTextActionControl(
    control: org.aerobag.app.domain.OwnshipTextAction,
    onSubmit: (String, String) -> Unit,
) {
    var value by remember(control.actionId) { mutableStateOf(control.value) }
    val context = androidx.compose.ui.platform.LocalContext.current
    LaunchedEffect(control.actionId, control.value) {
        value = control.value
    }
    val uiTheme = LocalAerobagUiTheme.current
    Column(
        verticalArrangement = Arrangement.spacedBy(2.dp),
        modifier = Modifier.fillMaxWidth(),
    ) {
        Text(
            text = control.label.uppercase(),
            fontSize = 11.sp,
            fontWeight = FontWeight.Bold,
            color = uiTheme.controls.buttonFg,
        )
        Row(
            horizontalArrangement = Arrangement.spacedBy(3.dp),
            verticalAlignment = Alignment.CenterVertically,
            modifier = Modifier.fillMaxWidth(),
        ) {
            BasicTextField(
                value = value,
                onValueChange = { value = it.uppercase() },
                singleLine = true,
                textStyle = MaterialTheme.typography.bodyLarge.copy(
                    color = uiTheme.controls.buttonFg,
                    fontWeight = FontWeight.Bold,
                ),
                keyboardOptions = KeyboardOptions(
                    capitalization = KeyboardCapitalization.Characters,
                    imeAction = ImeAction.Go,
                ),
                keyboardActions = KeyboardActions(
                    onGo = { if (control.enabled) onSubmit(control.actionId, value) },
                ),
                decorationBox = { innerTextField ->
                    Box(
                        contentAlignment = Alignment.CenterStart,
                        modifier = Modifier
                            .background(uiTheme.controls.buttonUnchecked, RoundedCornerShape(3.dp))
                            .border(1.dp, uiTheme.controls.buttonFg.copy(alpha = 0.45f), RoundedCornerShape(3.dp))
                            .padding(horizontal = 6.dp),
                    ) {
                        if (value.isEmpty()) {
                            Text(
                                text = control.placeholder,
                                color = uiTheme.controls.buttonFg.copy(alpha = 0.55f),
                                fontSize = 15.sp,
                            )
                        }
                        innerTextField()
                    }
                },
                modifier = Modifier
                    .weight(1f)
                    .height(ThumbSize * 0.58f),
            )
            CompactSquareButton(
                label = control.submitLabel,
                enabled = control.enabled,
                wide = false,
                modifier = Modifier.size(ThumbSize, ThumbSize * 0.58f),
                onDisabledClick = control.disabledReason?.let { reason ->
                    { showDisabledActionToast(context, reason) }
                },
                onClick = { onSubmit(control.actionId, value) },
            )
        }
    }
}

@Composable
internal fun SituationSourceRow(
    sources: List<org.aerobag.app.domain.OwnshipSourceMenuItem>,
    onSelectSource: (org.aerobag.app.domain.OwnshipSourceMenuItem) -> Unit,
) {
    val context = LocalContext.current
    Row(
        horizontalArrangement = Arrangement.spacedBy(3.dp),
        modifier = Modifier.fillMaxWidth(),
    ) {
        sources.forEach { source ->
            CompactSquareButton(
                label = situationSourceButtonLabel(source),
                enabled = source.enabled,
                selected = source.active,
                wide = false,
                modifier = Modifier.size(ThumbSize),
                maxLines = 2,
                testTag = "parity:ownship-source:${source.sourceId}",
                onDisabledClick = source.disabledReason?.let { reason ->
                    { showDisabledActionToast(context, reason) }
                },
                onClick = { onSelectSource(source) },
            )
        }
    }
}

internal fun situationSourceButtonLabel(source: org.aerobag.app.domain.OwnshipSourceMenuItem): String =
    source.label

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
            SituationTransportButton(control.label, control.input, control.enabled, control.disabledReason, onInput)
        }
    }
}

@Composable
internal fun SituationTransportButton(
    label: String,
    input: SituationControlInput,
    enabled: Boolean,
    disabledReason: String?,
    onInput: (SituationControlInput) -> Unit,
) {
    val context = LocalContext.current
    CompactSquareButton(
        label = label,
        enabled = enabled,
        wide = false,
        modifier = Modifier
            .size(ThumbSize),
        onDisabledClick = disabledReason?.let { reason ->
            { showDisabledActionToast(context, reason) }
        },
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
        headingDeg = heading - viewport.rotationDeg.toFloat(),
        predictorUnits = predictor,
        ring = selectSituationRing(
            position,
            viewport,
            widthUnits,
            heightUnits,
            ringCandidates,
            ownship.magneticVariationDeg?.toFloat()?.minus(viewport.rotationDeg.toFloat()),
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
    val screenPoint = MapDisplayFrame(viewport, widthUnits, heightUnits).latLonToScreen(lat, lon)
    return Offset(screenPoint.x, screenPoint.y)
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
    val screenPoint = MapDisplayFrame(viewport, widthUnits, heightUnits).worldToScreen(
        WorldPoint(world.x.toDouble(), world.y.toDouble()),
    )
    return Offset(screenPoint.x, screenPoint.y)
}

internal fun screenToWorldOffset(
    viewport: MapViewportState,
    screenX: Float,
    screenY: Float,
    widthUnits: Float,
    heightUnits: Float,
): Offset {
    val world = MapDisplayFrame(viewport, widthUnits, heightUnits).screenToWorld(ScreenPoint(screenX, screenY))
    return Offset(world.x.toFloat(), world.y.toFloat())
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
        flightPlanFeatures = overlay.flightPlanFeatures.map { feature ->
            transformVisibleFeature(feature, fromViewport, fromSurface, toViewport, toSurface)
        },
        visibleMetars = overlay.visibleMetars.map { feature ->
            transformVisibleMetarFeature(feature, fromViewport, fromSurface, toViewport, toSurface)
        },
        visiblePireps = overlay.visiblePireps.map { feature ->
            transformVisiblePirepFeature(feature, fromViewport, fromSurface, toViewport, toSurface)
        },
        visibleTraffic = overlay.visibleTraffic.map { feature ->
            val transformed = transformScreenPoint(
                x = feature.screenX,
                y = feature.screenY,
                fromViewport = fromViewport,
                fromSurface = fromSurface,
                toViewport = toViewport,
                toSurface = toSurface,
            )
            feature.copy(screenX = transformed.x, screenY = transformed.y)
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
        offlineRegions = overlay.offlineRegions.map { region ->
            transformOfflineRegionDisplay(region, fromViewport, fromSurface, toViewport, toSurface)
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

internal fun transformOfflineRegionDisplay(
    region: OfflineRegionDisplay,
    fromViewport: MapViewportState,
    fromSurface: OverlaySurfaceUnits,
    toViewport: MapViewportState,
    toSurface: OverlaySurfaceUnits,
): OfflineRegionDisplay {
    val label = transformScreenPoint(
        x = region.labelX,
        y = region.labelY,
        fromViewport = fromViewport,
        fromSurface = fromSurface,
        toViewport = toViewport,
        toSurface = toSurface,
    )
    return region.copy(
        points = region.points.map { point ->
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
        labelX = label.x,
        labelY = label.y,
    )
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
    val transformed = MapDisplayFrame(toViewport, toSurface.width, toSurface.height).transformScreenPointFrom(
        from = MapDisplayFrame(fromViewport, fromSurface.width, fromSurface.height),
        point = ScreenPoint(x.toFloat(), y.toFloat()),
    )
    return AirspaceScreenPoint(
        x = transformed.x.toDouble(),
        y = transformed.y.toDouble(),
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
    return runCatching { AppPage.valueOf(stored) }.getOrDefault(AppPage.Map)
}

internal fun readStoredGpsCaptureDebugFlag(prefs: SharedPreferences): Boolean =
    prefs.getBoolean(UiPrefsDebugGpsCaptureKey, false)

internal fun readStoredMapOrientationMode(prefs: SharedPreferences): MapOrientationMode =
    if (prefs.getString(UiPrefsMapOrientationModeKey, null) == "track") {
        MapOrientationMode.Track
    } else {
        MapOrientationMode.North
    }

internal fun writeStoredMapOrientationMode(prefs: SharedPreferences, mode: MapOrientationMode) {
    prefs.edit()
        .putString(UiPrefsMapOrientationModeKey, if (mode == MapOrientationMode.Track) "track" else "north")
        .apply()
}

internal fun writeStoredGpsCaptureDebugFlag(prefs: SharedPreferences, enabled: Boolean) {
    prefs.edit().putBoolean(UiPrefsDebugGpsCaptureKey, enabled).apply()
}

internal fun summarizeRuntimeLoadFailure(error: Throwable): String {
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
        "Runtime loading failed."
    } else {
        "Runtime loading failed: ${chain.joinToString(" <- ")}"
    }
}

private const val DebugArmLayerNavKvFaultExtra =
    "org.aerobag.app.extra.DEBUG_ARM_LAYER_NAV_KV_FAULT"

class MainActivity : ComponentActivity() {
    var onHardwareZoomDelta: ((Double) -> Boolean)? = null
    var onSituationControlInput: ((SituationControlInput) -> Boolean)? = null

    private val displayPolicyHandler = Handler(Looper.getMainLooper())
    private var displayPolicyForeground = false
    private var displayPolicyWindowFocused = false
    private var displayPolicyTopResumed = true
    private var activeDisplayPolicy: UiDisplayPolicy? = null
    private val displayDimRunnable = Runnable {
        applyDisplayPolicyDimState()
    }

    private val gpsPermissionLauncher =
        registerForActivityResult(ActivityResultContracts.RequestMultiplePermissions()) { grants ->
            if (grants[Manifest.permission.ACCESS_FINE_LOCATION] == true) {
                startAerobagGpsService()
            } else {
                AndroidGpsSource.publishStatus(AndroidGpsSource.unavailableStatus("Precise location required"))
            }
        }

    fun applyCoreDisplayPolicy(policy: UiDisplayPolicy?) {
        activeDisplayPolicy = policy
        syncDisplayPolicy()
    }

    private fun syncDisplayPolicy() {
        displayPolicyHandler.removeCallbacks(displayDimRunnable)
        val policy = activeDisplayPolicy
        if (!displayPolicyCanControlWindow() || policy?.keepScreenOn != true) {
            window.clearFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
            restoreSystemWindowBrightness()
            return
        }
        window.addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
        restoreSystemWindowBrightness()
        val dimAfterMs = policy.dimAfterMs ?: return
        displayPolicyHandler.postDelayed(displayDimRunnable, dimAfterMs.coerceAtLeast(1L))
    }

    private fun applyDisplayPolicyDimState() {
        val policy = activeDisplayPolicy ?: return
        if (!displayPolicyCanControlWindow() || !policy.keepScreenOn || policy.dimAfterMs == null) {
            return
        }
        val attrs = window.attributes
        attrs.screenBrightness = policy.dimBrightness.coerceIn(0.0f, 1.0f)
        window.attributes = attrs
    }

    private fun displayPolicyCanControlWindow(): Boolean =
        displayPolicyForeground && displayPolicyWindowFocused && displayPolicyTopResumed

    private fun restoreSystemWindowBrightness() {
        val attrs = window.attributes
        if (attrs.screenBrightness == WindowManager.LayoutParams.BRIGHTNESS_OVERRIDE_NONE) {
            return
        }
        attrs.screenBrightness = WindowManager.LayoutParams.BRIGHTNESS_OVERRIDE_NONE
        window.attributes = attrs
    }

    private fun noteDisplayUserActivity() {
        syncDisplayPolicy()
    }

    @OptIn(ExperimentalComposeUiApi::class)
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        runCatching {
            NativeBindings.configureGpsCaptureLogPath(
                File(filesDir, "aerobag-gps-capture.jsonl").absolutePath,
            )
        }.onFailure { error ->
            Log.w("AerobagGpsCapture", "failed to configure GPS capture log path", error)
        }
        val retainedModel = ViewModelProvider(this)[AerobagRetainedModel::class.java]
        if (intent?.getBooleanExtra(OpenOfflinePackagesExtra, false) == true) {
            retainedModel.page = AppPage.OfflinePackages
            intent?.removeExtra(OpenOfflinePackagesExtra)
        }
        val perfScenario = androidPerfScenarioFromIntentValue(
            intent?.getStringExtra(AndroidPerfScenarioExtra),
        )
        val armLayerNavKvFault = BuildConfig.DEBUG &&
            intent?.getBooleanExtra(DebugArmLayerNavKvFaultExtra, false) == true
        intent?.removeExtra(DebugArmLayerNavKvFaultExtra)
        requestAndroidGps()
        setContent {
            MaterialTheme {
                Surface(
                    modifier = Modifier
                        .fillMaxSize()
                        .semantics { testTagsAsResourceId = true },
                    color = Color(0xFFF3EFE4),
                ) {
                    AerobagApp(
                        retainedModel = retainedModel,
                        perfScenario = perfScenario,
                        armLayerNavKvFault = armLayerNavKvFault,
                    )
                }
            }
        }
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        setIntent(intent)
        if (intent.getBooleanExtra(OpenOfflinePackagesExtra, false)) {
            intent.removeExtra(OpenOfflinePackagesExtra)
            ViewModelProvider(this)[AerobagRetainedModel::class.java].page = AppPage.OfflinePackages
            recreate()
        }
    }

    override fun onResume() {
        super.onResume()
        displayPolicyForeground = true
        syncDisplayPolicy()
    }

    override fun onPause() {
        displayPolicyForeground = false
        displayPolicyHandler.removeCallbacks(displayDimRunnable)
        window.clearFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
        restoreSystemWindowBrightness()
        super.onPause()
    }

    override fun onWindowFocusChanged(hasFocus: Boolean) {
        super.onWindowFocusChanged(hasFocus)
        displayPolicyWindowFocused = hasFocus
        if (hasFocus) {
            noteDisplayUserActivity()
        } else {
            syncDisplayPolicy()
        }
    }

    override fun onTopResumedActivityChanged(isTopResumedActivity: Boolean) {
        super.onTopResumedActivityChanged(isTopResumedActivity)
        displayPolicyTopResumed = isTopResumedActivity
        if (isTopResumedActivity) {
            noteDisplayUserActivity()
        } else {
            syncDisplayPolicy()
        }
    }

    override fun dispatchTouchEvent(event: MotionEvent): Boolean {
        if (event.actionMasked == MotionEvent.ACTION_DOWN) {
            noteDisplayUserActivity()
        }
        return super.dispatchTouchEvent(event)
    }

    override fun dispatchKeyEvent(event: AndroidKeyEvent): Boolean {
        if (event.action == AndroidKeyEvent.ACTION_DOWN) {
            noteDisplayUserActivity()
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
        if (AndroidGpsPower.isGpsPaused(this)) {
            AndroidGpsSource.publishStatus(AndroidGpsSource.pausedStatus())
            return
        }
        if (hasPreciseLocationPermission()) {
            startAerobagGpsService()
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

    private fun startAerobagGpsService() {
        AerobagGpsService.startHighPrecisionGps(this)
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
internal fun DisclaimerConsentModal(
    state: UiDisclaimerState,
    onAccept: () -> Unit,
) {
    val uiTheme = LocalAerobagUiTheme.current
    val scrollState = rememberScrollState()
    Box(
        modifier = Modifier
            .fillMaxSize()
            .zIndex(OverlayPlaneModal)
            .background(Color.Black.copy(alpha = 0.74f))
            .clickable(
                indication = null,
                interactionSource = remember { MutableInteractionSource() },
            ) {},
        contentAlignment = Alignment.Center,
    ) {
        Column(
            modifier = Modifier
                .fillMaxWidth(0.90f)
                .widthIn(max = ThumbSize * 8.5f)
                .heightIn(max = ThumbSize * 7.2f)
                .clip(RoundedCornerShape(18.dp))
                .background(Color(0xFFFFFAF1))
                .border(3.dp, Color.White, RoundedCornerShape(18.dp))
                .clickable(
                    indication = null,
                    interactionSource = remember { MutableInteractionSource() },
                ) {}
                .padding(ThumbSize * 0.34f),
            verticalArrangement = Arrangement.spacedBy(ThumbSize * 0.22f),
        ) {
            Text(
                text = "BEFORE YOU USE AEROBAG",
                color = Color(0xFF8D1F16),
                fontSize = 27.sp,
                fontWeight = FontWeight.Black,
                lineHeight = 29.sp,
            )
            Text(
                text = state.text,
                modifier = Modifier
                    .weight(1f, fill = false)
                    .verticalScroll(scrollState),
                color = Color(0xFF111111),
                fontSize = 21.sp,
                lineHeight = 27.sp,
                fontWeight = FontWeight.SemiBold,
            )
            Button(
                onClick = onAccept,
                modifier = Modifier
                    .fillMaxWidth()
                    .height(ThumbSize * 0.95f)
                    .testTag("parity:disclaimer-accept-button"),
                colors = ButtonDefaults.buttonColors(
                    containerColor = uiTheme.controls.buttonChecked,
                    contentColor = uiTheme.controls.buttonFg,
                ),
            ) {
                Text(
                    text = state.acceptLabel.uppercase(),
                    fontSize = 18.sp,
                    fontWeight = FontWeight.Black,
                    textAlign = TextAlign.Center,
                )
            }
        }
    }
}

internal data class UiInvalidationRevisions(
    val navData: Int = 0,
    val sessionSnapshot: Int = 0,
    val rasterTiles: Int = 0,
    val mapOverlay: Int = 0,
    val nexradOverlay: Int = 0,
    val terrainOverlay: Int = 0,
    val flightPlanRoute: Int = 0,
    val debugPanel: Int = 0,
) {
    fun bumped(invalidations: List<String>): UiInvalidationRevisions =
        invalidations.fold(this) { revisions, invalidation ->
            when (invalidation) {
                "nav_data" -> revisions.copy(navData = revisions.navData + 1)
                "session_snapshot" -> revisions.copy(sessionSnapshot = revisions.sessionSnapshot + 1)
                "raster_tiles" -> revisions.copy(rasterTiles = revisions.rasterTiles + 1)
                "map_overlay" -> revisions.copy(mapOverlay = revisions.mapOverlay + 1)
                "nexrad_overlay" -> revisions.copy(nexradOverlay = revisions.nexradOverlay + 1)
                "terrain_overlay" -> revisions.copy(terrainOverlay = revisions.terrainOverlay + 1)
                "flight_plan_route" -> revisions.copy(flightPlanRoute = revisions.flightPlanRoute + 1)
                "debug_panel" -> revisions.copy(debugPanel = revisions.debugPanel + 1)
                else -> revisions
            }
        }
}

@Composable
private fun FlightPlanOverlayHost(
    controller: FlightPlanOverlayController,
    uiSession: NativeUiSession,
    onSessionSnapshot: (UiSessionSnapshot) -> Unit,
) {
    val airportInfoScope = rememberCoroutineScope()
    val presentation = controller.state.present()
    val weatherDetail = presentation.weatherDetail
    val airportInfo = presentation.airportInfo
    if (weatherDetail != null || airportInfo != null) {
        Box(
            modifier = Modifier
                .fillMaxSize()
                .zIndex(OverlayPlaneModalScrim),
        ) {
            Scrim {
                controller.dispatch(FlightPlanOverlayAction.Dismiss)
            }
            when {
                weatherDetail != null ->
                    WeatherDetailModal(
                        detail = weatherDetail,
                        modifier = Modifier
                            .align(Alignment.Center)
                            .zIndex(OverlayPlaneModal),
                    )
                airportInfo?.detail != null ->
                    AirportInfoModal(
                        detail = airportInfo.detail,
                        onTimeDisplayAction = { actionId ->
                            val airportId = airportInfo.airportId
                            airportInfoScope.launch {
                                runCatching {
                                    withContext(Dispatchers.Default) {
                                        uiSession.performTimeDisplayAction(actionId)
                                    }
                                }.onSuccess(onSessionSnapshot)
                                    .onFailure { return@launch }
                                runCatching {
                                    withContext(Dispatchers.IO) {
                                        uiSession.airportInfo(airportId)
                                    }
                                }.onSuccess { detail ->
                                    controller.dispatch(
                                        FlightPlanOverlayAction.ResolveAirportInfo(
                                            airportId,
                                            detail,
                                        ),
                                    )
                                }
                            }
                        },
                        modifier = Modifier
                            .align(Alignment.Center)
                            .zIndex(OverlayPlaneModal),
                    )
                airportInfo != null ->
                    MapSelectionDetailModal(
                        title = airportInfo.airportId,
                        text = airportInfo.error?.let { "Airport info unavailable: $it" }
                            ?: "Loading airport info...",
                        modifier = Modifier
                            .align(Alignment.Center)
                            .zIndex(OverlayPlaneModal),
                    )
            }
        }
    }
}

@Composable
internal fun AerobagApp(
    retainedModel: AerobagRetainedModel,
    perfScenario: AndroidPerfScenario? = null,
    armLayerNavKvFault: Boolean = false,
) {
    val context = LocalContext.current
    val appContext = context.applicationContext
    val prefs = remember(context) { context.applicationContext.getSharedPreferences(UiPrefsName, Context.MODE_PRIVATE) }
    var runtimeReloadToken by remember { mutableStateOf(0) }
    var runtimeFixture by remember { mutableStateOf<Result<RuntimeContent>?>(retainedModel.runtimeResult) }
    fun requestRuntimeReload(targetPage: AppPage? = null) {
        if (targetPage != null) {
            retainedModel.page = targetPage
        }
        runtimeFixture = null
        retainedModel.resetRuntime()
        runtimeReloadToken += 1
    }
    val offlinePackagesControllerHandle = remember(prefs) { initialOfflinePackagesControllerHandle(prefs) }
    DisposableEffect(offlinePackagesControllerHandle) {
        onDispose { NativeBindings.destroyOfflinePackagesController(offlinePackagesControllerHandle) }
    }
    val uiTheme = remember(context) { UiThemeLoader.load(context.applicationContext) }
    LaunchedEffect(context, runtimeReloadToken) {
        retainedModel.runtimeResult?.let {
            runtimeFixture = it
            return@LaunchedEffect
        }
        runtimeFixture = null
        val loaded = withContext(Dispatchers.IO) {
            runCatching {
                AndroidRuntimeContent.loadInstalledRuntime(
                    context.applicationContext,
                    readOfflinePackagesLibraryCacheJson(prefs),
                )
            }
        }
        retainedModel.runtimeResult = loaded
        runtimeFixture = loaded
    }
    var runtimeFailureMessage by remember { mutableStateOf<String?>(null) }
    @Composable
    fun RenderOfflinePackagesWithoutRuntime() {
        HomePage(
            page = AppPage.OfflinePackages,
            pageHistory = emptyList(),
            uptimeLabel = rememberUptimeLabel(SystemClock.elapsedRealtime()),
            debugState = defaultUiDebugState(gpsCapture = readStoredGpsCaptureDebugFlag(prefs)),
            navElement = null,
            onSelectPage = { targetPage -> requestRuntimeReload(targetPage) },
            onOpenPlan = { requestRuntimeReload(AppPage.Plan) },
            onOpenRecentChartOrPlate = { requestRuntimeReload(AppPage.Map) },
            offlinePackagesControllerHandle = offlinePackagesControllerHandle,
        )
    }
    LaunchedEffect(runtimeFixture) {
        when {
            runtimeFixture?.isFailure == true -> {
                val error = runtimeFixture?.exceptionOrNull() ?: return@LaunchedEffect
                val message = summarizeRuntimeLoadFailure(error)
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
            if (runtimeFailureMessage != null) {
                RenderOfflinePackagesWithoutRuntime()
            } else {
                Box(
                    modifier = Modifier
                        .fillMaxSize()
                        .background(uiTheme.controls.chartSurfaceBg),
                )
            }
        }
        return
    }
    if (runtimeFixture!!.isFailure) {
        CompositionLocalProvider(LocalAerobagUiTheme provides uiTheme) {
            RenderOfflinePackagesWithoutRuntime()
        }
        return
    }
    val fixture = runtimeFixture!!.getOrThrow()
    val sessionStartElapsedMs = remember { SystemClock.elapsedRealtime() }
    val uptimeLabel = rememberUptimeLabel(sessionStartElapsedMs)
    val storedRecentAirportIds = remember { readRecentAirportIds(context.applicationContext) }
    val storedSelectedAirportId = remember { prefs.getString(UiPrefsSelectedAirportKey, null).orEmpty() }
    val storedSelectedChartId = remember { prefs.getString(UiPrefsSelectedChartKey, null).orEmpty() }
    val retainedCoreSession = retainedModel.getOrCreateCoreSession(
        context = appContext,
        runtimeContent = fixture,
        recentAirportIds = storedRecentAirportIds,
        selectedAirportId = storedSelectedAirportId.ifBlank { null },
        selectedChartId = storedSelectedChartId.ifBlank { null },
    )
    val appCore = retainedCoreSession.appCore
    val situationRingCandidates = retainedCoreSession.situationRingCandidates
    val uiSession = retainedCoreSession.uiSession
    var page by remember {
        mutableStateOf(
            retainedModel.page ?: readStoredPage(prefs),
        )
    }
    var mapOrientationMode by remember { mutableStateOf(readStoredMapOrientationMode(prefs)) }
    LaunchedEffect(perfScenario?.id) {
        if (perfScenario != null) {
            Log.i(AndroidPerfScenarioTag, "startup scenario=${perfScenario.id}")
            page = AppPage.Map
        }
    }
    var pageHistory by remember { mutableStateOf(retainedModel.pageHistory) }
    val initialRasterMapState = remember(uiSession) {
        requireNotNull(uiSession.snapshot.rasterMap) {
            "core session did not provide raster map state"
        }
    }
    var rasterMapState by remember(uiSession) { mutableStateOf(initialRasterMapState) }
    var selectedMapId by remember(uiSession) { mutableStateOf(initialRasterMapState.selectedMapId) }
    var sessionSnapshot by remember(uiSession) { mutableStateOf(uiSession.snapshot) }
    val flightPlanOverlayController = remember(uiSession) { FlightPlanOverlayController() }
    var nextSessionCommandNoticeId by remember(uiSession) { mutableLongStateOf(1L) }
    var sessionCommandNotice by remember(uiSession) { mutableStateOf<SessionCommandNotice?>(null) }
    fun applySessionSnapshot(nextSnapshot: UiSessionSnapshot): Boolean {
        if (nextSnapshot.sessionRevision < sessionSnapshot.sessionRevision) {
            Log.i(
                "AerobagSession",
                "ignored stale snapshot revision=${nextSnapshot.sessionRevision} current=${sessionSnapshot.sessionRevision}",
            )
            return false
        }
        sessionSnapshot = nextSnapshot
        return true
    }
    fun recoverSessionCommandFailure(error: Throwable, notifyUser: Boolean = true) {
        if (error is CancellationException) {
            throw error
        }
        if (error !is NativeSessionCommandRejectedException) {
            throw error
        }
        applySessionSnapshot(error.refreshedSnapshot)
        Log.w(
            "AerobagSessionCommand",
            "recovered rejected session command command=${error.commandName}",
            error,
        )
        if (notifyUser) {
            sessionCommandNotice = SessionCommandNotice(
                id = nextSessionCommandNoticeId++,
                message = "Action failed; app state was refreshed.",
            )
        }
    }
    fun applySessionCommand(
        commandName: String,
        notifyUser: Boolean = true,
        operation: () -> UiSessionSnapshot,
    ): UiSessionSnapshot? {
        return try {
            val snapshot = operation()
            snapshot.takeIf { applySessionSnapshot(it) }
        } catch (error: Throwable) {
            recoverSessionCommandFailure(error, notifyUser = notifyUser)
            null
        }
    }
    fun applyBackgroundSessionCommand(
        commandName: String,
        logTag: String,
        operation: () -> UiSessionSnapshot,
    ): Boolean {
        return try {
            val snapshot = operation()
            applySessionSnapshot(snapshot)
            true
        } catch (error: Throwable) {
            if (error is NativeSessionCommandRejectedException) {
                recoverSessionCommandFailure(error, notifyUser = false)
            } else if (error is CancellationException) {
                throw error
            } else {
                Log.w(logTag, "background session command failed command=$commandName", error)
            }
            false
        }
    }
    suspend fun recordOfflinePackagePreferencesForCloud(
        preferencesJson: String,
        nowEpochMs: Long,
    ) {
        val failure = withContext(Dispatchers.Default) {
            try {
                uiSession.recordOfflinePackagePreferences(preferencesJson, nowEpochMs)
                null
            } catch (error: Throwable) {
                error
            }
        }
        failure?.let(::recoverSessionCommandFailure)
    }
    val mainExecutor = remember(appContext) { ContextCompat.getMainExecutor(appContext) }
    val sessionSnapshotRefreshRunner = retainedCoreSession.sessionSnapshotRefreshRunner
    DisposableEffect(sessionSnapshotRefreshRunner) {
        sessionSnapshotRefreshRunner.setListeners(
            onSnapshot = null,
            onFailure = { error ->
                Log.w("AerobagInvalidation", "session snapshot refresh failed", error)
            },
        )
        onDispose { sessionSnapshotRefreshRunner.setListeners(null, null) }
    }
    var uiInvalidationRevisions by remember(uiSession) { mutableStateOf(UiInvalidationRevisions()) }
    fun publishUiInvalidations(invalidations: List<String>) {
        if (invalidations.isEmpty()) return
        uiInvalidationRevisions = uiInvalidationRevisions.bumped(invalidations)
        if ("session_snapshot" in invalidations) {
            sessionSnapshotRefreshRunner.request(
                priority = if ("flight_plan_route" in invalidations) {
                    SessionSnapshotRefreshPriority.Timely
                } else {
                    SessionSnapshotRefreshPriority.LowPriority
                },
                reason = "invalidation",
            )
        }
    }
    fun enqueueUiInvalidations(invalidations: List<String>) {
        if (invalidations.isEmpty()) return
        mainExecutor.execute { publishUiInvalidations(invalidations) }
    }
    DisposableEffect(uiSession) {
        val snapshotDelivery = LatestValueExecutor(mainExecutor, ::applySessionSnapshot)
        val snapshotSubscription = uiSession.subscribeSnapshots(snapshotDelivery::submit)
        val invalidationSubscription = uiSession.subscribeInvalidations(::enqueueUiInvalidations)
        onDispose {
            snapshotSubscription.close()
            invalidationSubscription.close()
            snapshotDelivery.close()
        }
    }
    LaunchedEffect(context, sessionSnapshot.displayPolicy) {
        (context as? MainActivity)?.applyCoreDisplayPolicy(sessionSnapshot.displayPolicy)
    }
    DisposableEffect(context) {
        onDispose {
            (context as? MainActivity)?.applyCoreDisplayPolicy(null)
        }
    }
    fun selectOwnshipSource(sourceId: String) {
        applySessionCommand("selectOwnshipSource") {
            uiSession.selectOwnshipSource(OwnshipSelection.Source(sourceId))
        } ?: return
        AndroidGpsPower.clearPendingOwnshipSource(appContext)
        if (AndroidGpsPower.shouldRunHighPrecisionGpsForSource(sourceId)) {
            AerobagGpsService.startHighPrecisionGps(appContext)
        } else {
            AerobagGpsService.pauseForOwnshipSelection(appContext)
        }
    }
    LaunchedEffect(uiSession, sessionSnapshot.nextCycleProductFreshnessCheckEpochMs) {
        val nextCheckEpochMs = sessionSnapshot.nextCycleProductFreshnessCheckEpochMs ?: return@LaunchedEffect
        val delayMs = (nextCheckEpochMs - System.currentTimeMillis())
            .coerceAtLeast(0L)
        delay(delayMs)
        applySessionCommand("refreshSnapshot", notifyUser = false) {
            uiSession.refreshSnapshot()
        }
    }
    LaunchedEffect(uiSession, sessionSnapshot.appUiState.ownship.controls.nextRefreshEpochMs) {
        val deadlineEpochMs = sessionSnapshot.appUiState.ownship.controls.nextRefreshEpochMs
            ?: return@LaunchedEffect
        delay((deadlineEpochMs - System.currentTimeMillis()).coerceAtLeast(0L))
        applySessionCommand("refreshOwnshipSource", notifyUser = false) {
            uiSession.refreshSnapshot()
        }
    }
    val liveFeedRuntime = retainedCoreSession.liveFeedRuntime
    var liveFeedGeneration by remember(liveFeedRuntime) { mutableIntStateOf(0) }
    DisposableEffect(liveFeedRuntime) {
        val subscription = liveFeedRuntime.subscribeGeneration { generation ->
            liveFeedGeneration = generation
        }
        onDispose(subscription::close)
    }
    DisposableEffect(uiSession, context) {
        val activity = context as? MainActivity
        activity?.onSituationControlInput = { input ->
            applySessionCommand("applySituationControlInput") {
                uiSession.applySituationControlInput(input, System.currentTimeMillis().toDouble())
            } != null
        }
        onDispose {
            if (activity?.onSituationControlInput != null) {
                activity.onSituationControlInput = null
            }
        }
    }
    val appUiState = sessionSnapshot.appUiState
    val sessionPlanUiState = requireNotNull(appUiState.activePlan) {
        "UiSessionSnapshot must provide active flight-plan UI state"
    }
    var derivedChartPageState by remember(uiSession) {
        mutableStateOf(
            DerivedChartPageState(
                airports = emptyList<ChartAirport>(),
                referenceFamilies = emptyList(),
                airportMenuEntries = emptyList<ChartAirportMenuEntry>(),
                recentAirportIds = sessionSnapshot.chartPageState.recentAirportIds,
                selectedAirportId = sessionSnapshot.chartPageState.selectedAirportId,
                selectedReferenceFamilyId = sessionSnapshot.chartPageState.selectedReferenceFamilyId,
                selectedChartId = sessionSnapshot.chartPageState.selectedChartId,
                suggestedChartIds = sessionSnapshot.chartPageState.suggestedChartIds,
                procedureGeometryStatus = UiDataStatusState(
                    boxes = emptyList(),
                    launcherCount = null,
                    launcherSeverity = UiStatusSeverity.Ok,
                ),
            ),
        )
    }
    val selectedMap = rasterMapState
    var mapViewport by remember {
        mutableStateOf(
            retainedModel.mapViewport
                ?: createInitialViewport(
                    initialRasterMapState.initialViewport,
                    initialRasterMapState.minZoom,
                    initialRasterMapState.maxZoom,
                ),
        )
    }
    var chartViewport by remember { mutableStateOf<org.aerobag.app.domain.ImageViewportState?>(null) }
    var chartAssetDataRevision by remember { mutableIntStateOf(0) }
    suspend fun replaceInstalledPackageArtifacts(
        libraryCacheJson: String,
        plannedGcFilenames: Set<String>,
    ): Set<String> {
        val result = fixture.replaceInstalledArtifacts(
            appContext,
            libraryCacheJson,
            uiSession,
            plannedGcFilenames,
        )
        withContext(Dispatchers.Main) {
            applySessionSnapshot(result.snapshot)
            chartAssetDataRevision = chartAssetDataRevision + 1
        }
        return result.retainedArtifactFilenames
    }
    suspend fun maintainNavDb(nowEpochMs: Long) {
        val maintenance = withContext(Dispatchers.IO) {
            uiSession.maintainNavDb(nowEpochMs)
        }
        withContext(Dispatchers.Main) {
            applySessionSnapshot(maintenance.snapshot)
        }
        if (!maintenance.shouldAttemptAdvance) {
            return
        }
        val libraryCacheJson = readOfflinePackagesLibraryCacheJson(prefs)
        check(libraryCacheJson.isNotBlank()) {
            "core requested NAVDB advance without a cached package catalog"
        }
        replaceInstalledPackageArtifacts(libraryCacheJson, emptySet())
    }
    LaunchedEffect(uiSession, sessionSnapshot.nextNavDbMaintenanceEpochMs) {
        val nextCheckEpochMs = sessionSnapshot.nextNavDbMaintenanceEpochMs
            ?: return@LaunchedEffect
        delay((nextCheckEpochMs - System.currentTimeMillis()).coerceAtLeast(0L))
        while (true) {
            try {
                maintainNavDb(System.currentTimeMillis())
                val nextDeadline = uiSession.snapshot.nextNavDbMaintenanceEpochMs
                if (nextDeadline == null || nextDeadline > System.currentTimeMillis()) {
                    return@LaunchedEffect
                }
                delay(60_000)
            } catch (error: CancellationException) {
                throw error
            } catch (error: Throwable) {
                Log.w("AerobagNavDb", "scheduled NAVDB maintenance failed; retrying", error)
                delay(60_000)
            }
        }
    }
    var chartFolderOpen by remember { mutableStateOf(false) }
    var pageTilePaintTiming by remember { mutableStateOf<PageTilePaintTiming?>(null) }
    var nextPageTilePaintTimingId by remember { mutableStateOf(1L) }
    var debugLayerNavKvFaultsRemaining by remember(armLayerNavKvFault) {
        mutableIntStateOf(if (armLayerNavKvFault) 2 else 0)
    }
    val decodedTileBitmapCache = retainedCoreSession.decodedTileBitmapCache
    var playbackSourcePath by remember { mutableStateOf(DefaultPlaybackTracePath) }
    LaunchedEffect(page) {
        if (page != AppPage.Plan) {
            flightPlanOverlayController.dispatch(FlightPlanOverlayAction.Dismiss)
        }
    }
    val planListState = rememberLazyListState()
    val chartAirportById = remember(derivedChartPageState.airports) { derivedChartPageState.airports.associateBy { it.id } }
    val orderedChartAirports = remember(derivedChartPageState.airports) { derivedChartPageState.airports }
    val recentAirportIds = derivedChartPageState.recentAirportIds
    val selectedAirportId = derivedChartPageState.selectedAirportId
    val selectedReferenceFamilyId = derivedChartPageState.selectedReferenceFamilyId
    val selectedChartId = derivedChartPageState.selectedChartId
    val selectedAirport = remember(selectedAirportId, orderedChartAirports) {
        orderedChartAirports.find { it.id == selectedAirportId } ?: orderedChartAirports.firstOrNull()
    }
    val selectedReferenceFamily = remember(selectedReferenceFamilyId, derivedChartPageState.referenceFamilies) {
        derivedChartPageState.referenceFamilies.find { it.id == selectedReferenceFamilyId }
    }
    val selectedChartCollection = selectedReferenceFamily ?: selectedAirport
    val selectedChart = remember(selectedChartCollection, selectedChartId) {
        selectedChartCollection?.charts?.find { it.id == selectedChartId } ?: selectedChartCollection?.charts?.firstOrNull()
    }

    LaunchedEffect(page, selectedAirportId, selectedChartId, recentAirportIds) {
        retainedModel.page = page
        writeUiPrefs(context.applicationContext, page, selectedAirportId, selectedChartId, recentAirportIds)
    }
    LaunchedEffect(pageHistory) {
        retainedModel.pageHistory = pageHistory
    }
    LaunchedEffect(mapViewport) {
        retainedModel.mapViewport = mapViewport
    }
    LaunchedEffect(
        uiSession,
        sessionPlanUiState.planId,
        sessionPlanUiState.planVersion,
        sessionSnapshot.chartPageState.recentAirportIds,
        sessionSnapshot.chartPageState.plateTargetAirportId,
        sessionSnapshot.chartPageState.selectedAirportId,
        sessionSnapshot.chartPageState.selectedReferenceFamilyId,
        sessionSnapshot.chartPageState.selectedChartId,
        sessionSnapshot.chartPageState.suggestedChartIds,
    ) {
        derivedChartPageState = uiSession.deriveChartPageState()
    }
    LaunchedEffect(uiSession) {
        if (readStoredGpsCaptureDebugFlag(prefs)) {
            applyBackgroundSessionCommand("setDebugFlag", "AerobagDebug") {
                uiSession.setDebugFlag(DebugFlagId.GpsCapture, true)
            }
        }
        applyBackgroundSessionCommand("registerOwnshipSource", "AerobagOwnship") {
            uiSession.registerOwnshipSource(AndroidGpsSource.registration())
        }
        applyBackgroundSessionCommand("updateOwnshipSourceStatus", "AerobagOwnship") {
            uiSession.updateOwnshipSourceStatus(AndroidGpsSource.status.value)
        }
        val startupOwnshipSource = AndroidGpsPower.consumePendingOwnshipSource(appContext)
            ?: AndroidGpsPower.batterySavingFallbackSourceId().takeIf { AndroidGpsPower.isGpsPaused(appContext) }
        if (startupOwnshipSource != null) {
            selectOwnshipSource(startupOwnshipSource)
        }
        launch {
            AndroidGpsSource.status.collect { status ->
                applyBackgroundSessionCommand("updateOwnshipSourceStatus", "AerobagOwnship") {
                    uiSession.updateOwnshipSourceStatus(status)
                }
            }
        }
        launch {
            AndroidGpsSource.samples.collect { sample ->
                applyBackgroundSessionCommand("pushSituationSample", "AerobagOwnship") {
                    uiSession.pushSituationSample(sample)
                }
            }
        }
        launch {
            AndroidGpsSource.sourceSelectionRequests.collect { sourceId ->
                selectOwnshipSource(sourceId)
            }
        }
    }
    LaunchedEffect(
        uiSession,
        sessionSnapshot.playbackUiState.status,
        sessionSnapshot.playbackUiState.tickIntervalMs,
    ) {
        val tickIntervalMs = sessionSnapshot.playbackUiState.tickIntervalMs.coerceIn(16, 1000).toLong()
        while (sessionSnapshot.playbackUiState.status == PlaybackStatus.Playing) {
            delay(tickIntervalMs)
            applyBackgroundSessionCommand("tickPlayback", "AerobagPlayback") {
                uiSession.tickPlayback(System.currentTimeMillis().toDouble())
            }
        }
    }
    val badAutopilotActive = appUiState.ownship.controls.sources.any { source ->
        source.sourceKind == OwnshipSourceKind.BadAutopilot && source.active
    }
    LaunchedEffect(uiSession, badAutopilotActive) {
        while (badAutopilotActive) {
            applyBackgroundSessionCommand("tickBadAutopilot", "AerobagOwnship") {
                uiSession.tickBadAutopilot(System.currentTimeMillis().toDouble())
            }
            delay(250)
        }
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
        plateTargetAirportId = sessionSnapshot.chartPageState.plateTargetAirportId,
        selectedAirportId = selectedAirportId,
        selectedReferenceFamilyId = selectedReferenceFamilyId,
        selectedChartId = selectedChartId,
        selectedChartLabel = selectedChart?.label.orEmpty(),
        suggestedChartIds = derivedChartPageState.suggestedChartIds,
        recentAirportIds = recentAirportIds,
        chartViewport = chartViewport,
        chartFolderOpen = chartFolderOpen,
    )

    fun applySnapshotLocally(snapshot: AppViewSnapshot, history: List<AppViewSnapshot>) {
        pageHistory = history
        page = snapshot.page
        mapViewport = snapshot.mapViewport
        chartViewport = snapshot.chartViewport
        chartFolderOpen = snapshot.chartFolderOpen
    }

    fun restoreSnapshot(snapshot: AppViewSnapshot, history: List<AppViewSnapshot>) {
        if (snapshot.plateTargetAirportId != null || snapshot.selectedAirportId.isNotBlank() || snapshot.selectedChartId.isNotBlank() || snapshot.recentAirportIds.isNotEmpty()) {
            applySessionCommand("restoreChartPageState") {
                uiSession.restoreChartPageState(
                    recentAirportIds = snapshot.recentAirportIds,
                    plateTargetAirportId = snapshot.plateTargetAirportId,
                    selectedAirportId = snapshot.selectedAirportId.ifBlank { null },
                    selectedReferenceFamilyId = snapshot.selectedReferenceFamilyId,
                    selectedChartId = snapshot.selectedChartId.ifBlank { null },
                    suggestedChartIds = snapshot.suggestedChartIds,
                )
            }
        }
        applySnapshotLocally(snapshot, history)
        val nextSnapshot =
            applySessionCommand(if (snapshot.selectedMapId.isBlank()) "refreshSnapshot" else "selectRasterMap") {
                if (snapshot.selectedMapId.isBlank()) {
                    uiSession.refreshSnapshot()
                } else {
                    uiSession.selectRasterMap(snapshot.selectedMapId)
                }
            }
        if (nextSnapshot != null) {
            val nextRasterMapState = requireNotNull(nextSnapshot.rasterMap) {
                "core session returned no raster map state"
            }
            rasterMapState = nextRasterMapState
            selectedMapId = nextRasterMapState.selectedMapId
        }
    }

    fun navigateToPage(nextPage: AppPage) {
        diagnosticLogInfo("AerobagNavigation") {
            "navigate request from=$page to=$nextPage history=${pageHistory.size}"
        }
        if (nextPage == page) {
            diagnosticLogInfo("AerobagNavigation") { "navigate ignored same-page page=$page" }
            return
        }
        if (nextPage == AppPage.Map) {
            pageTilePaintTiming = PageTilePaintTiming(
                id = nextPageTilePaintTimingId++,
                fromPage = page,
                startedMs = SystemClock.elapsedRealtime(),
                trigger = "page-to-map",
            )
            perfLogInfo(TileBudgetLogTag) { "tile-paint-start id=${pageTilePaintTiming?.id} trigger=${pageTilePaintTiming?.trigger} from=$page" }
        }
        pageHistory = boundedHistory(pageHistory + currentSnapshot())
        page = nextPage
        diagnosticLogInfo("AerobagNavigation") {
            "navigate committed page=$page history=${pageHistory.size}"
        }
    }

    fun pushViewSnapshot(snapshot: AppViewSnapshot, restoreCore: Boolean = false) {
        val history = boundedHistory(pageHistory + currentSnapshot())
        if (restoreCore) {
            restoreSnapshot(snapshot, history)
        } else {
            applySnapshotLocally(snapshot, history)
        }
    }

    fun navigateToMostRecentChartOrPlate() {
        val target =
            pageHistory
                .asReversed()
                .firstOrNull { it.page == AppPage.Map || it.page == AppPage.Charts }
        if (target != null) {
            pushViewSnapshot(target, restoreCore = true)
        } else {
            navigateToPage(AppPage.Map)
        }
    }

    fun openChartsForAirport(airportId: String, chartId: String? = null) {
        val airport = chartAirportById[airportId]
        val selectedChart = chartId
            ?.let { requestedChartId -> airport?.charts?.find { it.id == requestedChartId } }
            ?: airport?.charts?.firstOrNull()
        val nextRecentAirportIds = moveAirportToFront(
            sessionSnapshot.chartPageState.recentAirportIds,
            airportId,
            derivedChartPageState.airports,
        )
        applySessionCommand("restoreChartPageState") {
            uiSession.restoreChartPageState(
                recentAirportIds = nextRecentAirportIds,
                plateTargetAirportId = airportId,
                selectedAirportId = airportId,
                selectedReferenceFamilyId = null,
                selectedChartId = selectedChart?.id,
                suggestedChartIds = emptyList(),
            )
        } ?: return
        applySnapshotLocally(
            currentSnapshot().copy(
                page = AppPage.Charts,
                plateTargetAirportId = airportId,
                selectedAirportId = airportId,
                selectedReferenceFamilyId = null,
                selectedChartId = selectedChart?.id.orEmpty(),
                selectedChartLabel = selectedChart?.label.orEmpty(),
                recentAirportIds = nextRecentAirportIds,
                suggestedChartIds = emptyList(),
                chartViewport = null,
                chartFolderOpen = chartId == null,
            ),
            boundedHistory(pageHistory + currentSnapshot()),
        )
    }

    fun openPlateTarget(airportId: String, target: String, chartId: String) {
        val nextRecentAirportIds = moveAirportToFront(
            sessionSnapshot.chartPageState.recentAirportIds,
            airportId,
            derivedChartPageState.airports,
        )
        applySessionCommand("restoreChartPageState") {
            uiSession.restoreChartPageState(
                recentAirportIds = nextRecentAirportIds,
                plateTargetAirportId = airportId,
                selectedAirportId = airportId,
                selectedReferenceFamilyId = null,
                selectedChartId = chartId,
                suggestedChartIds = emptyList(),
            )
        } ?: return
        applySnapshotLocally(
            currentSnapshot().copy(
                page = AppPage.Charts,
                plateTargetAirportId = airportId,
                selectedAirportId = airportId,
                selectedReferenceFamilyId = null,
                selectedChartId = chartId,
                selectedChartLabel = "",
                recentAirportIds = nextRecentAirportIds,
                suggestedChartIds = emptyList(),
                chartViewport = null,
                chartFolderOpen = target == "Folder",
            ),
            boundedHistory(pageHistory + currentSnapshot()),
        )
    }

    BackHandler(enabled = pageHistory.isNotEmpty()) {
        val previous = pageHistory.lastOrNull() ?: return@BackHandler
        restoreSnapshot(previous, pageHistory.dropLast(1))
    }
    BackHandler(enabled = sessionSnapshot.disclaimerState.required) {
        // Agreement is mandatory; do not let system back bypass it.
    }

    CompositionLocalProvider(LocalAerobagUiTheme provides uiTheme) {
        CloudEffectPump(
            uiSession = uiSession,
            onSnapshot = ::applySessionSnapshot,
        )
        BoxWithConstraints(modifier = Modifier.fillMaxSize()) {
            val bottomCornerControlsRaised = shouldRaiseBottomCornerControls(maxWidth)
            when (page) {
                AppPage.Map -> {
                    key(sessionSnapshot.navDataEpoch) {
                        MapExplorerPage(
                        appCore = appCore,
                        page = page,
                        pageHistory = pageHistory,
                        uptimeLabel = uptimeLabel,
                        uiSession = uiSession,
                        sessionSnapshot = sessionSnapshot,
                        uiInvalidationRevisions = uiInvalidationRevisions,
                        liveFeedGeneration = liveFeedGeneration,
                        uiTheme = uiTheme,
                        ownship = appUiState.ownship.render,
                        flightDataBanner = appUiState.flightDataBanner,
                        playbackUiState = sessionSnapshot.playbackUiState,
                        playbackPanelState = sessionSnapshot.playbackPanelState,
                        playbackSourcePath = playbackSourcePath,
                        mapFollowUiState = sessionSnapshot.mapFollowUiState,
                        mapFollowTargetViewport = sessionSnapshot.mapFollowTargetViewport,
                        situationRingCandidates = situationRingCandidates,
                        selectedMap = selectedMap,
                        mapFamilyOptions = rasterMapState.familyOptions,
                        viewport = mapViewport,
                        mapOrientationMode = mapOrientationMode,
                        decodedTileBitmapCache = decodedTileBitmapCache,
                        debugState = sessionSnapshot.debugState,
                        perfScenario = perfScenario,
                        pageTilePaintTiming = pageTilePaintTiming,
                        ownshipControls = appUiState.ownship.controls,
                        onPageTilePaintTimingComplete = { completedId ->
                            if (pageTilePaintTiming?.id == completedId) {
                                pageTilePaintTiming = null
                            }
                        },
                        onViewportChange = { mapViewport = it },
                        onMapOrientationModeChange = { mode ->
                            mapOrientationMode = mode
                            writeStoredMapOrientationMode(prefs, mode)
                        },
                        onSessionSnapshotChange = { applySessionSnapshot(it) },
                        onSessionCommandFailure = { recoverSessionCommandFailure(it) },
                        onBeforeMapLayerCommand = {
                            if (debugLayerNavKvFaultsRemaining > 0) {
                                fixture.navKvStore.debugDropAttachedSessionPages()
                                debugLayerNavKvFaultsRemaining -= 1
                            }
                        },
                        onReloadApplication = { requestRuntimeReload(AppPage.Map) },
                        onSelectOwnshipSource = ::selectOwnshipSource,
                        onSituationControlInput = { input ->
                            applySessionCommand("applySituationControlInput") {
                                uiSession.applySituationControlInput(input, System.currentTimeMillis().toDouble())
                            }
                        },
                        onPlaybackSourcePathChange = { playbackSourcePath = it },
                        onSelectMapFamily = {
                            val timingId = nextPageTilePaintTimingId++
                            val clickStartMs = SystemClock.elapsedRealtime()
                            pageTilePaintTiming = PageTilePaintTiming(
                                id = timingId,
                                fromPage = page,
                                startedMs = SystemClock.elapsedRealtime(),
                                trigger = "map-family:$it",
                            )
                            perfLogInfo(TileBudgetLogTag) { "map-family-click id=$timingId family=$it" }
                            pageHistory = boundedHistory(pageHistory + currentSnapshot())
                            page = AppPage.Map
                            val selectStartMs = SystemClock.elapsedRealtime()
                            val nextSnapshot = applySessionCommand("selectMapFamily") {
                                uiSession.selectMapFamily(it)
                            }
                            if (nextSnapshot != null) {
                                perfLogInfo(TileBudgetLogTag) {
                                    "map-family-select-core id=$timingId family=$it elapsedMs=${SystemClock.elapsedRealtime() - selectStartMs}"
                                }
                                val nextRasterMapState = requireNotNull(nextSnapshot.rasterMap) {
                                    "core selectMapFamily returned no raster map state"
                                }
                                rasterMapState = nextRasterMapState
                                selectedMapId = nextRasterMapState.selectedMapId
                                perfLogInfo(TileBudgetLogTag) {
                                    "map-family-click-done id=$timingId family=$it elapsedMs=${SystemClock.elapsedRealtime() - clickStartMs}"
                                }
                            }
                        },
                        onOpenChartReference = { familyId, suggestedChartIds ->
                            if (applySessionCommand("selectChartReference") {
                                    uiSession.selectChartReference(familyId, suggestedChartIds)
                                } != null
                            ) {
                                chartViewport = null
                                chartFolderOpen = true
                                navigateToPage(AppPage.Charts)
                            }
                        },
                        onSelectPage = ::navigateToPage,
                        onOpenPlateTarget = ::openPlateTarget,
                        onOpenPlan = { navigateToPage(AppPage.Plan) },
                        navElement = navElement,
                        planUiState = sessionPlanUiState,
                        )
                    }
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
                        uiTheme = uiTheme,
                        overlayState = flightPlanOverlayController.state,
                        onOverlayAction = flightPlanOverlayController::dispatch,
                        onSelectPage = ::navigateToPage,
                        onOpenRecentChartOrPlate = ::navigateToMostRecentChartOrPlate,
                        onOpenCharts = { airportId, chartId -> openChartsForAirport(airportId, chartId) },
                        onApplySessionSnapshot = { snapshot ->
                            applySessionSnapshot(snapshot)
                        },
                        onSessionCommandFailure = { recoverSessionCommandFailure(it) },
                    )
                }
                AppPage.AltitudePlanner -> {
                    AltitudePlannerPage(
                        page = page,
                        planner = sessionPlanUiState.altitudePlanner,
                        planVersion = sessionPlanUiState.planVersion,
                        uiSession = uiSession,
                        navElement = navElement,
                        mostRecentChartOrPlatePage = mostRecentChartOrPlatePageFromHistory(pageHistory),
                        onOpenPlan = { navigateToPage(AppPage.Plan) },
                        onOpenRecentChartOrPlate = ::navigateToMostRecentChartOrPlate,
                        onSelectPage = ::navigateToPage,
                        onApplySessionSnapshot = ::applySessionSnapshot,
                        onSessionCommandFailure = ::recoverSessionCommandFailure,
                    )
                }
                AppPage.Charts -> {
                    ChartsPage(
                        page = page,
                        pageHistory = pageHistory,
                        uptimeLabel = uptimeLabel,
                        airportMenuEntries = derivedChartPageState.airportMenuEntries,
                        selectedCollection = selectedChartCollection,
                        selectedChart = selectedChart,
                        suggestedChartIds = derivedChartPageState.suggestedChartIds,
                        chartAssetDataRevision = chartAssetDataRevision,
                        flightPlanRouteRevision = sessionSnapshot.flightPlanRouteRevision,
                        debugState = sessionSnapshot.debugState,
                        uiTheme = uiTheme,
                        ownship = appUiState.ownship.render,
                        ownshipControls = appUiState.ownship.controls,
                        dataStatusState = sessionSnapshot.dataStatusState,
                        procedureGeometryStatus = derivedChartPageState.procedureGeometryStatus,
                        flightDataBanner = appUiState.flightDataBanner,
                        uiSession = uiSession,
                        navElement = navElement,
                        folderOpen = chartFolderOpen,
                        viewport = chartViewport,
                        onViewportChange = { chartViewport = it },
                        onSessionSnapshotChange = { applySessionSnapshot(it) },
                        onSessionCommandFailure = { recoverSessionCommandFailure(it) },
                        onFolderOpenChange = {
                            applySnapshotLocally(
                                currentSnapshot().copy(
                                    page = AppPage.Charts,
                                    chartFolderOpen = it,
                                ),
                                boundedHistory(pageHistory + currentSnapshot()),
                            )
                        },
                        onSelectPage = ::navigateToPage,
                        onOpenPlan = { navigateToPage(AppPage.Plan) },
                        onStatusAction = { actionId ->
                            if (actionId == "app:reload") {
                                requestRuntimeReload(AppPage.Map)
                            } else {
                                applySessionCommand("performStatusAction") {
                                    uiSession.performStatusAction(actionId)
                                }
                            }
                        },
                        onSelectAirport = { airportId ->
                            if (applySessionCommand("selectAirport") { uiSession.selectAirport(airportId) } != null) {
                                val airport = chartAirportById[airportId]
                                applySnapshotLocally(
                                    currentSnapshot().copy(
                                        page = AppPage.Charts,
                                        selectedAirportId = airportId,
                                        selectedReferenceFamilyId = null,
                                        selectedChartId = airport?.charts?.firstOrNull()?.id.orEmpty(),
                                        selectedChartLabel = airport?.charts?.firstOrNull()?.label.orEmpty(),
                                        recentAirportIds = sessionSnapshot.chartPageState.recentAirportIds,
                                        suggestedChartIds = emptyList(),
                                        chartViewport = null,
                                        chartFolderOpen = false,
                                    ),
                                    boundedHistory(pageHistory + currentSnapshot()),
                                )
                            }
                        },
                        onSelectReference = { familyId ->
                            if (applySessionCommand("selectChartReference") {
                                    uiSession.selectChartReference(familyId, emptyList())
                                } != null
                            ) {
                                chartViewport = null
                                chartFolderOpen = true
                            }
                        },
                        onSelectChart = {
                            if (applySessionCommand("selectChart") { uiSession.selectChart(it) } != null) {
                                applySnapshotLocally(
                                    currentSnapshot().copy(
                                        page = AppPage.Charts,
                                        selectedReferenceFamilyId = selectedReferenceFamilyId,
                                        selectedChartId = it,
                                        selectedChartLabel = selectedChartCollection
                                            ?.charts
                                            ?.firstOrNull { chart -> chart.id == it }
                                            ?.label
                                            .orEmpty(),
                                        suggestedChartIds = derivedChartPageState.suggestedChartIds,
                                        chartViewport = null,
                                        chartFolderOpen = false,
                                    ),
                                    boundedHistory(pageHistory + currentSnapshot()),
                                )
                            }
                        },
                        onSelectOwnshipSource = ::selectOwnshipSource,
                    )
                }
                AppPage.Home -> {
                    diagnosticLogInfo("AerobagNavigation") { "render home history=${pageHistory.size}" }
                    HomePage(
                        page = page,
                        homePageState = sessionSnapshot.homePageState,
                        pageHistory = pageHistory,
                        mostRecentChartOrPlatePage = mostRecentChartOrPlatePageFromHistory(pageHistory),
                        uptimeLabel = uptimeLabel,
                        debugState = sessionSnapshot.debugState,
                        navElement = navElement,
                        onSelectPage = ::navigateToPage,
                        onOpenPlan = { navigateToPage(AppPage.Plan) },
                        onOpenRecentChartOrPlate = ::navigateToMostRecentChartOrPlate,
                        offlinePackagesControllerHandle = offlinePackagesControllerHandle,
                        synchronizedOfflinePackagePreferencesJson =
                            sessionSnapshot.offlinePackagePreferencesJson,
                        onOfflinePackagePreferencesForCloud =
                            ::recordOfflinePackagePreferencesForCloud,
                        onOfflinePackageLibraryCacheChanged = { cacheJson ->
                            applySessionCommand("loadOfflinePackageLibraryCache") {
                                uiSession.loadOfflinePackageLibraryCache(cacheJson)
                            }
                        },
                        onOfflinePackageArtifactsChanged = { libraryCacheJson, plannedGcFilenames ->
                            replaceInstalledPackageArtifacts(libraryCacheJson, plannedGcFilenames)
                        },
                    )
                }
                AppPage.OfflinePackages -> {
                    HomePage(
                        page = page,
                        pageHistory = pageHistory,
                        mostRecentChartOrPlatePage = mostRecentChartOrPlatePageFromHistory(pageHistory),
                        uptimeLabel = uptimeLabel,
                        debugState = sessionSnapshot.debugState,
                        navElement = navElement,
                        onSelectPage = ::navigateToPage,
                        onOpenPlan = { navigateToPage(AppPage.Plan) },
                        onOpenRecentChartOrPlate = ::navigateToMostRecentChartOrPlate,
                        offlinePackagesControllerHandle = offlinePackagesControllerHandle,
                        synchronizedOfflinePackagePreferencesJson =
                            sessionSnapshot.offlinePackagePreferencesJson,
                        onOfflinePackagePreferencesForCloud =
                            ::recordOfflinePackagePreferencesForCloud,
                        onOfflinePackageLibraryCacheChanged = { cacheJson ->
                            applySessionCommand("loadOfflinePackageLibraryCache") {
                                uiSession.loadOfflinePackageLibraryCache(cacheJson)
                            }
                        },
                        onOfflinePackageArtifactsChanged = { libraryCacheJson, plannedGcFilenames ->
                            replaceInstalledPackageArtifacts(libraryCacheJson, plannedGcFilenames)
                        },
                    )
                }
                AppPage.DataStatus -> {
                    DataStatusPage(
                        page = page,
                        state = sessionSnapshot.dataStatusPageState,
                        dataSourcesRow = dataSourcesStatusRow(appContext, prefs),
                        navElement = navElement,
                        mostRecentChartOrPlatePage = mostRecentChartOrPlatePageFromHistory(pageHistory),
                        onOpenPlan = { navigateToPage(AppPage.Plan) },
                        onOpenRecentChartOrPlate = ::navigateToMostRecentChartOrPlate,
                        onSelectPage = ::navigateToPage,
                        onTimeDisplayAction = { actionId ->
                            applySessionCommand("performTimeDisplayAction") {
                                uiSession.performTimeDisplayAction(actionId)
                            }
                        },
                    )
                }
                AppPage.Settings -> {
                    SettingsPage(
                        page = page,
                        state = sessionSnapshot.settingsPageState,
                        navElement = navElement,
                        mostRecentChartOrPlatePage = mostRecentChartOrPlatePageFromHistory(pageHistory),
                        onOpenPlan = { navigateToPage(AppPage.Plan) },
                        onOpenRecentChartOrPlate = ::navigateToMostRecentChartOrPlate,
                        onSelectPage = ::navigateToPage,
                        onSettingsAction = { actionId, valueId ->
                            val snapshot = applySessionCommand("performSettingsAction") {
                                uiSession.performSettingsAction(actionId, valueId)
                            }
                            if (snapshot != null) {
                                writeStoredGpsCaptureDebugFlag(prefs, snapshot.debugState.gpsCapture)
                            }
                        },
                    )
                }
                AppPage.Cloud -> {
                    CloudPage(
                        page = page,
                        state = sessionSnapshot.cloudPageState,
                        navElement = navElement,
                        mostRecentChartOrPlatePage = mostRecentChartOrPlatePageFromHistory(pageHistory),
                        onOpenPlan = { navigateToPage(AppPage.Plan) },
                        onOpenRecentChartOrPlate = ::navigateToMostRecentChartOrPlate,
                        onSelectPage = ::navigateToPage,
                        onAction = { actionId, fields ->
                            applySessionCommand("performCloudUiAction") {
                                uiSession.performCloudUiAction(
                                    actionId,
                                    fields,
                                    System.currentTimeMillis(),
                                )
                            } != null
                        },
                    )
                }
            }
            FlightPlanOverlayHost(
                flightPlanOverlayController,
                uiSession,
                ::applySessionSnapshot,
            )
            if (sessionSnapshot.disclaimerState.required) {
                DisclaimerConsentModal(
                    state = sessionSnapshot.disclaimerState,
                    onAccept = {
                        applySessionCommand("acceptDisclaimer") {
                            uiSession.acceptDisclaimer(sessionSnapshot.disclaimerState.agreementId)
                        }
                    },
                )
            }
            sessionCommandNotice?.let { notice ->
                LaunchedEffect(notice.id) {
                    delay(SessionCommandNoticeDurationMs)
                    if (sessionCommandNotice?.id == notice.id) {
                        sessionCommandNotice = null
                    }
                }
                SessionCommandNoticeBanner(
                    notice = notice,
                    uiTheme = uiTheme,
                    modifier = Modifier
                        .align(Alignment.TopCenter)
                        .padding(top = ThumbGap, start = ThumbGap, end = ThumbGap)
                        .zIndex(OverlayPlaneModal),
                )
            }
        }
    }
}

@Composable
private fun SessionCommandNoticeBanner(
    notice: SessionCommandNotice,
    uiTheme: UiTheme,
    modifier: Modifier = Modifier,
) {
    AnimatedVisibility(
        visible = true,
        enter = fadeIn() + slideInVertically { -it },
        exit = fadeOut() + slideOutVertically { -it },
        modifier = modifier,
    ) {
        Surface(
            shape = RoundedCornerShape(ThumbRadius * 0.65f),
            color = uiTheme.controls.dataStatusWarningBg,
            contentColor = uiTheme.controls.panelFg,
            border = BorderStroke(1.dp, uiTheme.controls.dataStatusWarningStroke),
            shadowElevation = 8.dp,
        ) {
            Text(
                text = notice.message,
                modifier = Modifier.padding(horizontal = ThumbSize * 0.28f, vertical = ThumbSize * 0.16f),
                style = MaterialTheme.typography.labelLarge,
                fontWeight = FontWeight.Bold,
                maxLines = 2,
                overflow = TextOverflow.Ellipsis,
                textAlign = TextAlign.Center,
            )
        }
    }
}
