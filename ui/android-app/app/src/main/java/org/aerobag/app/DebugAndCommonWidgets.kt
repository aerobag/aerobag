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
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.rememberTextMeasurer
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
import androidx.compose.ui.unit.Constraints
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.TextUnit
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


@Composable
internal fun CommonDebugPanel(
    uptimeLabel: String,
    debugState: UiDebugState,
    onDebugFlagChange: (String, Boolean) -> Unit,
) {
    Text("up $uptimeLabel", style = MaterialTheme.typography.labelSmall, color = Color(0xFF52656D))
    DebugCheckbox("tile labels", debugState.tileLabels) { onDebugFlagChange("tile_labels", it) }
    DebugCheckbox("NEXRAD tile labels", debugState.nexradTileLabels) { onDebugFlagChange("nexrad_tile_labels", it) }
    DebugCheckbox("fast tiles", debugState.fastTiles) { onDebugFlagChange("fast_tiles", it) }
    DebugCheckbox("offline simulated clock buttons", debugState.offlineSimulatedClockButtons) {
        onDebugFlagChange("offline_simulated_clock_buttons", it)
    }
}

@Composable
internal fun DebugCheckbox(
    label: String,
    checked: Boolean,
    onCheckedChange: (Boolean) -> Unit,
) {
    Row(
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(4.dp),
    ) {
        Checkbox(
            checked = checked,
            onCheckedChange = onCheckedChange,
            modifier = Modifier.size(ThumbSize * 0.36f),
        )
        Text(label, style = MaterialTheme.typography.labelSmall, color = Color(0xFF52656D))
    }
}

@Composable
internal fun DebugDock(
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
            Card(
                modifier = Modifier
                    .width(ThumbSize * 4f),
            ) {
                Column(
                    modifier = Modifier.padding(horizontal = 10.dp, vertical = 8.dp),
                    verticalArrangement = Arrangement.spacedBy(2.dp),
                    content = content,
                )
            }
        }
    }
}

internal fun pageLabel(page: AppPage): String = PageOptions.firstOrNull { it.page == page }?.launcherLabel ?: page.name.uppercase()

@Composable
internal fun rememberUptimeLabel(sessionStartElapsedMs: Long): String {
    val nowMs by produceState(initialValue = SystemClock.elapsedRealtime(), sessionStartElapsedMs) {
        while (true) {
            value = SystemClock.elapsedRealtime()
            delay(1000)
        }
    }
    return formatUptimeLabel(nowMs - sessionStartElapsedMs)
}

