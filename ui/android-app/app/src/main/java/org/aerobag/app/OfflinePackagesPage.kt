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
import org.aerobag.app.domain.AirwayPresentationPlan
import org.aerobag.app.domain.AirwaySuggestion
import org.aerobag.app.domain.WaypointIdentifierSuggestion
import org.aerobag.app.domain.CoreMapViewport
import org.aerobag.app.domain.DerivedChartPageState
import org.aerobag.app.domain.FlightPlanEntryPreview
import org.aerobag.app.domain.FlightPlanDisplayRowKind
import org.aerobag.app.domain.FlightPlanDisplayRowUiView
import org.aerobag.app.domain.FlightPlanRowActionUiView
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
import java.io.FileOutputStream
import java.io.IOException
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

private fun Modifier.offlinePackagesActionButtonSize(): Modifier =
    width(ThumbSize * 2f).height(ThumbSize)

@Composable
internal fun OfflinePackagesErrorPanel(
    message: String,
    closeEnabled: Boolean,
    onClose: () -> Unit,
    showCloseButton: Boolean = true,
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
                if (showCloseButton) {
                    CompactSquareButton(
                        label = "X",
                        modifier = Modifier.size(ThumbSize * 0.72f),
                        enabled = closeEnabled,
                        onClick = onClose,
                    )
                }
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
    storageCapacityLabel: String?,
    packageSourceBaseUrl: String,
    onPackageSourceBaseUrlChange: (String) -> Unit,
    refreshInFlight: Boolean,
    sourceEditable: Boolean,
    sourceEditDisabledReason: String? = null,
    refreshEnabled: Boolean,
    refreshDisabledReason: String? = null,
    refreshCancelEnabled: Boolean,
    cancelRequested: Boolean,
    onRefresh: () -> Unit,
    onCancelRefresh: () -> Unit,
    closeEnabled: Boolean,
    onClose: () -> Unit,
    showCloseButton: Boolean = true,
    modifier: Modifier = Modifier,
) {
    val context = LocalContext.current
    val uiTheme = LocalAerobagUiTheme.current
    val refreshButtonDisabledReason = when {
        cancelRequested -> "Waiting for cancellation to finish."
        refreshInFlight && !refreshCancelEnabled -> "Refresh cancellation is not available."
        !refreshInFlight && !refreshEnabled -> refreshDisabledReason
        else -> null
    }
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
                Row(
                    horizontalArrangement = Arrangement.spacedBy(ThumbGap),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    CompactSquareButton(
                        label = if (refreshInFlight) {
                            if (cancelRequested) "⇣ CANCELING..." else "⇣ REFRESHING\n(cancel)"
                        } else {
                            "⇣ REFRESH\nCATALOG"
                        },
                        modifier = Modifier.offlinePackagesActionButtonSize(),
                        maxLines = 2,
                        enabled = !cancelRequested && if (refreshInFlight) refreshCancelEnabled else refreshEnabled,
                        testTag = "parity:offline-refresh-button",
                        onDisabledClick = refreshButtonDisabledReason?.let { reason ->
                            { showDisabledActionToast(context, reason) }
                        },
                        onClick = if (refreshInFlight) onCancelRefresh else onRefresh,
                    )
                    if (showCloseButton) {
                        CompactSquareButton(
                            label = "X",
                            modifier = Modifier.size(ThumbSize * 0.72f),
                            enabled = closeEnabled,
                            testTag = "parity:offline-close-button",
                            onClick = onClose,
                        )
                    }
                }
            }
            Text(
                text = message,
                style = MaterialTheme.typography.bodyMedium,
                color = uiTheme.controls.panelFg,
            )
            storageCapacityLabel?.let { label ->
                Text(
                    text = label,
                    style = MaterialTheme.typography.labelMedium,
                    fontWeight = FontWeight.Bold,
                    color = uiTheme.controls.panelFg,
                )
            }
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
                        .then(
                            if (!sourceEditable && !sourceEditDisabledReason.isNullOrBlank()) {
                                Modifier.clickable(
                                    indication = null,
                                    interactionSource = remember { MutableInteractionSource() },
                                ) { showDisabledActionToast(context, sourceEditDisabledReason) }
                            } else {
                                Modifier
                            },
                        )
                        .padding(horizontal = ThumbGap * 0.7f, vertical = ThumbGap * 0.55f),
                )
            }
        }
    }
}

