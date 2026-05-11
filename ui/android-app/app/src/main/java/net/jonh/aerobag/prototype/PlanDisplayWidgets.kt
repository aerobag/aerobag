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
internal fun PlanHeaderRow() {
    Row(horizontalArrangement = Arrangement.spacedBy(PlanGridGap)) {
        PlanCell("Waypoint", Modifier.width(ThumbSize * 2.5f), isHeader = true)
        PlanCell("Dist (nm)", Modifier.weight(1f), isHeader = true)
        PlanCell("ETE (h:m)", Modifier.weight(1f), isHeader = true)
        PlanCell("Course (°)", Modifier.weight(1f), isHeader = true)
    }
}

internal fun buildFlightPlanDisplayRows(planUiState: FlightPlanUiState): List<FlightPlanDisplayRow> =
    planUiState.displayRows.mapIndexed { index, row ->
        FlightPlanDisplayRow(
            id = row.uid,
            selectionKey = selectionKeyForDisplayRow(row, index),
            label = row.label,
            rowKind =
                when (row.rowKind) {
                    FlightPlanDisplayRowKind.Waypoint -> "waypoint"
                    FlightPlanDisplayRowKind.Group -> "group"
                    FlightPlanDisplayRowKind.Discontinuity -> "discontinuity"
                },
            componentKind = row.componentKind,
            componentUid = row.componentUid,
            componentIndex = row.componentIndex,
            procedureId = row.procedureId,
            procedureKind = row.procedureKind,
            legIndex = row.legIndex,
            distanceNm = row.distanceNm,
            courseDeg = row.courseDeg,
            etaText = row.etaText,
            legTimeText = row.legTimeText,
            fuelGalText = row.fuelGalText,
            showPlateTargetId = row.showPlateTargetId,
            chartAirportId = row.chartAirportId,
            navRef = row.navRef,
            symbolFeature = row.symbolFeature,
            depth = row.depth,
            active = row.active,
            enabled = row.enabled,
            syntheticDirectTo = row.syntheticDirectTo,
            canAddAirwayAfter = row.canAddAirwayAfter,
            canAddProcedureBefore = row.canAddProcedureBefore,
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

internal fun selectionKeyForDisplayRow(row: FlightPlanDisplayRowUiView, index: Int): String =
    row.uid.ifEmpty { "row:$index" }

internal fun buildFlightPlanDisplayBlocks(rows: List<FlightPlanDisplayRow>): List<FlightPlanDisplayBlock> {
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

internal fun navRefLabel(ref: NavRef): String = when (ref) {
    is NavRef.Airport -> ref.code
    is NavRef.Navaid -> ref.code
    is NavRef.ArincNavaid -> ref.identifier
    is NavRef.TerminalNavaid -> ref.identifier
    is NavRef.Fix -> ref.code
    is NavRef.LatLon -> "${"%.3f".format(ref.lat)},${"%.3f".format(ref.lon)}"
    is NavRef.Spot -> "SPOT ${"%.3f".format(ref.lat)},${"%.3f".format(ref.lon)}"
}
