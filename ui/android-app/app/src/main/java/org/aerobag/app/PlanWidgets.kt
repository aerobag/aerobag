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
import androidx.compose.foundation.ScrollState
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
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.Spacer
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
import org.aerobag.app.domain.FlightPlanWeatherBadgeUiView
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
import org.aerobag.app.generated.actionSymbol as generatedActionSymbol
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
import org.aerobag.app.generated.weatherCameraSymbol
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
internal fun ActionIcon(
    actionId: String,
    enabled: Boolean,
    modifier: Modifier = Modifier,
) {
    val uiTheme = LocalAerobagUiTheme.current
    Canvas(
        modifier = modifier.alpha(
            if (enabled) 1f else uiTheme.controls.buttonDisabledIconOpacity,
        ),
    ) {
        val scale = size.minDimension / 48f
        val center = Offset(size.width / 2f, size.height / 2f)
        generatedActionSymbol(actionId, center, scale).orEmpty().forEach { layer ->
            drawNavSymbolLayer(layer, scale, uiTheme)
        }
    }
}


@Composable
internal fun PlanWaypointSymbol(
    feature: org.aerobag.app.domain.NavSymbolFeature?,
    modifier: Modifier = Modifier,
    weatherBadge: FlightPlanWeatherBadgeUiView? = null,
) {
    if (feature == null) {
        return
    }
    val uiTheme = LocalAerobagUiTheme.current
    Canvas(modifier = modifier.size(ThumbSize * 0.78f)) {
        val scale = size.minDimension / 40f
        val center = Offset(size.width / 2f, size.height / 2f)
        val fixMarkerStrokeColor = Color(0xB3081218)
        val fixMarkerFillColor = uiTheme.aviation.intersectionCyan
        val airportMarkerStrokeColor = Color(0xB3081218)
        val airportFillColor = if (feature.towered) uiTheme.aviation.classBDBlue else uiTheme.aviation.classCMagenta
        val openAirportStrokeColor = uiTheme.aviation.classCMagenta
        val vorMarkerColor = uiTheme.aviation.classBDBlue
        when (feature.symbolKind) {
            "airport" -> {
                val usesOpenAirportCircle =
                    feature.heliport == true ||
                        feature.hasWaterRunway == true ||
                        feature.hasPavedRunway == false
                if (usesOpenAirportCircle) {
                    airportOpenMarkerSymbol(center, scale).forEach { layer ->
                        drawNavSymbolLayer(layer, scale, uiTheme)
                    }
                } else if (feature.fuelAvailable) {
                    val markerPath = airportFuelMarkerPath(center, scale)
                    drawPath(markerPath, airportFillColor)
                    drawPath(markerPath, airportMarkerStrokeColor, style = Stroke(width = 2f * scale))
                } else {
                    val markerPath = airportCircleMarkerPath(center, scale)
                    drawPath(markerPath, airportFillColor)
                    drawPath(markerPath, airportMarkerStrokeColor, style = Stroke(width = 2f * scale))
                }
                if (feature.heliport == true) {
                    val heliportPath = heliportHPath(center, scale)
                    drawPath(
                        heliportPath,
                        openAirportStrokeColor,
                        style = Stroke(width = 2.4f * scale, cap = StrokeCap.Round),
                    )
                } else if (feature.hasWaterRunway == true) {
                    rotate(15f, center) {
                        val anchorPath = seaplaneAnchorPath(center, scale)
                        drawPath(
                            anchorPath,
                            openAirportStrokeColor,
                            style = Stroke(width = 2.2f * scale, cap = StrokeCap.Round),
                        )
                    }
                }
                if (!usesOpenAirportCircle) feature.longestRunwayHeadingTrueDeg?.let { heading ->
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

            "nav" -> {
                val radius = 8f * scale
                val outerHex = vorOuterHexPath(center, radius)
                val band = vorBandPath(center, radius)
                drawPath(band, vorMarkerColor)
                drawPath(band, fixMarkerStrokeColor, style = Stroke(width = 1.6f * scale))
                drawPath(outerHex, fixMarkerStrokeColor, style = Stroke(width = 1.6f * scale))
            }

            "weather_camera" -> {
                weatherCameraSymbol(center, scale).forEach { layer ->
                    drawNavSymbolLayer(layer, scale, uiTheme)
                }
            }

            "obstacle" -> {
                val isTallObstacle = feature.obstacleVariant == "tall"
                val obstaclePath = if (isTallObstacle) {
                    obstacleTallPath(center, scale)
                } else {
                    obstacleShortPath(center, scale)
                }
                val dotY = if (isTallObstacle) obstacleTallDotY else obstacleShortDotY
                val obstacleColor = obstacleToneColor(uiTheme, feature.obstacleTone)
                val obstacleUnderColor = uiTheme.aviation.obstacleUnder
                drawPath(
                    obstaclePath,
                    obstacleUnderColor,
                    style = Stroke(width = 2.4f * scale, join = StrokeJoin.Miter),
                )
                drawPath(
                    obstaclePath,
                    obstacleColor,
                    style = Stroke(width = 1.2f * scale, join = StrokeJoin.Miter),
                )
                drawCircle(
                    color = obstacleUnderColor,
                    radius = obstacleDotRadius * scale,
                    center = Offset(center.x, center.y + dotY * scale),
                )
                drawCircle(
                    color = obstacleColor,
                    radius = obstacleDotRadius * scale,
                    center = Offset(center.x, center.y + dotY * scale),
                )
            }

            else -> {
                val triangle = fixTrianglePath(center, 8f * scale)
                drawPath(triangle, fixMarkerFillColor)
                drawPath(triangle, fixMarkerStrokeColor, style = Stroke(width = 2.5f * scale))
            }
        }
        weatherBadge?.let { badge ->
            drawMetarDisc(
                flightCategory = badge.flightCategory,
                ceilingAmount = badge.ceilingAmount,
                center = Offset(center.x + 10f * scale, center.y + 10f * scale),
                densityScale = scale,
                uiTheme = uiTheme,
            )
        }
    }
}

@Composable
internal fun FlightPlanDataRow(
    row: FlightPlanDisplayRow,
    selected: Boolean,
    dataScrollState: ScrollState,
    modifier: Modifier = Modifier,
    structuredRowBounds: MutableMap<String, Rect>? = null,
    onWaypointClick: () -> Unit,
    onDataCellAction: (String) -> Unit,
) {
    val context = LocalContext.current
    val uiTheme = LocalAerobagUiTheme.current
    val targetIndent = PlanChildWaypointIndent * row.depth
    val indent by animateDpAsState(targetValue = targetIndent, label = "planRowIndent")
    val cellHeight = ThumbSize
    val rowBoundsModifier =
        if (structuredRowBounds != null) {
            rememberStructuredRowBounds(row.id, structuredRowBounds)
        } else {
            Modifier
        }
    val defaultButtonColor =
        when {
            row.rowKind == "group" -> uiTheme.controls.headerButton
            else -> uiTheme.controls.buttonUnchecked
        }
    val selectedButtonColor =
        when {
            row.active -> Color(0xFF9B3A88)
            else -> Color(
                red = uiTheme.controls.buttonUnchecked.red * 0.74f,
                green = uiTheme.controls.buttonUnchecked.green * 0.74f,
                blue = uiTheme.controls.buttonUnchecked.blue * 0.74f,
                alpha = uiTheme.controls.buttonUnchecked.alpha,
            )
        }
    val procedureGroupCell = row.rowKind == "group" && row.componentKind == RouteComponentViewKind.Procedure
    val hasWaypointSymbol = row.symbolFeature != null
    val fullWidthLabel =
        flightPlanWaypointUsesFullWidthLabel(procedureGroupCell, hasWaypointSymbol)
    PlanGridRow(
        dataColumnCount = row.dataCells.size,
        dataScrollState = dataScrollState,
        modifier = modifier.then(rowBoundsModifier),
        waypointContent = {
            Box(
                modifier =
                    Modifier
                        .width(PlanWaypointColumnWidth)
                        .height(cellHeight)
                        .then(
                            if (row.rowKind == "group" && row.procedureId != null) {
                                Modifier.testTag("parity:plan-procedure-row:${row.procedureId}:uid:${row.id}")
                            } else {
                                Modifier
                            },
                        ),
            ) {
                if (row.rowKind == "summary") {
                    PlanCell(
                        row.label,
                        modifier = Modifier
                            .height(cellHeight)
                            .width(PlanWaypointColumnWidth)
                            .align(Alignment.CenterEnd),
                        cellHeight = cellHeight,
                        muted = true,
                    )
                } else {
                    CompactSquareButton(
                        label = row.label,
                        modifier =
                            Modifier
                                .height(cellHeight)
                                .width(PlanWaypointColumnWidth - indent)
                                .align(Alignment.CenterEnd)
                                .alpha(1f),
                        testTag = "parity:plan-row:${row.id}",
                        centered = false,
                        textStartPadding = 10.dp,
                        backgroundColor = defaultButtonColor,
                        selected = selected,
                        selectedColor = selectedButtonColor,
                        enabled = row.enabled || row.syntheticDirectTo,
                        onDisabledClick = row.disabledReason?.let { reason ->
                            { showDisabledActionToast(context, reason) }
                        },
                        maxLines = if (procedureGroupCell) 3 else 2,
                        textModifier =
                            if (fullWidthLabel) {
                                Modifier.fillMaxWidth()
                            } else {
                                Modifier.padding(
                                    end = if (row.depth > 0) {
                                        PlanChildWaypointSymbolTextReserve
                                    } else {
                                        PlanWaypointSymbolTextReserve
                                    },
                                )
                            },
                        onClick = onWaypointClick,
                    )
                    if (!procedureGroupCell && hasWaypointSymbol) {
                        PlanWaypointSymbol(
                            feature = row.symbolFeature,
                            weatherBadge = row.weatherBadge,
                            modifier = Modifier
                                .align(Alignment.CenterEnd)
                                .padding(end = ThumbSize * 0.12f)
                                .then(
                                    row.weatherBadge?.let {
                                        Modifier.testTag("parity:plan-weather-badge:${it.flightCategory}")
                                    } ?: Modifier,
                                )
                                .alpha(1f),
                        )
                    }
                }
            }
        },
    ) { dataColumnWidth ->
        row.dataCells.forEach { cell ->
            PlanCell(
                cell.value ?: "—",
                Modifier
                    .width(dataColumnWidth)
                    .testTag("parity:plan-data:${row.id}:${cell.id}:${cell.value ?: "none"}")
                    .then(cell.action?.let { action ->
                        Modifier.clickable { onDataCellAction(action.actionId) }
                    } ?: Modifier),
                cellHeight = cellHeight,
                tone = cell.tone,
                estimateKind = cell.estimateKind,
            )
        }
    }
}

internal fun flightPlanWaypointUsesFullWidthLabel(
    procedureGroupCell: Boolean,
    hasWaypointSymbol: Boolean,
): Boolean = procedureGroupCell || !hasWaypointSymbol

@Composable
internal fun FlightPlanGroupBlock(
    header: FlightPlanDisplayRow,
    headerSelected: Boolean,
    dataScrollState: ScrollState,
    structuredRowBounds: MutableMap<String, Rect>? = null,
    onHeaderClick: () -> Unit,
    children: List<Pair<Int, FlightPlanDisplayRow>>,
    selectedWaypointUid: String?,
    onChildClick: (FlightPlanDisplayRow) -> Unit,
    onDataCellAction: (String) -> Unit,
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
            .padding(vertical = 8.dp),
    ) {
        FlightPlanDataRow(
            row = header,
            selected = headerSelected,
            dataScrollState = dataScrollState,
            structuredRowBounds = structuredRowBounds,
            onWaypointClick = onHeaderClick,
            onDataCellAction = onDataCellAction,
        )
        children.forEach { (_, childRow) ->
            FlightPlanDataRow(
                row = childRow,
                selected = selectedWaypointUid == childRow.id,
                dataScrollState = dataScrollState,
                structuredRowBounds = structuredRowBounds,
                onWaypointClick = { onChildClick(childRow) },
                onDataCellAction = onDataCellAction,
            )
        }
    }
}

internal val PlanWaypointColumnWidth = ThumbSize * 2f
internal val PlanChildWaypointIndent = ThumbSize * 0.2f
internal val PlanWaypointSymbolTextReserve = ThumbSize * 0.78f
internal val PlanChildWaypointSymbolTextReserve = ThumbSize * 0.54f
internal val PlanMinimumDataColumnWidth = ThumbSize

internal fun planDataColumnWidth(rowWidth: Dp, dataColumnCount: Int): Dp {
    if (dataColumnCount <= 0) return PlanMinimumDataColumnWidth
    val totalGaps = PlanGridGap * dataColumnCount
    val evenlyFilledWidth = (rowWidth - PlanWaypointColumnWidth - totalGaps) / dataColumnCount
    return maxOf(PlanMinimumDataColumnWidth, evenlyFilledWidth)
}

@Composable
internal fun PlanGridRow(
    dataColumnCount: Int,
    dataScrollState: ScrollState,
    modifier: Modifier = Modifier,
    waypointContent: @Composable () -> Unit,
    dataContent: @Composable RowScope.(Dp) -> Unit,
) {
    BoxWithConstraints(modifier = modifier.fillMaxWidth()) {
        val dataColumnWidth = planDataColumnWidth(maxWidth, dataColumnCount)
        Row(modifier = Modifier.fillMaxWidth()) {
            waypointContent()
            Spacer(modifier = Modifier.width(PlanGridGap))
            Row(
                modifier = Modifier
                    .weight(1f)
                    .horizontalScroll(dataScrollState),
                horizontalArrangement = Arrangement.spacedBy(PlanGridGap),
            ) {
                dataContent(dataColumnWidth)
            }
        }
    }
}

@Composable
internal fun PlanCell(
    value: String,
    modifier: Modifier,
    isHeader: Boolean = false,
    cellHeight: Dp? = null,
    alpha: Float = 1f,
    muted: Boolean = false,
    tone: String = "planned",
    estimateKind: String = "basic",
) {
    val uiTheme = LocalAerobagUiTheme.current
    val resolvedCellHeight = cellHeight ?: if (isHeader) ThumbSize * 0.68f else ThumbSize
    Box(
        modifier = modifier
            .height(resolvedCellHeight)
            .alpha(alpha)
            .background(uiTheme.controls.panelBg, RoundedCornerShape(ThumbRadius))
            .border(1.dp, uiTheme.controls.panelBorder, RoundedCornerShape(ThumbRadius))
            .padding(horizontal = ThumbSize * 0.12f),
        contentAlignment = if (isHeader) Alignment.CenterStart else Alignment.CenterEnd,
    ) {
        Text(
            value,
            style = if (isHeader) MaterialTheme.typography.labelMedium else MaterialTheme.typography.bodyMedium,
            color =
                when {
                    isHeader -> uiTheme.controls.panelMuted
                    tone == "active" -> uiTheme.controls.flightDataActiveValue
                    tone == "passed" -> uiTheme.controls.flightDataPassedValue
                    estimateKind == "modeled" -> uiTheme.controls.flightDataModeledValue
                    muted -> uiTheme.controls.panelFg.copy(alpha = 0.6f)
                    else -> uiTheme.controls.panelFg
                },
            fontWeight = if (isHeader) FontWeight.Bold else FontWeight.Medium,
            maxLines = 2,
            textAlign = if (isHeader) TextAlign.Start else TextAlign.End,
            overflow = TextOverflow.Ellipsis,
        )
    }
}