@Composable
internal fun OfflinePackagesPanel(
    uiState: OfflinePackagesUiStateWire,
    storageCapacityLabel: String?,
    syncMessage: String?,
    cancelRequested: Boolean,
    showSimulatedClockButtons: Boolean,
    packageSourceBaseUrl: String,
    onPackageSourceBaseUrlChange: (String) -> Unit,
    onRefreshLibrary: () -> Unit,
    libraryRefreshInFlight: Boolean,
    packageSourceEditable: Boolean,
    packageSourceEditDisabledReason: String? = null,
    refreshEnabled: Boolean,
    refreshDisabledReason: String? = null,
    refreshCancelEnabled: Boolean,
    syncEnabled: Boolean,
    syncDisabledReason: String? = null,
    syncCancelEnabled: Boolean,
    plannerInteractionsEnabled: Boolean,
    plannerInteractionsDisabledReason: String? = null,
    onCancelRefresh: () -> Unit,
    onRowClick: (OfflinePackagesEventWire) -> Unit,
    onClockClick: (String) -> Unit,
    onSync: () -> Unit,
    onCancelOperation: () -> Unit,
    syncInFlight: Boolean,
    closeEnabled: Boolean,
    onClose: () -> Unit,
    showCloseButton: Boolean = true,
    modifier: Modifier = Modifier,
) {
    val context = LocalContext.current
    val uiTheme = LocalAerobagUiTheme.current
    val refreshButtonDisabledReason = when {
        cancelRequested -> "Waiting for cancellation to finish."
        libraryRefreshInFlight && !refreshCancelEnabled -> "Refresh cancellation is not available."
        !libraryRefreshInFlight && !refreshEnabled -> refreshDisabledReason
        else -> null
    }
    val syncButtonDisabledReason = when {
        cancelRequested -> "Waiting for cancellation to finish."
        syncInFlight && !syncCancelEnabled -> "Sync cancellation is not available."
        !syncInFlight && !syncEnabled -> syncDisabledReason
        else -> null
    }
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
                Row(
                    horizontalArrangement = Arrangement.spacedBy(ThumbGap),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    CompactSquareButton(
                        label = if (libraryRefreshInFlight) {
                            if (cancelRequested) "⇣ CANCELING..." else "⇣ REFRESHING\n(cancel)"
                        } else {
                            "⇣ REFRESH\nCATALOG"
                        },
                        modifier = Modifier.offlinePackagesActionButtonSize(),
                        maxLines = 2,
                        enabled = !cancelRequested && if (libraryRefreshInFlight) refreshCancelEnabled else refreshEnabled,
                        testTag = "parity:offline-refresh-button",
                        onDisabledClick = refreshButtonDisabledReason?.let { reason ->
                            { showDisabledActionToast(context, reason) }
                        },
                        onClick = if (libraryRefreshInFlight) onCancelRefresh else onRefreshLibrary,
                    )
                    CompactSquareButton(
                        label = if (syncInFlight) {
                            if (cancelRequested) "⇊ CANCELING..." else "⇊ APPLYING\n(cancel)"
                        } else {
                            "⇊ APPLY\nCHANGES"
                        },
                        modifier = Modifier.offlinePackagesActionButtonSize(),
                        maxLines = 2,
                        enabled = !cancelRequested && if (syncInFlight) syncCancelEnabled else syncEnabled,
                        testTag = "parity:offline-sync-button",
                        onDisabledClick = syncButtonDisabledReason?.let { reason ->
                            { showDisabledActionToast(context, reason) }
                        },
                        onClick = if (syncInFlight) onCancelOperation else onSync,
                    )
                    if (showCloseButton) {
                        CompactSquareButton(
                            label = "X",
                            modifier = Modifier.size(ThumbSize * 0.72f),
                            enabled = closeEnabled,
                            testTag = "parity:offline-close-button",
                            onClick = onClose,
                        )
                    }
                }
            }

            syncMessage?.let { message ->
                Text(
                    text = message,
                    style = MaterialTheme.typography.labelMedium,
                    fontWeight = FontWeight.Bold,
                    color = Color(0xFFD98B38),
                )
            }
            storageCapacityLabel?.let { label ->
                Text(
                    text = label,
                    style = MaterialTheme.typography.labelMedium,
                    fontWeight = FontWeight.Bold,
                    color = uiTheme.controls.panelFg,
                )
            }
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
                        .then(
                            if (!packageSourceEditable && !packageSourceEditDisabledReason.isNullOrBlank()) {
                                Modifier.clickable(
                                    indication = null,
                                    interactionSource = remember { MutableInteractionSource() },
                                ) { showDisabledActionToast(context, packageSourceEditDisabledReason) }
                            } else {
                                Modifier
                            },
                        )
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
                            onDisabledClick = plannerInteractionsDisabledReason?.let { reason ->
                                { showDisabledActionToast(context, reason) }
                            },
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
                        OfflinePackageSection(
                            title = "CORE",
                            testTagPrefix = "parity:offline-core",
                            rows = uiState.coreProducts,
                            enabled = plannerInteractionsEnabled,
                            disabledReason = plannerInteractionsDisabledReason,
                            onRowClick = onRowClick,
                        )
                    }
                }
                if (uiState.zoomLevels.isNotEmpty()) {
                    item("zoom-levels") {
                        OfflinePackageSection(
                            title = "ZOOM LEVELS",
                            testTagPrefix = "parity:offline-zoom-level",
                            rows = uiState.zoomLevels,
                            enabled = plannerInteractionsEnabled,
                            disabledReason = plannerInteractionsDisabledReason,
                            onRowClick = onRowClick,
                        )
                    }
                }
                item("regions") {
                    OfflinePackageSection(
                        title = "REGIONS",
                        testTagPrefix = "parity:offline-region",
                        rows = uiState.regions,
                        enabled = plannerInteractionsEnabled,
                        disabledReason = plannerInteractionsDisabledReason,
                        onRowClick = onRowClick,
                    )
                }
                item("products") {
                    OfflinePackageSection(
                        title = "PRODUCTS",
                        testTagPrefix = "parity:offline-product",
                        rows = uiState.products,
                        enabled = plannerInteractionsEnabled,
                        disabledReason = plannerInteractionsDisabledReason,
                        onRowClick = onRowClick,
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
            label = row.label,
            row = row,
            enabled = false,
            onCycleClick = null,
            backgroundOverride = lerp(uiTheme.controls.buttonUnchecked, Color.Gray, 0.34f),
        )
    }
}

