package org.aerobag.app

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.IntSize
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import org.aerobag.app.domain.FlightDataBannerModel
import org.aerobag.app.domain.FlightDataCell
import org.aerobag.app.domain.UiTheme
import kotlin.math.ceil
import kotlin.math.floor
import kotlin.math.max
import kotlin.math.min

private val FlightDataCellWidth = ThumbSize * 1.28f
private val FlightDataCellHeight = ThumbSize * 0.64f
private val FlightDataGap = ThumbSize * 0.06f
private val FlightDataTopNormal = ThumbSize + (ThumbGap * 2.4f)
private val FlightDataEdgeTopNormal = ThumbSize * 0.72f
private val FlightDataBottomReserve = ThumbSize * 1.25f

@Composable
internal fun FlightDataBanner(
    banner: FlightDataBannerModel,
    surfaceSize: IntSize,
    situationDockTopPadding: Dp,
    uiTheme: UiTheme,
    modifier: Modifier = Modifier,
) {
    val cells = banner.cells
    if (cells.isEmpty() || surfaceSize.width <= 0 || surfaceSize.height <= 0) {
        return
    }
    val density = LocalDensity.current
    val surfaceWidthDp = with(density) { surfaceSize.width.toDp().value }
    val surfaceHeightDp = with(density) { surfaceSize.height.toDp().value }
    val edgeLayout = surfaceSize.width > surfaceSize.height
    val topAfterSituationDock = situationDockTopPadding + MenuDockStyle.Situation.buttonHeight + ThumbGap
    val edgeTopPadding = maxDp(FlightDataEdgeTopNormal, topAfterSituationDock)
    val topPadding = maxDp(FlightDataTopNormal, topAfterSituationDock)
    val edgeColumnCount = remember(surfaceHeightDp, cells.size, edgeTopPadding) {
        if (edgeLayout) {
            flightDataEdgeColumnCount(surfaceHeightDp, cells.size, edgeTopPadding)
        } else {
            1
        }
    }
    if (edgeLayout) {
        val rows = cells.chunked(ceil(cells.size.toDouble() / edgeColumnCount.toDouble()).toInt().coerceAtLeast(1))
        Row(
            modifier = modifier
                .padding(
                    top = edgeTopPadding,
                    end = ThumbGap,
                    bottom = FlightDataBottomReserve,
                ),
            horizontalArrangement = Arrangement.spacedBy(FlightDataGap),
        ) {
            rows.forEach { columnCells ->
                Column(verticalArrangement = Arrangement.spacedBy(FlightDataGap)) {
                    columnCells.forEach { cell ->
                        FlightDataBannerCell(cell, uiTheme)
                    }
                }
            }
        }
    } else {
        val columnsPerRow = remember(surfaceWidthDp) {
            max(
                1,
                floor((surfaceWidthDp - (ThumbGap.value * 2f)) / (FlightDataCellWidth.value + FlightDataGap.value)).toInt(),
            )
        }
        Column(
            modifier = modifier
                .padding(
                    top = topPadding,
                    start = ThumbGap,
                    end = ThumbGap,
                ),
            horizontalAlignment = Alignment.CenterHorizontally,
            verticalArrangement = Arrangement.spacedBy(FlightDataGap),
        ) {
            cells.chunked(columnsPerRow).forEach { rowCells ->
                Row(horizontalArrangement = Arrangement.spacedBy(FlightDataGap)) {
                    rowCells.forEach { cell ->
                        FlightDataBannerCell(cell, uiTheme)
                    }
                }
            }
        }
    }
}

private fun flightDataEdgeColumnCount(
    surfaceHeightDp: Float,
    cellCount: Int,
    topPadding: Dp,
): Int {
    if (cellCount <= 0 || surfaceHeightDp <= 0f) {
        return 1
    }
    val topReserve = topPadding.value
    val bottomReserve = FlightDataBottomReserve.value
    val availableHeight = max(ThumbSize.value, surfaceHeightDp - topReserve - bottomReserve)
    val rowsPerColumn = max(1, floor((availableHeight + FlightDataGap.value) / (FlightDataCellHeight.value + FlightDataGap.value)).toInt())
    return min(3, max(1, ceil(cellCount.toDouble() / rowsPerColumn.toDouble()).toInt()))
}

private fun maxDp(left: Dp, right: Dp): Dp =
    if (left.value >= right.value) left else right

@Composable
private fun FlightDataBannerCell(
    cell: FlightDataCell,
    uiTheme: UiTheme,
) {
    val shape = RoundedCornerShape(ThumbRadius * 0.38f)
    Box(
        modifier = Modifier
            .width(FlightDataCellWidth)
            .height(FlightDataCellHeight)
            .clip(shape)
            .background(uiTheme.controls.flightDataBg)
            .border(1.dp, uiTheme.controls.flightDataBorder, shape)
            .padding(horizontal = ThumbSize * 0.08f, vertical = ThumbSize * 0.055f),
        contentAlignment = Alignment.Center,
    ) {
        Column(
            horizontalAlignment = Alignment.CenterHorizontally,
            verticalArrangement = Arrangement.spacedBy(ThumbSize * 0.01f),
        ) {
            Text(
                text = cell.label,
                color = uiTheme.controls.flightDataLabel,
                fontSize = 9.sp,
                fontWeight = FontWeight.ExtraBold,
                lineHeight = 9.sp,
                textAlign = TextAlign.Center,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
            Text(
                text = cell.value ?: "\u2014",
                color = if (cell.value == null) uiTheme.controls.flightDataMissingValue else uiTheme.controls.flightDataValue,
                fontSize = 19.sp,
                fontWeight = FontWeight.Black,
                lineHeight = 18.sp,
                textAlign = TextAlign.Center,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
        }
    }
}
