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
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.platform.LocalContext
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
import org.aerobag.app.domain.NativeSessionCommandRejectedException
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
internal fun PlaybackWidget(
    uiSession: NativeUiSession,
    playbackUiState: PlaybackUiState,
    sourcePath: String,
    onSourcePathChange: (String) -> Unit,
    onSnapshotChange: (UiSessionSnapshot) -> Unit,
    onSessionCommandFailure: (Throwable) -> Unit,
    modifier: Modifier = Modifier,
) {
    val context = LocalContext.current
    val devServerBaseUrl = remember(context) { loadAndroidDevServerBaseUrl(context.applicationContext) }
    val scope = rememberCoroutineScope()
    var isBusy by remember { mutableStateOf(false) }
    var scrubCursorSeconds by remember { mutableStateOf<Double?>(null) }
    var seekJob by remember { mutableStateOf<Job?>(null) }
    fun applyPlaybackCommand(commandName: String, operation: () -> UiSessionSnapshot): UiSessionSnapshot? =
        try {
            operation().also(onSnapshotChange)
        } catch (error: CancellationException) {
            throw error
        } catch (error: NativeSessionCommandRejectedException) {
            onSessionCommandFailure(error)
            null
        } catch (error: Throwable) {
            Log.e("AerobagPlayback", "$commandName failed", error)
            null
        }
    val durationSeconds = playbackUiState.durationSeconds.coerceAtLeast(0.0)
    val committedCursorSeconds = playbackUiState.cursorSeconds.coerceIn(0.0, durationSeconds.takeIf { it > 0.0 } ?: 0.0)
    val cursorSeconds = (scrubCursorSeconds ?: committedCursorSeconds).coerceIn(0.0, durationSeconds.takeIf { it > 0.0 } ?: 0.0)
    val summary = playbackUiState.titleLabel
    val panelShape = RoundedCornerShape(ThumbRadius * 0.9f)
    val rowHeight = ThumbSize * 0.63f
    val rowGap = ThumbSize * 0.12f
    Surface(
        modifier =
            modifier
                .widthIn(min = ThumbSize * 4.2f, max = ThumbSize * 7.8f)
                .consumePointerGestures(),
        shape = panelShape,
        color = Color(0xF0FCF8F1),
        contentColor = Color(0xFF132129),
        border = BorderStroke(1.dp, Color(0x334E626C)),
        shadowElevation = 6.dp,
    ) {
        Column(
            modifier = Modifier.padding(ThumbSize * 0.18f),
            verticalArrangement = Arrangement.spacedBy(rowGap),
        ) {
            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.spacedBy(rowGap),
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
                horizontalArrangement = Arrangement.spacedBy(rowGap),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                BasicTextField(
                    value = sourcePath,
                    onValueChange = onSourcePathChange,
                    singleLine = true,
                    textStyle =
                        MaterialTheme.typography.labelSmall.copy(
                            color = Color(0xFF132129),
                            fontSize = 16.sp,
                        ),
                    modifier =
                        Modifier
                            .weight(1f)
                            .height(rowHeight)
                            .clip(RoundedCornerShape(ThumbRadius * 0.55f))
                            .background(Color.White)
                            .border(1.dp, Color(0x24132129), RoundedCornerShape(ThumbRadius * 0.55f))
                            .padding(horizontal = ThumbSize * 0.15f, vertical = ThumbSize * 0.08f),
                )
                PlaybackSmallButton(
                    label = "LOAD",
                    enabled = !isBusy && sourcePath.isNotBlank(),
                    onClick = {
                        scope.launch {
                            isBusy = true
                            try {
                                val traceUrl = resolvePlaybackTraceUrl(sourcePath, devServerBaseUrl)
                                diagnosticLogInfo("AerobagPlayback") { "loading trace $traceUrl" }
                                val traceJson =
                                    withContext(Dispatchers.IO) {
                                        fetchResourceBytes(traceUrl).decodeToString()
                                    }
                                applyPlaybackCommand("loadPlaybackTrace") {
                                    uiSession.loadPlaybackTrace(sourcePath, traceJson)
                                }
                            } catch (error: Throwable) {
                                if (error is NativeSessionCommandRejectedException) {
                                    onSessionCommandFailure(error)
                                } else {
                                    Log.e("AerobagPlayback", "trace load failed: $sourcePath", error)
                                }
                            } finally {
                                isBusy = false
                            }
                        }
                    },
                )
            }
            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.spacedBy(rowGap),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                PlaybackSmallButton(
                    label = "",
                    icon = if (playbackUiState.status == PlaybackStatus.Playing) PlaybackButtonIcon.Pause else PlaybackButtonIcon.Play,
                    enabled = playbackUiState.status != PlaybackStatus.Empty,
                    onClick = {
                        scope.launch {
                            applyPlaybackCommand("playPausePlayback") {
                                if (playbackUiState.status == PlaybackStatus.Playing) {
                                    uiSession.pausePlayback(System.currentTimeMillis().toDouble())
                                } else {
                                    uiSession.playPlayback(System.currentTimeMillis().toDouble())
                                }
                            }
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
                    modifier = Modifier.weight(1f).height(rowHeight),
                    onValueChange = { nextRate ->
                        scope.launch {
                            applyPlaybackCommand("setPlaybackRate") {
                                uiSession.setPlaybackRate(nextRate.toDouble(), System.currentTimeMillis().toDouble())
                            }
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
                        applyPlaybackCommand("seekPlayback") {
                            uiSession.seekPlayback(nextCursorSeconds, System.currentTimeMillis().toDouble())
                        }?.let {
                            if (finished) {
                                scrubCursorSeconds = null
                            }
                        }
                    }
                },
            )
            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.SpaceBetween,
            ) {
                Text(
                    playbackUiState.cursorLabel,
                    style = MaterialTheme.typography.labelSmall,
                    fontWeight = FontWeight.Bold,
                    color = Color(0xFF52656D),
                )
                Text(
                    playbackUiState.durationLabel,
                    style = MaterialTheme.typography.labelSmall,
                    fontWeight = FontWeight.Bold,
                    color = Color(0xFF52656D),
                )
            }
        }
    }
}

@Composable
internal fun PlaybackSmallButton(
    label: String,
    enabled: Boolean,
    onClick: () -> Unit,
    icon: PlaybackButtonIcon? = null,
    height: Dp = ThumbSize * 0.63f,
) {
    Surface(
        modifier =
            Modifier
                .height(height)
                .then(if (icon == null) Modifier.widthIn(min = height * 2.05f) else Modifier.width(height))
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
            modifier = if (icon == null) Modifier.padding(horizontal = ThumbSize * 0.21f) else Modifier.fillMaxSize(),
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

internal enum class PlaybackButtonIcon {
    Play,
    Pause,
}

@Composable
internal fun PlaybackButtonIconCanvas(icon: PlaybackButtonIcon) {
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

internal fun Modifier.consumePointerGestures(): Modifier =
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
internal fun PlaybackRateRail(
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
internal fun PlaybackOverview(
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
                .height(ThumbSize * 0.84f)
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
