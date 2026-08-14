// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
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
import androidx.compose.ui.draw.drawWithContent
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.IntSize
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import org.aerobag.app.domain.FlightDataBannerModel
import org.aerobag.app.domain.FlightDataCell
import org.aerobag.app.domain.UiSettingsGridItem
import org.aerobag.app.domain.UiTheme
import kotlin.math.ceil
import kotlin.math.floor
import kotlin.math.max
import kotlin.math.min

internal val FlightDataCellMinWidth = ThumbSize * 1.85f
private val FlightDataCellMinHeight = ThumbSize * 1.02f
private val FlightDataGap = ThumbSize * 0.06f
private val FlightDataTextGap = ThumbSize * 0.025f
private val FlightDataCellVerticalPadding = ThumbSize * 0.09f
private val FlightDataTopNormal = ThumbSize + (ThumbGap * 2.4f)
private val FlightDataEdgeTopNormal = ThumbSize * 0.72f
private val FlightDataBottomReserve = ThumbSize * 1.25f
private val FlightDataLabelFontSize = 14.sp
private val FlightDataLabelLineHeight = 15.sp
private val FlightDataValueFontSize = 30.sp
private val FlightDataValueLineHeight = 31.sp

@Composable
internal fun FlightDataBanner(
    banner: FlightDataBannerModel,
    surfaceSize: IntSize,
    situationDockTopPadding: Dp,
    uiTheme: UiTheme,
    modifier: Modifier = Modifier,
    onAction: (String) -> Unit,
) {
    val cells = banner.cells
    if (cells.isEmpty() || surfaceSize.width <= 0 || surfaceSize.height <= 0) {
        return
    }
    val density = LocalDensity.current
    val surfaceWidthDp = with(density) { surfaceSize.width.toDp().value }
    val surfaceHeightDp = with(density) { surfaceSize.height.toDp().value }
    val cellHeight = remember(density.fontScale) {
        flightDataCellHeight(density)
    }
    val edgeLayout = surfaceSize.width > surfaceSize.height
    val topAfterSituationDock = situationDockTopPadding + MenuDockStyle.Situation.buttonHeight + ThumbGap
    val edgeTopPadding = maxDp(FlightDataEdgeTopNormal, topAfterSituationDock)
    val topPadding = maxDp(FlightDataTopNormal, topAfterSituationDock)
    val edgeColumnCount = remember(surfaceHeightDp, cells.size, edgeTopPadding, cellHeight) {
        if (edgeLayout) {
            flightDataEdgeColumnCount(surfaceHeightDp, cells.size, edgeTopPadding, cellHeight)
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
                        FlightDataBannerCell(
                            cell,
                            uiTheme,
                            FlightDataCellMinWidth,
                            cellHeight,
                            modifier = cell.actionId?.let { actionId ->
                                Modifier.clickable { onAction(actionId) }
                            } ?: Modifier,
                        )
                    }
                }
            }
        }
    } else {
        val availableWidthDp = max(FlightDataCellMinWidth.value, surfaceWidthDp - (ThumbGap.value * 2f))
        val columnsPerRow = remember(availableWidthDp) {
            max(
                1,
                floor((availableWidthDp + FlightDataGap.value) / (FlightDataCellMinWidth.value + FlightDataGap.value)).toInt(),
            )
        }
        val cellWidth = remember(availableWidthDp, columnsPerRow) {
            ((availableWidthDp - (FlightDataGap.value * (columnsPerRow - 1).coerceAtLeast(0))) / columnsPerRow)
                .coerceAtLeast(FlightDataCellMinWidth.value)
                .dp
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
                Row(
                    modifier = Modifier.fillMaxWidth(),
                    horizontalArrangement = Arrangement.spacedBy(FlightDataGap, Alignment.CenterHorizontally),
                ) {
                    rowCells.forEach { cell ->
                        FlightDataBannerCell(
                            cell,
                            uiTheme,
                            cellWidth,
                            cellHeight,
                            modifier = cell.actionId?.let { actionId ->
                                Modifier.clickable { onAction(actionId) }
                            } ?: Modifier,
                        )
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
    cellHeight: Dp,
): Int {
    if (cellCount <= 0 || surfaceHeightDp <= 0f) {
        return 1
    }
    val topReserve = topPadding.value
    val bottomReserve = FlightDataBottomReserve.value
    val availableHeight = max(ThumbSize.value, surfaceHeightDp - topReserve - bottomReserve)
    val rowsPerColumn = max(1, floor((availableHeight + FlightDataGap.value) / (cellHeight.value + FlightDataGap.value)).toInt())
    return min(3, max(1, ceil(cellCount.toDouble() / rowsPerColumn.toDouble()).toInt()))
}

private fun maxDp(left: Dp, right: Dp): Dp =
    if (left.value >= right.value) left else right

private fun flightDataCellHeight(density: androidx.compose.ui.unit.Density): Dp {
    val textHeight = with(density) {
        (FlightDataLabelLineHeight.toPx() + FlightDataValueLineHeight.toPx()).toDp()
    }
    return maxDp(
        FlightDataCellMinHeight,
        textHeight + FlightDataTextGap + (FlightDataCellVerticalPadding * 2f),
    )
}

@Composable
internal fun FlightDataSettingsCell(
    item: UiSettingsGridItem,
    modifier: Modifier = Modifier,
    onClick: () -> Unit,
) {
    val uiTheme = LocalAerobagUiTheme.current
    val density = LocalDensity.current
    val cellHeight = remember(density.fontScale) { flightDataCellHeight(density) }
    FlightDataBannerCell(
        cell = item.cell,
        uiTheme = uiTheme,
        cellWidth = FlightDataCellMinWidth,
        cellHeight = cellHeight,
        modifier = modifier.clickable(onClick = onClick),
        foregroundOverlay = if (item.enabled) {
            Color.Transparent
        } else {
            uiTheme.controls.buttonDisabled.copy(alpha = 0.4f)
        },
    )
}

@Composable
private fun FlightDataBannerCell(
    cell: FlightDataCell,
    uiTheme: UiTheme,
    cellWidth: Dp,
    cellHeight: Dp,
    modifier: Modifier = Modifier,
    background: Brush = SolidColor(uiTheme.controls.flightDataBg),
    foregroundOverlay: Color = Color.Transparent,
) {
    val shape = RoundedCornerShape(ThumbRadius * 0.38f)
    val labelStyle = buttonLabelStyle().copy(
        fontSize = FlightDataLabelFontSize,
        fontWeight = FontWeight.ExtraBold,
        lineHeight = FlightDataLabelLineHeight,
    )
    val valueStyle = buttonLabelStyle().copy(
        fontSize = FlightDataValueFontSize,
        fontWeight = FontWeight.Black,
        lineHeight = FlightDataValueLineHeight,
    )
    Box(
        modifier = modifier
            .width(cellWidth)
            .height(cellHeight)
            .clip(shape)
            .background(background)
            .drawWithContent {
                drawContent()
                if (foregroundOverlay.alpha > 0f) {
                    drawRect(foregroundOverlay)
                }
            }
            .border(1.dp, uiTheme.controls.flightDataBorder, shape)
            .padding(horizontal = ThumbSize * 0.08f, vertical = FlightDataCellVerticalPadding),
        contentAlignment = Alignment.Center,
    ) {
        Column(
            horizontalAlignment = Alignment.CenterHorizontally,
            verticalArrangement = Arrangement.spacedBy(FlightDataTextGap),
        ) {
            FittedSingleLineText(
                text = cell.label,
                color = uiTheme.controls.flightDataLabel,
                style = labelStyle,
                textAlign = TextAlign.Center,
                minFontSize = 9.5.sp,
                modifier = Modifier.fillMaxWidth(),
            )
            Text(
                text = cell.value ?: "\u2014",
                color = if (cell.value == null) uiTheme.controls.flightDataMissingValue else uiTheme.controls.flightDataValue,
                style = valueStyle,
                textAlign = TextAlign.Center,
                maxLines = 1,
                softWrap = false,
                modifier = Modifier.fillMaxWidth(),
            )
        }
    }
}