internal fun formatUptimeLabel(elapsedMs: Long): String {
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

internal fun formatSnapshot(snapshot: AppViewSnapshot): String {
    return formatSnapshot(snapshot, emptyMap())
}

internal fun formatSnapshot(snapshot: AppViewSnapshot, chartLabelsById: Map<String, String>): String {
    val label = pageLabel(snapshot.page)
    if (snapshot.page == AppPage.Map) {
        val family = snapshot.selectedMapLauncherLabel
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

internal fun formatPageStack(
    pageHistory: List<AppViewSnapshot>,
    currentPage: AppPage,
    selectedMapId: String = "",
    selectedMapLauncherLabel: String = "",
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
        selectedMapLauncherLabel = selectedMapLauncherLabel,
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
internal fun ToolbarButton(label: String, modifier: Modifier = Modifier, onClick: () -> Unit) {
    CompactSquareButton(label = label, modifier = modifier.size(ThumbSize), onClick = onClick)
}

internal fun buttonContainerColor(
    uiTheme: UiTheme,
    enabled: Boolean,
    selected: Boolean,
    backgroundColor: Color? = null,
    selectedColor: Color? = null,
): Color = when {
    !enabled -> uiTheme.controls.disabledButton
    selected -> selectedColor ?: uiTheme.controls.buttonSelectedBg
    else -> backgroundColor ?: uiTheme.controls.buttonBg
}

internal fun buttonLabel(label: String): String = label.uppercase()

@Composable
internal fun buttonLabelStyle(): TextStyle = MaterialTheme.typography.labelSmall.copy(fontWeight = FontWeight.Bold)

@Composable
internal fun FittedSingleLineText(
    text: String,
    modifier: Modifier = Modifier,
    style: TextStyle,
    color: Color,
    minFontSize: TextUnit = 9.sp,
    maxFontSize: TextUnit = style.fontSize.takeIf { it != TextUnit.Unspecified } ?: 12.sp,
    textAlign: TextAlign = TextAlign.Center,
    outlineColor: Color? = null,
    outlineOffsets: List<IntOffset> = emptyList(),
) {
    val textMeasurer = rememberTextMeasurer()
    BoxWithConstraints(
        modifier = modifier,
        contentAlignment = when (textAlign) {
            TextAlign.Start -> Alignment.CenterStart
            TextAlign.End -> Alignment.CenterEnd
            else -> Alignment.Center
        },
    ) {
        val maxWidthPx = constraints.maxWidth
        val maxHeightPx = constraints.maxHeight
        val fittedStyle = remember(text, style, minFontSize, maxFontSize, maxWidthPx, maxHeightPx) {
            if (maxWidthPx <= 0 || maxHeightPx <= 0) {
                return@remember style.copy(fontSize = maxFontSize)
            }
            var candidateSp = maxFontSize.value
            val minSp = minFontSize.value.coerceAtMost(candidateSp)
            while (candidateSp >= minSp) {
                val candidateStyle = style.copy(fontSize = candidateSp.sp)
                val result = textMeasurer.measure(
                    text = AnnotatedString(text),
                    style = candidateStyle,
                    overflow = TextOverflow.Clip,
                    softWrap = false,
                    maxLines = 1,
                    constraints = Constraints(maxWidth = maxWidthPx, maxHeight = maxHeightPx),
                )
                if (!result.hasVisualOverflow) {
                    return@remember candidateStyle
                }
                candidateSp -= 0.5f
            }
            style.copy(fontSize = minSp.sp)
        }
        outlineColor?.let { strokeColor ->
            outlineOffsets.forEach { offset ->
                Text(
                    text = text,
                    modifier = Modifier.offset { offset },
                    style = fittedStyle,
                    maxLines = 1,
                    softWrap = false,
                    overflow = TextOverflow.Clip,
                    textAlign = textAlign,
                    color = strokeColor,
                )
            }
        }
        Text(
            text = text,
            style = fittedStyle,
            maxLines = 1,
            softWrap = false,
            overflow = TextOverflow.Clip,
            textAlign = textAlign,
            color = color,
        )
    }
}

@Composable
internal fun IconFrame(
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
internal fun LayerToggle(
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
internal fun OutlinedButtonLabel(
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
    if (maxLines == 1 && !text.contains('\n')) {
        FittedSingleLineText(
            text = text,
            modifier = modifier,
            style = style,
            color = color,
            minFontSize = 9.sp,
            textAlign = textAlign,
            outlineColor = Color.Black,
            outlineOffsets = offsets,
        )
        return
    }
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
internal fun CompactSquareButton(
    label: String,
    modifier: Modifier = Modifier,
    maxLines: Int = 1,
    enabled: Boolean = true,
    selected: Boolean = false,
    backgroundColor: Color? = null,
    foregroundColor: Color? = null,
    selectedColor: Color? = null,
    accentColor: Color? = null,
    @DrawableRes iconResId: Int? = null,
    wide: Boolean = false,
    centered: Boolean = true,
    textStartPadding: Dp = 0.dp,
    textModifier: Modifier = Modifier,
    testTag: String? = null,
    onDisabledClick: (() -> Unit)? = null,
    onClick: () -> Unit,
) {
    val uiTheme = LocalAerobagUiTheme.current
    val iconShape = RoundedCornerShape(ThumbRadius)
    val resolvedContentColor = foregroundColor ?: uiTheme.controls.buttonFg
    val renderedLabel = buttonLabel(label)
    val renderedLabelStyle = buttonLabelStyle()
    val resolvedContainerColor = buttonContainerColor(
        uiTheme = uiTheme,
        enabled = enabled,
        selected = selected,
        backgroundColor = backgroundColor,
        selectedColor = selectedColor,
    )
    Surface(
        modifier = modifier
            .testTag(testTag ?: "parity:button:$label")
            .semantics {
                if (!enabled) {
                    disabled()
                }
            }
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
        color = resolvedContainerColor,
        contentColor = resolvedContentColor,
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
                    text = renderedLabel,
                    modifier = Modifier
                        .align(Alignment.BottomCenter)
                        .fillMaxWidth()
                .padding(horizontal = if (wide) 0.dp else 1.dp, vertical = 2.dp)
                        .then(textModifier),
                    style = renderedLabelStyle.copy(fontSize = 13.sp),
                    maxLines = if (renderedLabel.contains('\n')) maxLines else 1,
                    color = resolvedContentColor,
                )
            } else {
                if (maxLines == 1 && !renderedLabel.contains('\n')) {
                    FittedSingleLineText(
                        text = renderedLabel,
                        modifier = (if (centered) Modifier.fillMaxWidth() else Modifier.padding(start = textStartPadding, end = 8.dp).fillMaxWidth()).then(textModifier),
                        style = renderedLabelStyle,
                        textAlign = if (centered) TextAlign.Center else TextAlign.Start,
                        color = resolvedContentColor,
                    )
                } else {
                    Text(
                        text = renderedLabel,
                        modifier = (if (centered) Modifier else Modifier.padding(start = textStartPadding, end = 8.dp)).then(textModifier),
                        style = renderedLabelStyle,
                        textAlign = if (centered) TextAlign.Center else TextAlign.Start,
                        maxLines = maxLines,
                        overflow = TextOverflow.Clip,
                        color = resolvedContentColor,
                    )
                }
            }
        }
    }
}

@Composable
internal fun Scrim(modifier: Modifier = Modifier, onDismiss: () -> Unit) {
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
