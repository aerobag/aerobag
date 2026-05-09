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
internal fun OfflinePackagesErrorPanel(
    message: String,
    closeEnabled: Boolean,
    onClose: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val uiTheme = LocalAerobagUiTheme.current
    Surface(
        modifier = modifier
            .fillMaxSize()
            .testTag("parity:offline-library-panel"),
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
internal fun OfflinePackagesLibraryPanel(
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
                    testTag = "parity:offline-refresh-button",
                    onClick = if (refreshInFlight) onCancelRefresh else onRefresh,
                )
                CompactSquareButton(
                    label = "X",
                    modifier = Modifier.size(ThumbSize * 0.72f),
                    enabled = closeEnabled,
                    testTag = "parity:offline-close-button",
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
internal fun OfflinePackagesPanel(
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
        modifier = modifier
            .fillMaxSize()
            .testTag("parity:offline-packages-panel"),
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
                    testTag = "parity:offline-refresh-button",
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
                    testTag = "parity:offline-sync-button",
                    onClick = if (syncInFlight) onCancelOperation else onSync,
                )
                CompactSquareButton(
                    label = "X",
                    modifier = Modifier.size(ThumbSize * 0.72f),
                    enabled = closeEnabled,
                    testTag = "parity:offline-close-button",
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
                        testTagPrefix = "parity:offline-region",
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
                        testTagPrefix = "parity:offline-product",
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
internal fun OfflinePackageAllSection(
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
internal fun OfflinePackageSection(
    title: String,
    testTagPrefix: String,
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
                testTag = "$testTagPrefix:${option.id}",
                enabled = enabled,
                onClick = { onRowClick(option.id) },
            )
        }
    }
}

@Composable
internal fun OfflinePackageCoreSection(
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
internal fun OfflinePackageSelectionRow(
    label: String,
    row: OfflinePackagesUiRowWire,
    testTag: String,
    enabled: Boolean,
    onClick: () -> Unit,
) {
    OfflinePackagePlanRow(
        label = label,
        row = row,
        enabled = enabled,
        onCycleClick = onClick,
        testTag = testTag,
    )
}

@Composable
internal fun OfflinePackagePlanRow(
    label: String,
    row: OfflinePackagesUiRowWire,
    enabled: Boolean,
    onCycleClick: (() -> Unit)?,
    showSelectionIcon: Boolean = true,
    backgroundOverride: Color? = null,
    testTag: String? = null,
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
            .then(testTag?.let { Modifier.testTag(it) } ?: Modifier)
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
                    .then(testTag?.let { Modifier.testTag("$it:toggle") } ?: Modifier)
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
internal fun OfflinePackagePlanSummary(
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

internal fun offlinePackagePlanLines(
    entries: List<OfflinePackagesUiPlanEntryWire>,
): List<List<OfflinePackagesUiPlanEntryWire>> =
    if (entries.size <= 2) {
        entries.map { listOf(it) }
    } else {
        entries.chunked(2).take(2)
    }

@Composable
internal fun OfflinePackageSizeSummary(
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
internal fun OfflinePackageSelectionIcon(selection: OfflinePackageSelection, modifier: Modifier = Modifier) {
    val action = when (selection) {
        OfflinePackageSelection.Play -> OfflinePackagesUiPlanActionWire.Fetch
        OfflinePackageSelection.Pause -> OfflinePackagesUiPlanActionWire.Pause
        OfflinePackageSelection.Unselected -> OfflinePackagesUiPlanActionWire.Delete
    }
    OfflinePackagePlanActionIcon(action = action, modifier = modifier)
}

@Composable
internal fun OfflinePackagePlanActionIcon(
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

internal val OfflinePackageMagenta = Color(0xFFFF3DCE)
internal val OfflinePackageOrange = Color(0xFFFFA12B)
internal val OfflinePackageRed = Color(0xFFFF4D5E)

internal fun offlinePackagePlanActionColor(action: OfflinePackagesUiPlanActionWire): Color = when (action) {
    OfflinePackagesUiPlanActionWire.Delete -> OfflinePackageRed
    OfflinePackagesUiPlanActionWire.Keep -> Color.White
    OfflinePackagesUiPlanActionWire.Pause -> OfflinePackageOrange
    OfflinePackagesUiPlanActionWire.Fetch -> OfflinePackageMagenta
}

internal fun readPackageSourceBaseUrl(
    context: Context,
    prefs: android.content.SharedPreferences,
): String =
    prefs.getString(UiPrefsPackageSourceBaseUrlKey, null)
        ?.trim()
        ?.trimEnd('/')
        ?.takeIf { it.isNotBlank() }
        ?: loadAndroidPackageSourceBaseUrl(context)

internal fun initialOfflinePackagesControllerHandle(
    prefs: android.content.SharedPreferences,
): Long = NativeBindings.createOfflinePackagesController(readOfflinePackagesStateJson(prefs))

internal fun writePackageSourceBaseUrl(
    prefs: android.content.SharedPreferences,
    value: String,
) {
    prefs.edit()
        .putString(UiPrefsPackageSourceBaseUrlKey, value.trim())
        .apply()
}

internal fun writeOfflinePackagesStateJson(
    prefs: android.content.SharedPreferences,
    stateJson: String?,
) {
    prefs.edit()
        .putString(UiPrefsOfflinePackagePreferencesKey, stateJson)
        .apply()
}

internal fun listInstalledPackageArtifacts(context: Context): List<InstalledArtifactWire> {
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

internal suspend fun syncOfflinePackages(
    context: Context,
    plan: PackageManagementPlanWire,
    bundle: BundleManifestWire,
    packageSourceBaseUrl: String,
    packagedArtifactRoot: String,
    maxParallelFetches: Int,
    activeConnections: ActivePackageConnections,
    onProgress: suspend (String, OfflinePackagesSyncProgressWire?) -> Unit = { _, _ -> },
): OfflinePackagesSyncSummary {
    val syncStartMs = SystemClock.elapsedRealtime()
    val packagedArtifactRootUrl = resolvePackageSourceUrl(
        packagedArtifactRoot,
        resolvePublicationRootUrl(packageSourceBaseUrl),
    )
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
                            val sourceUrl = resolvePackageSourceUrl(pkg.relativePath, packagedArtifactRootUrl)
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

internal fun syncProgressText(
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

internal fun formatProgressMegabytes(bytes: Long): String = "${bytes / 1_000_000L}MB"

internal const val PackageHttpConnectTimeoutMs = 5_000
internal const val PackageHttpReadTimeoutMs = 5_000

internal class ActivePackageConnections {
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

internal suspend fun refreshOfflinePackageLibrary(
    packageSourceBaseUrl: String,
    activeConnections: ActivePackageConnections,
): OfflinePackagesControllerEventWire.LibraryRefreshSucceeded {
    val publicationRootUrl = resolvePublicationRootUrl(packageSourceBaseUrl)
    val discoveryUrls = buildList {
        add(resolvePackageSourceUrl(CurrentArtifactsDiscoveryFilename, publicationRootUrl))
    }.distinct()
    val discoveryJsons = discoveryUrls.map { discoveryUrl ->
        currentCoroutineContext().ensureActive()
        readPackageSourceText(
            discoveryUrl,
            activeConnections,
        )
    }
    val discoveryManifests = discoveryJsons
        .map { PackageManagementJson.decodeFromString<CurrentArtifactsManifestWire>(it) }
    val bundleRefsByFilename = discoveryManifests
        .flatMap { manifest ->
            val packagedRootUrl = packagedArtifactRootUrl(publicationRootUrl, manifest)
            manifest.bundles
                .filter { it.bundleType == "cycle" }
                .map { bundle -> bundle.filename to resolvePackageSourceUrl(bundle.relativePath ?: bundle.filename, packagedRootUrl) }
        }
        .distinctBy { it.first }
        .sortedBy { it.first }
    val bundleJsonsByFilename = bundleRefsByFilename.associate { (filename, bundleUrl) ->
        currentCoroutineContext().ensureActive()
        filename to readPackageSourceText(
            bundleUrl,
            activeConnections,
        )
    }
    return OfflinePackagesControllerEventWire.LibraryRefreshSucceeded(
        fetchedAtEpochMs = System.currentTimeMillis(),
        discoveryJsons = discoveryJsons,
        bundleJsonsByFilename = bundleJsonsByFilename,
    )
}

internal suspend fun readPackageSourceText(
    sourceUrl: String,
    activeConnections: ActivePackageConnections,
): String =
    readPackageSourceBytes(
        sourceUrl = sourceUrl,
        expectedSizeBytes = null,
        activeConnections = activeConnections,
        onBytesRead = {},
    ).decodeToString()

internal suspend fun readPackageSourceBytes(
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

internal fun openCancellablePackageConnection(sourceUrl: String): HttpURLConnection =
    (URL(sourceUrl).openConnection() as HttpURLConnection).apply {
        connectTimeout = PackageHttpConnectTimeoutMs
        readTimeout = PackageHttpReadTimeoutMs
        instanceFollowRedirects = true
        useCaches = false
    }

internal fun resolvePublicationRootUrl(configuredPackageSourceBaseUrl: String): String {
    val configured = configuredPackageSourceBaseUrl.trim().trimEnd('/')
    check(configured.isNotBlank()) { "package source URL is blank" }
    if (configured.endsWith("/$CurrentArtifactsDiscoveryFilename")) {
        return configured.substringBeforeLast("/")
    }
    if (configured.startsWith("http://") || configured.startsWith("https://")) {
        val hostOnly = configured.removePrefix("http://").removePrefix("https://").contains('/').not()
        return if (hostOnly) {
            resolvePackageSourceUrl(PublicationPackageRootPath, configured)
        } else {
            configured
        }
    }
    check(!configured.contains('/')) {
        "package source host must not contain a path without a scheme: $configuredPackageSourceBaseUrl"
    }
    return "https://$configured/$PublicationPackageRootPath"
}

internal fun packagedArtifactRootUrl(publicationRootUrl: String, manifest: CurrentArtifactsManifestWire): String {
    val packagedRoot = manifest.artifactRoots.packaged.trim()
    check(packagedRoot.isNotBlank()) { "current artifacts manifest missing artifact_roots.packaged" }
    return resolvePackageSourceUrl(packagedRoot, publicationRootUrl)
}

internal suspend fun downloadPackageToTempFile(
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

internal fun installDownloadedPackage(
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

internal fun validateInstalledPackageOrNull(
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

internal fun resolvePackageSourceUrl(relativePath: String, packageSourceBaseUrl: String): String =
    when {
        relativePath.startsWith("http://") || relativePath.startsWith("https://") -> relativePath
        packageSourceBaseUrl.endsWith("/") -> "$packageSourceBaseUrl$relativePath"
        else -> "$packageSourceBaseUrl/$relativePath"
    }

internal fun formatNavDbStatusLine(status: net.jonh.aerobag.prototype.domain.NavDbStatus): String {
    if (status.installed.isEmpty()) {
        return "NAVDB none installed"
    }
    val parts = status.installed.map { artifact ->
        val cycle = artifact.packageId.split('_').getOrNull(2) ?: artifact.packageId
        if (artifact.readable) "$cycle ok" else "$cycle bad"
    }
    return "NAVDB ${status.installed.size}: ${parts.joinToString(", ")}"
}

internal fun installedPackageKindForFamilyId(familyId: String): InstalledPackageKind = when (familyId) {
    "sec", "tac", "shaded-relief", "enr-l", "enr-h" -> InstalledPackageKind.Charts
    "tpp", "csup" -> InstalledPackageKind.Plates
    "nav-db", "vectors", "geo", "terrain", "metars", "tfrs", "nexrad", "obstacles" -> InstalledPackageKind.Data
    else -> error("unsupported package family for install: $familyId")
}

internal fun deleteInstalledArtifact(
    context: Context,
    artifactId: String,
    filename: String,
    keepFilename: String? = null,
) {
    InstalledPackageKind.entries.forEach { kind ->
        InstalledPackages.deleteInstalledArtifact(context, kind, artifactId, filename, keepFilename)
    }
}

internal fun sha256Hex(bytes: ByteArray): String =
    MessageDigest.getInstance("SHA-256")
        .digest(bytes)
        .joinToString("") { "%02x".format(it) }

internal fun offlineRegionOptions(): List<OfflinePackageDimension> {
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
