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


internal fun aviationColor(uiTheme: UiTheme, colorKey: String): Color = when (colorKey) {
    "class_c_magenta", "magenta" -> uiTheme.aviation.classCMagenta
    "class_b_d_blue", "blue" -> uiTheme.aviation.classBDBlue
    "tfr_red", "red" -> uiTheme.aviation.tfrRed
    "intersection_cyan", "cyan" -> uiTheme.aviation.intersectionCyan
    "dark_gray" -> uiTheme.aviation.darkGray
    else -> uiTheme.aviation.classBDBlue
}

internal fun airspacePath(subpath: org.aerobag.app.domain.AirspaceDisplaySubpath): Path =
    Path().apply {
        val first = subpath.points.firstOrNull() ?: return@apply
        moveTo(first.x.toFloat(), first.y.toFloat())
        subpath.points.drop(1).forEach { point -> lineTo(point.x.toFloat(), point.y.toFloat()) }
        if (subpath.closed) {
            close()
        }
    }

internal fun strokeCapFor(lineCap: String): StrokeCap = when (lineCap) {
    "butt" -> StrokeCap.Butt
    "square" -> StrokeCap.Square
    else -> StrokeCap.Round
}

internal fun androidx.compose.ui.graphics.drawscope.DrawScope.drawAirspaceDisplayPath(
    uiTheme: UiTheme,
    feature: AirspaceDisplayPath,
    densityScale: Float = 1f,
) {
    feature.paths.forEach { subpath ->
        val path = airspacePath(subpath)
        if (subpath.closed && feature.style.fillOpacity > 0.0) {
            drawPath(
                path = path,
                color = aviationColor(uiTheme, feature.style.fillColorKey).copy(alpha = feature.style.fillOpacity.toFloat()),
            )
        }
        feature.style.strokes.forEach { stroke ->
            drawPath(
                path = path,
                color = aviationColor(uiTheme, stroke.colorKey),
                style = Stroke(
                    width = stroke.widthPx.toFloat() * densityScale,
                    cap = strokeCapFor(stroke.lineCap),
                    pathEffect = stroke.dashPx.takeIf { it.isNotEmpty() }?.let { dash ->
                        PathEffect.dashPathEffect(dash.map { it.toFloat() * densityScale }.toFloatArray())
                    },
                ),
            )
        }
    }
    feature.decorations.forEach { decoration ->
        decoration.paths.forEach { subpath ->
            drawPath(
                path = airspacePath(subpath),
                color = aviationColor(uiTheme, decoration.colorKey),
                style = Stroke(width = decoration.widthPx.toFloat() * densityScale, cap = strokeCapFor(decoration.lineCap)),
            )
        }
        decoration.segments.forEach { segment ->
            drawLine(
                color = aviationColor(uiTheme, decoration.colorKey),
                start = Offset(segment.x1.toFloat(), segment.y1.toFloat()),
                end = Offset(segment.x2.toFloat(), segment.y2.toFloat()),
                strokeWidth = decoration.widthPx.toFloat() * densityScale,
                cap = strokeCapFor(decoration.lineCap),
            )
        }
    }
}

internal fun androidx.compose.ui.graphics.drawscope.DrawScope.drawAirspaceDisplayPathContrast(
    feature: AirspaceDisplayPath,
    densityScale: Float = 1f,
) {
    feature.paths.forEach { subpath ->
        drawPath(
            path = airspacePath(subpath),
            color = Color.White,
            style = Stroke(
                width = 9f * densityScale,
                cap = StrokeCap.Round,
                join = StrokeJoin.Round,
            ),
        )
    }
}

internal fun androidx.compose.ui.graphics.drawscope.DrawScope.drawAirspaceLimitGlyph(
    uiTheme: UiTheme,
    glyph: AirspaceLimitGlyph,
    center: Offset,
    scale: Float,
) {
    val color = aviationColor(uiTheme, glyph.colorKey)
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

internal fun metarColor(category: String): Color = when (category.lowercase()) {
    "vfr" -> Color(0xFF26C85A)
    "mvfr" -> Color(0xFF2D8CFF)
    "ifr" -> Color(0xFFE03131)
    "lifr" -> Color(0xFFFF4FD8)
    else -> Color(0xFF9AA6AE)
}

internal fun androidx.compose.ui.graphics.drawscope.DrawScope.drawMetarSymbol(
    feature: VisibleMetarFeature,
    center: Offset,
    densityScale: Float,
    uiTheme: UiTheme,
) {
    val fillColor = metarColor(feature.flightCategory)
    val layers = when (feature.ceilingAmount.lowercase()) {
        "few" -> metarFewSymbol(center, densityScale)
        "sct" -> metarSctSymbol(center, densityScale)
        "bkn" -> metarBknSymbol(center, densityScale)
        "ovc" -> metarOvcSymbol(center, densityScale)
        "missing" -> metarMissingSymbol(center, densityScale)
        else -> metarClearSymbol(center, densityScale)
    }
    layers.forEach { layer ->
        drawNavSymbolLayer(layer, densityScale, uiTheme, mapOf("metar_category" to fillColor))
    }
}

internal fun pirepColor(symbol: String): Color = when (symbol.lowercase()) {
    "light-turbulence" -> Color(0xFFE9BE5E)
    "moderate-turbulence" -> Color(0xFFE79347)
    "severe-turbulence" -> Color(0xFFD24700)
    "light-icing" -> Color(0xFF64C6E9)
    "moderate-icing" -> Color(0xFF3C7EE0)
    "severe-icing" -> Color(0xFF0018E0)
    else -> Color(0xFF071015)
}

internal fun androidx.compose.ui.graphics.drawscope.DrawScope.drawPirepSymbol(
    feature: VisiblePirepFeature,
    center: Offset,
    densityScale: Float,
    uiTheme: UiTheme,
    symbolScale: Float = 1f,
) {
    val scale = densityScale * symbolScale
    val glyphColor = pirepColor(feature.symbol)
    val layers = when (feature.symbol.lowercase()) {
        "light-turbulence" -> pirepLightTurbulenceSymbol(center, scale)
        "moderate-turbulence" -> pirepModerateTurbulenceSymbol(center, scale)
        "severe-turbulence" -> pirepSevereTurbulenceSymbol(center, scale)
        "light-icing" -> pirepLightIcingSymbol(center, scale)
        "moderate-icing" -> pirepModerateIcingSymbol(center, scale)
        "severe-icing" -> pirepSevereIcingSymbol(center, scale)
        else -> pirepGenericSymbol(center, scale)
    }
    layers.forEach { layer ->
        drawNavSymbolLayer(layer, scale, uiTheme, mapOf("pirep_symbol" to glyphColor))
    }
}