@Composable
internal fun OfflinePackageSection(
    title: String,
    testTagPrefix: String,
    rows: List<OfflinePackagesUiRowWire>,
    enabled: Boolean,
    disabledReason: String? = null,
    onRowClick: (OfflinePackagesEventWire) -> Unit,
) {
    val uiTheme = LocalAerobagUiTheme.current
    MenuPanel(modifier = Modifier.fillMaxWidth()) {
        Text(
            text = title,
            modifier = Modifier.padding(horizontal = 9.dp, vertical = 5.dp),
            style = MaterialTheme.typography.labelMedium,
            fontWeight = FontWeight.ExtraBold,
            color = uiTheme.controls.panelMuted,
        )
        rows.forEach { row ->
            val selectionEvent = row.selectionEvent
            OfflinePackagePlanRow(
                label = row.label,
                row = row,
                testTag = "$testTagPrefix:${row.id}",
                enabled = enabled && selectionEvent != null,
                disabledReason = selectionEvent?.let { disabledReason },
                onCycleClick = selectionEvent?.let { event ->
                    { onRowClick(event) }
                },
            )
        }
    }
}

@Composable
internal fun OfflinePackagePlanRow(
    label: String,
    row: OfflinePackagesUiRowWire,
    enabled: Boolean,
    disabledReason: String? = null,
    onCycleClick: (() -> Unit)?,
    backgroundOverride: Color? = null,
    testTag: String? = null,
) {
    val context = LocalContext.current
    val uiTheme = LocalAerobagUiTheme.current
    val background = backgroundOverride ?: when (row.selection) {
            OfflinePackageSelection.Play -> lerp(uiTheme.controls.buttonUnchecked, Color.White, 0.14f)
            OfflinePackageSelection.Pause -> lerp(uiTheme.controls.buttonUnchecked, Color(0xFFFFC166), 0.18f)
            OfflinePackageSelection.Unselected -> uiTheme.controls.buttonUnchecked
        }
    val progressFraction = row.syncProgressPerMille?.coerceIn(0, 1000)?.toFloat()?.div(1000f)
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .height(ThumbSize * 1.32f)
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
        Box(
            modifier = Modifier.size(ThumbSize * 0.46f),
            contentAlignment = Alignment.Center,
        ) {
            if (onCycleClick != null) {
                Box(
                    modifier = Modifier
                        .fillMaxSize()
                        .clip(CircleShape)
                        .then(testTag?.let { Modifier.testTag("$it:toggle") } ?: Modifier)
                        .then(
                            if (enabled) {
                                Modifier.clickable(
                                    indication = null,
                                    interactionSource = remember { MutableInteractionSource() },
                                ) { onCycleClick() }
                            } else {
                                Modifier
                                    .alpha(0.58f)
                                    .then(
                                        if (!disabledReason.isNullOrBlank()) {
                                            Modifier.clickable(
                                                indication = null,
                                                interactionSource = remember { MutableInteractionSource() },
                                            ) { showDisabledActionToast(context, disabledReason) }
                                        } else {
                                            Modifier
                                        },
                                    )
                            },
                        ),
                    contentAlignment = Alignment.Center,
                ) {
                    OfflinePackageSelectionIcon(
                        selection = row.selection,
                        modifier = Modifier.fillMaxSize(),
                    )
                }
            }
        }
        Text(
            text = label,
            modifier = Modifier.width(ThumbSize * 1.72f),
            style = MaterialTheme.typography.labelLarge.copy(lineHeight = 17.sp),
            color = uiTheme.controls.buttonFg,
            maxLines = 3,
            overflow = TextOverflow.Clip,
        )
        Box(
            modifier = Modifier.size(26.dp),
            contentAlignment = Alignment.Center,
        ) {
            if (!row.helpText.isNullOrBlank()) {
                Box(
                    modifier = Modifier
                        .fillMaxSize()
                        .clip(CircleShape)
                        .border(1.dp, uiTheme.controls.buttonFg, CircleShape)
                        .clickable(
                            indication = null,
                            interactionSource = remember { MutableInteractionSource() },
                        ) { showActionToast(context, row.helpText, long = true) },
                    contentAlignment = Alignment.Center,
                ) {
                    Text(
                        text = "?",
                        style = MaterialTheme.typography.labelMedium,
                        fontWeight = FontWeight.ExtraBold,
                        color = uiTheme.controls.buttonFg,
                    )
                }
            }
        }
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
    val unchangedColor = LocalAerobagUiTheme.current.controls.buttonFg
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
                        color = offlinePackagePlanActionColor(entry.action, unchangedColor),
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
    val color = offlinePackagePlanActionColor(
        action,
        LocalAerobagUiTheme.current.controls.buttonFg,
    )
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

internal fun offlinePackagePlanActionColor(
    action: OfflinePackagesUiPlanActionWire,
    unchangedColor: Color,
): Color = when (action) {
    OfflinePackagesUiPlanActionWire.Delete -> OfflinePackageRed
    OfflinePackagesUiPlanActionWire.Keep -> unchangedColor
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
): Long = NativeBindings.createOfflinePackagesController(
    readOfflinePackagesStateJson(prefs),
    readOfflinePackagesLibraryCacheJson(prefs),
)

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

internal fun writeOfflinePackagesLibraryCacheJson(
    prefs: android.content.SharedPreferences,
    libraryCacheJson: String?,
) {
    prefs.edit()
        .putString(UiPrefsOfflinePackageLibraryCacheKey, libraryCacheJson)
        .apply()
}

internal fun listInstalledPackageArtifacts(context: Context): List<InstalledArtifactWire> {
    return InstalledPackages.listInstalledArtifacts(context)
        .asSequence()
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

internal fun installedPackageStorageInfo(context: Context): OfflinePackagesStorageInfoWire {
    val stats = InstalledPackages.packageStorageStats(context)
    return OfflinePackagesStorageInfoWire(
        availableBytes = stats.availableBytes,
        totalBytes = stats.totalBytes,
    )
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
    beforeGc: suspend () -> Set<String> = { emptySet() },
): OfflinePackagesSyncSummary {
    val syncStartMs = SystemClock.elapsedRealtime()
    val packagedArtifactRootUrl = resolvePackageSourceUrl(
        packagedArtifactRoot,
        resolvePublicationRootUrl(packageSourceBaseUrl),
    )
    val packagesById = bundle.packages.associateBy { it.id }
    val warnings = mutableListOf<OfflinePackagesWarning>()
    val totalFetchBytes = plan.fetch.sumOf { artifactId -> packagesById[artifactId]?.sizeBytes ?: 0L }
    val completedFetchArtifactIds = linkedSetOf<String>()
    val activeFetchBytesByArtifactId = linkedMapOf<String, Long>()
    val progressMutex = Mutex()
    var completedFetchBytes = 0L
    var fetchedCount = 0
    var gcCount = 0
    fun activeFetchBytes(): Long = activeFetchBytesByArtifactId.values.sum()
    suspend fun reportProgress(message: String) {
        progressMutex.withLock {
            onProgress(
                message,
                OfflinePackagesSyncProgressWire(
                    completedFetchArtifactIds = completedFetchArtifactIds.toSet(),
                    activeFetchBytesByArtifactId = activeFetchBytesByArtifactId.toMap(),
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
                            reportProgress("Fetching package ${index + 1}/${plan.fetch.size}: ${pkg.filename}")
                            check(packageSourceBaseUrl.isNotBlank()) { "package source URL is blank" }
                            val sourceUrl = resolvePackageSourceUrl(pkg.relativePath, packagedArtifactRootUrl)
                            var packageDownloadedBytes = 0L
                            var lastReportedPackageBytes = 0L
                            progressMutex.withLock {
                                activeFetchBytesByArtifactId[artifactId] = 0L
                            }
                            val tempFile = downloadPackageWithRetries(
                                context = context,
                                filename = pkg.filename,
                                sourceUrl = sourceUrl,
                                expectedSizeBytes = pkg.sizeBytes,
                                expectedSha256 = pkg.checksumSha256,
                                activeConnections = activeConnections,
                                onBytesRead = { downloadedBytes ->
                                    packageDownloadedBytes = downloadedBytes
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
                                        )
                                    }
                                },
                            )
                            installDownloadedPackage(
                                context = context,
                                artifactId = pkg.id,
                                filename = pkg.filename,
                                tempFile = tempFile,
                                sizeBytes = pkg.sizeBytes,
                                checksumSha256 = pkg.checksumSha256,
                            )
                            progressMutex.withLock {
                                activeFetchBytesByArtifactId.remove(artifactId)
                                completedFetchBytes += packageDownloadedBytes
                                completedFetchArtifactIds += artifactId
                                fetchedCount += 1
                            }
                            val aggregateFetchBytes = progressMutex.withLock {
                                completedFetchBytes + activeFetchBytes()
                            }
                            reportProgress(syncProgressText(fetchedCount, plan.fetch.size, aggregateFetchBytes, totalFetchBytes))
                            diagnosticLogInfo("OfflinePackages") {
                                "fetch installed $artifactId worker=$workerIndex in ${SystemClock.elapsedRealtime() - fetchStartMs}ms from $sourceUrl"
                            }
                        }.onFailure {
                            if (it is CancellationException) throw it
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
    val retainedFilenames = if (plan.fetch.isNotEmpty() || plan.gc.isNotEmpty()) {
        try {
            beforeGc()
        } catch (error: Throwable) {
            Log.e("OfflinePackages", "runtime adoption failed; GC suppressed", error)
            warnings += OfflinePackagesWarning(
                artifactId = "runtime-adoption",
                familyId = null,
                regionId = null,
                message = error.message ?: error::class.simpleName ?: "runtime adoption failed",
            )
            return OfflinePackagesSyncSummary(
                fetchedCount = fetchedCount,
                gcCount = 0,
                warnings = warnings,
            )
        }
    } else {
        emptySet()
    }
    val gcResult = gcOfflinePackages(
        context = context,
        plan = plan,
        bundle = bundle,
        retainedFilenames = retainedFilenames,
        onProgress = { message -> reportProgress(message) },
    )
    gcCount += gcResult.gcCount
    warnings += gcResult.warnings
    reportProgress("Sync complete: fetched $fetchedCount, GC $gcCount")
    return OfflinePackagesSyncSummary(
        fetchedCount = fetchedCount,
        gcCount = gcCount,
        warnings = warnings,
    ).also {
        diagnosticLogInfo("OfflinePackages") {
            "sync completed in ${SystemClock.elapsedRealtime() - syncStartMs}ms " +
                "(fetch=${plan.fetch.size}, gc=${plan.gc.size}, warnings=${warnings.size})"
        }
    }
}

internal data class OfflinePackageGcResult(
    val gcCount: Int,
    val warnings: List<OfflinePackagesWarning>,
)

internal suspend fun gcOfflinePackages(
    context: Context,
    plan: PackageManagementPlanWire,
    bundle: BundleManifestWire,
    retainedFilenames: Set<String>,
    onProgress: suspend (String) -> Unit = {},
): OfflinePackageGcResult {
    val packagesById = bundle.packages.associateBy { it.id }
    val installedByFilename = listInstalledPackageArtifacts(context).associateBy { it.filename }
    val warnings = mutableListOf<OfflinePackagesWarning>()
    var gcCount = 0
    plan.gc.forEachIndexed { index, filename ->
        currentCoroutineContext().ensureActive()
        if (filename in retainedFilenames) {
            diagnosticLogInfo("OfflinePackages") { "gc retained active artifact $filename" }
            return@forEachIndexed
        }
        runCatching {
            val gcStartMs = SystemClock.elapsedRealtime()
            onProgress("Removing package ${index + 1}/${plan.gc.size}: $filename")
            val installedArtifact = installedByFilename[filename]
            if (installedArtifact == null) {
                diagnosticLogInfo("OfflinePackages") {
                    "gc artifact already absent after interrupted cleanup: $filename"
                }
                return@runCatching
            }
            val keepFilename = packagesById[installedArtifact.artifactId]?.filename
                ?.takeIf { plan.fetch.contains(installedArtifact.artifactId) }
            deleteInstalledArtifact(context, installedArtifact.artifactId, filename, keepFilename)
            gcCount += 1
            diagnosticLogInfo("OfflinePackages") {
                "gc removed $filename in ${SystemClock.elapsedRealtime() - gcStartMs}ms keep=$keepFilename"
            }
        }.onFailure {
            if (it is CancellationException) throw it
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
    return OfflinePackageGcResult(gcCount = gcCount, warnings = warnings)
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
internal const val PackageHttpReadTimeoutMs = 30_000

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
    val currentArtifactsJsons = discoveryUrls.map { discoveryUrl ->
        currentCoroutineContext().ensureActive()
        readPackageSourceText(
            discoveryUrl,
            activeConnections,
        )
    }
    val discoveryPlans = currentArtifactsJsons.map { currentArtifactsJson ->
        val input = CurrentArtifactsDiscoveryInputWire(
            publicationRootUrl = publicationRootUrl,
            currentArtifactsJson = currentArtifactsJson,
        )
        PackageManagementJson.decodeFromString<CurrentArtifactsDiscoveryPlanWire>(
            NativeBindings.planCurrentArtifactsDiscoveryJson(PackageManagementJson.encodeToString(input)),
        )
    }
    val discoveryJsons = discoveryPlans.flatMap { it.discoveryJsons }
    val bundleRefsByFilename = discoveryPlans
        .flatMap { plan ->
            plan.bundleRequests.map { request -> request.filename to request.url }
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
            diagnosticLogInfo("OfflinePackages") { "cancel disconnect $sourceUrl" }
            connection.disconnect()
        }
    }
    return try {
        diagnosticLogInfo("OfflinePackages") { "http read start $sourceUrl" }
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
        diagnosticLogInfo("OfflinePackages") {
            "http read end bytes=$totalBytesRead elapsedMs=${SystemClock.elapsedRealtime() - startMs} url=$sourceUrl"
        }
    }
}

internal fun openCancellablePackageConnection(sourceUrl: String): HttpURLConnection =
    (URL(sourceUrl).openConnection() as HttpURLConnection).apply {
        connectTimeout = PackageHttpConnectTimeoutMs
        readTimeout = PackageHttpReadTimeoutMs
        instanceFollowRedirects = true
        useCaches = false
    }

internal fun openCancellablePackageConnection(
    sourceUrl: String,
    resumeOffsetBytes: Long,
): HttpURLConnection =
    openCancellablePackageConnection(sourceUrl).apply {
        if (resumeOffsetBytes > 0L) {
            setRequestProperty("Range", "bytes=$resumeOffsetBytes-")
        }
    }

private fun downloadTimingMs(nanos: Long): Long = nanos / 1_000_000L

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

internal class PackageHttpStatusException(
    val statusCode: Int,
    sourceUrl: String,
) : IOException("HTTP $statusCode fetching $sourceUrl")

internal suspend fun downloadPackageWithRetries(
    context: Context,
    filename: String,
    sourceUrl: String,
    expectedSizeBytes: Long?,
    expectedSha256: String?,
    activeConnections: ActivePackageConnections,
    onBytesRead: suspend (Long) -> Unit = {},
): File {
    var attempt = 1
    while (true) {
        try {
            return downloadPackageToTempFile(
                context = context,
                filename = filename,
                sourceUrl = sourceUrl,
                expectedSizeBytes = expectedSizeBytes,
                expectedSha256 = expectedSha256,
                activeConnections = activeConnections,
                onBytesRead = onBytesRead,
            )
        } catch (error: CancellationException) {
            throw error
        } catch (error: Throwable) {
            if (attempt >= PackageDownloadMaxAttempts || !packageDownloadErrorIsRetryable(error)) {
                throw error
            }
            diagnosticLogInfo("OfflinePackages") {
                "retrying package download filename=$filename attempt=${attempt + 1}/$PackageDownloadMaxAttempts after ${error::class.simpleName}: ${error.message}"
            }
            delay(PackageDownloadRetryBaseDelayMs * attempt)
            attempt += 1
        }
    }
}

internal fun packageDownloadErrorIsRetryable(error: Throwable): Boolean =
    when (error) {
        is PackageHttpStatusException ->
            error.statusCode == 408 || error.statusCode == 429 || error.statusCode >= 500
        is IOException -> true
        else -> false
    }

internal suspend fun downloadPackageToTempFile(
    context: Context,
    filename: String,
    sourceUrl: String,
    expectedSizeBytes: Long?,
    expectedSha256: String?,
    activeConnections: ActivePackageConnections,
    onBytesRead: suspend (Long) -> Unit = {},
): File {
    val target = InstalledPackages.internalPackageFile(context, filename)
    target.parentFile?.mkdirs()
    val temp = File(target.parentFile, "${target.name}.download")
    var resumeOffsetBytes = temp.length().coerceAtLeast(0L)
    if (temp.isFile && expectedSizeBytes != null && resumeOffsetBytes >= expectedSizeBytes) {
        check(temp.delete()) { "failed to discard completed staging file ${temp.absolutePath}" }
        resumeOffsetBytes = 0L
    }
    val digest = MessageDigest.getInstance("SHA-256")
    var sizeBytes = 0L
    var complete = false
    var responseCode: Int? = null
    var responseContentLength: Long? = null
    var readNanos = 0L
    var writeNanos = 0L
    var digestNanos = 0L
    var progressNanos = 0L
    var progressCallbacks = 0L
    val downloadStartNanos = SystemClock.elapsedRealtimeNanos()
    val connection = openCancellablePackageConnection(sourceUrl, resumeOffsetBytes)
    activeConnections.add(connection)
    val completionHandle = currentCoroutineContext()[Job]?.invokeOnCompletion { error ->
        if (error is CancellationException) {
            diagnosticLogInfo("OfflinePackages") { "cancel disconnect $sourceUrl" }
            connection.disconnect()
        }
    }
    try {
        val responseStartNanos = SystemClock.elapsedRealtimeNanos()
        responseCode = connection.responseCode
        responseContentLength = connection.contentLengthLong.takeIf { it >= 0L }
        if (responseCode !in 200..299) {
            throw PackageHttpStatusException(responseCode, sourceUrl)
        }
        val appendToPartial = resumeOffsetBytes > 0L &&
            responseCode == HttpURLConnection.HTTP_PARTIAL &&
            contentRangeStartsAt(connection.getHeaderField("Content-Range"), resumeOffsetBytes)
        if (appendToPartial) {
            temp.inputStream().buffered().use { input ->
                val buffer = ByteArray(64 * 1024)
                while (true) {
                    val read = input.read(buffer)
                    if (read < 0) break
                    digest.update(buffer, 0, read)
                }
            }
            sizeBytes = resumeOffsetBytes
            onBytesRead(sizeBytes)
        } else {
            resumeOffsetBytes = 0L
        }
        diagnosticLogInfo("OfflinePackages") {
            "http download start $sourceUrl response=$responseCode contentLength=${responseContentLength ?: "unknown"} resume=$resumeOffsetBytes setupMs=${downloadTimingMs(SystemClock.elapsedRealtimeNanos() - responseStartNanos)}"
        }
        connection.inputStream.buffered().use { input ->
            BufferedOutputStream(FileOutputStream(temp, appendToPartial)).use { output ->
                val buffer = ByteArray(64 * 1024)
                while (true) {
                    currentCoroutineContext().ensureActive()
                    val readStartNanos = SystemClock.elapsedRealtimeNanos()
                    val read = input.read(buffer)
                    readNanos += SystemClock.elapsedRealtimeNanos() - readStartNanos
                    currentCoroutineContext().ensureActive()
                    if (read < 0) {
                        break
                    }
                    val writeStartNanos = SystemClock.elapsedRealtimeNanos()
                    output.write(buffer, 0, read)
                    writeNanos += SystemClock.elapsedRealtimeNanos() - writeStartNanos
                    val digestStartNanos = SystemClock.elapsedRealtimeNanos()
                    digest.update(buffer, 0, read)
                    digestNanos += SystemClock.elapsedRealtimeNanos() - digestStartNanos
                    sizeBytes += read.toLong()
                    val progressStartNanos = SystemClock.elapsedRealtimeNanos()
                    onBytesRead(sizeBytes)
                    progressNanos += SystemClock.elapsedRealtimeNanos() - progressStartNanos
                    progressCallbacks += 1
                }
            }
        }
        complete = true
    } finally {
        completionHandle?.dispose()
        activeConnections.remove(connection)
        connection.disconnect()
        diagnosticLogInfo("OfflinePackages") { "http download end $sourceUrl complete=$complete" }
        diagnosticLogInfo("OfflinePackages") {
            "http download stats filename=$filename bytes=$sizeBytes complete=$complete totalMs=${downloadTimingMs(SystemClock.elapsedRealtimeNanos() - downloadStartNanos)} readMs=${downloadTimingMs(readNanos)} writeMs=${downloadTimingMs(writeNanos)} digestMs=${downloadTimingMs(digestNanos)} progressMs=${downloadTimingMs(progressNanos)} progressCallbacks=$progressCallbacks response=$responseCode contentLength=${responseContentLength ?: "unknown"} url=$sourceUrl"
        }
    }
    expectedSizeBytes?.let { expected ->
        if (sizeBytes != expected) {
            temp.delete()
            error("size mismatch for $filename: expected $expected got $sizeBytes")
        }
    }
    expectedSha256?.let { expected ->
        val actual = digest.digest().joinToString("") { "%02x".format(it) }
        if (!actual.equals(expected, ignoreCase = true)) {
            temp.delete()
            error("checksum mismatch for $filename: expected $expected got $actual")
        }
    }
    return temp
}

internal const val PackageDownloadMaxAttempts = 3
internal const val PackageDownloadRetryBaseDelayMs = 1_000L

internal fun contentRangeStartsAt(
    contentRange: String?,
    expectedOffsetBytes: Long,
): Boolean {
    val start = contentRange
        ?.trim()
        ?.removePrefix("bytes ")
        ?.substringBefore('-')
        ?.toLongOrNull()
    return start == expectedOffsetBytes
}

internal fun installDownloadedPackage(
    context: Context,
    artifactId: String,
    filename: String,
    tempFile: File,
    sizeBytes: Long?,
    checksumSha256: String?,
) {
    tempFile.inputStream().buffered().use { source ->
        InstalledPackages.replaceInstalledFileFromStream(
            context = context,
            artifactId = artifactId,
            filename = filename,
            source = source,
            sizeBytes = sizeBytes,
            checksumSha256 = checksumSha256,
        )
    }
    tempFile.delete()
}

internal fun resolvePackageSourceUrl(relativePath: String, packageSourceBaseUrl: String): String =
    when {
        relativePath.startsWith("http://") || relativePath.startsWith("https://") -> relativePath
        packageSourceBaseUrl.endsWith("/") -> "$packageSourceBaseUrl$relativePath"
        else -> "$packageSourceBaseUrl/$relativePath"
    }

internal fun deleteInstalledArtifact(
    context: Context,
    artifactId: String,
    filename: String,
    keepFilename: String? = null,
) {
    InstalledPackages.deleteInstalledArtifact(context, artifactId, filename, keepFilename)
}

internal fun sha256Hex(bytes: ByteArray): String =
    MessageDigest.getInstance("SHA-256")
        .digest(bytes)
        .joinToString("") { "%02x".format(it) }
