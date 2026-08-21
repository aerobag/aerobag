// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.selection.toggleable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items as lazyColumnItems
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Checkbox
import androidx.compose.material3.Slider
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.zIndex
import kotlin.math.floor
import kotlin.math.roundToInt
import org.aerobag.app.domain.NavElementUiView
import org.aerobag.app.domain.UiSettingsPageRow
import org.aerobag.app.domain.UiSettingsPageSection
import org.aerobag.app.domain.UiSettingsPageState

private val SettingsPageTitleTextSize = 16.sp
private val SettingsPageRowTitleTextSize = 13.sp
private val SettingsPageStopTextSize = 12.sp
private val SettingsSliderStopLabelSlotWidth = 112.dp
private val SettingsSliderStopLabelsHeight = 18.dp

@Composable
internal fun SettingsPage(
    page: AppPage,
    state: UiSettingsPageState,
    navElement: NavElementUiView?,
    mostRecentChartOrPlatePage: AppPage,
    onOpenPlan: () -> Unit,
    onOpenRecentChartOrPlate: () -> Unit,
    onSelectPage: (AppPage) -> Unit,
    onSettingsAction: (String, String) -> Unit,
) {
    val uiTheme = LocalAerobagUiTheme.current
    Box(
        modifier = Modifier
            .fillMaxSize()
            .background(uiTheme.controls.chartSurfaceBg),
    ) {
        PrimaryNavigationDock(
            currentPage = page,
            navElement = navElement,
            chartPlateTargetPage = mostRecentChartOrPlatePage,
            onHomeClick = { onSelectPage(AppPage.Home) },
            onOpenPlan = onOpenPlan,
            onSelectPage = onSelectPage,
            onOpenChartOrPlate = onOpenRecentChartOrPlate,
            modifier = Modifier
                .align(Alignment.BottomCenter)
                .padding(bottom = ThumbGap)
                .zIndex(OverlayPlaneControls),
        )
        Column(
            modifier = Modifier
                .fillMaxSize()
                .padding(
                    start = ThumbGap,
                    end = ThumbGap,
                    top = ThumbGap,
                    bottom = ThumbSize + (ThumbGap * 2f),
                )
                .clip(RoundedCornerShape(ThumbRadius))
                .background(uiTheme.controls.buttonUnchecked.copy(alpha = 0.84f))
                .padding(ThumbSize * 0.3f),
            verticalArrangement = Arrangement.spacedBy(ThumbSize * 0.25f),
        ) {
            Text(
                text = state.title.uppercase(),
                style = MaterialTheme.typography.headlineSmall.copy(
                    fontSize = SettingsPageTitleTextSize,
                    lineHeight = SettingsPageTitleTextSize,
                    fontWeight = FontWeight.Black,
                ),
                color = uiTheme.controls.buttonFg,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
            if (state.rows.isEmpty() && state.sections.isEmpty()) {
                Text(
                    text = state.summary.ifBlank { "No settings available." },
                    style = MaterialTheme.typography.bodyLarge.copy(
                        fontSize = SettingsPageRowTitleTextSize,
                        lineHeight = SettingsPageRowTitleTextSize * 1.2f,
                        fontWeight = FontWeight.Bold,
                    ),
                    color = uiTheme.controls.buttonFg.copy(alpha = 0.78f),
                )
            } else {
                LazyColumn(
                    modifier = Modifier.fillMaxSize(),
                    verticalArrangement = Arrangement.spacedBy(ThumbSize * 0.2f),
                ) {
                    lazyColumnItems(state.rows, key = { it.id }) { row ->
                        SettingsPageRowView(row = row, onSettingsAction = onSettingsAction)
                    }
                    state.sections.forEach { section ->
                        item(key = "section:${section.id}") {
                            SettingsPageSectionView(
                                section = section,
                                onSettingsAction = onSettingsAction,
                            )
                        }
                    }
                }
            }
        }
    }
}

@Composable
private fun SettingsPageRowView(
    row: UiSettingsPageRow,
    onSettingsAction: (String, String) -> Unit,
) {
    Box(
        modifier = Modifier
            .fillMaxWidth()
            .padding(start = ThumbSize * (0.35f * row.indentLevel.toFloat())),
    ) {
        when (row.kind) {
            "grid_choices" -> SettingsGridChoicesRow(row, onSettingsAction)
            "slider" -> SettingsSliderRow(row, onSettingsAction)
            "toggle" -> SettingsToggleRow(row, onSettingsAction)
        }
    }
}

@Composable
private fun SettingsPageSectionView(
    section: UiSettingsPageSection,
    onSettingsAction: (String, String) -> Unit,
) {
    val uiTheme = LocalAerobagUiTheme.current
    var expanded by remember(section.id) { mutableStateOf(!section.collapsedByDefault) }
    Column(
        verticalArrangement = Arrangement.spacedBy(ThumbSize * 0.1f),
    ) {
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .height(ThumbSize * 0.64f)
                .clip(RoundedCornerShape(ThumbRadius * 0.65f))
                .background(uiTheme.controls.buttonUnchecked)
                .border(
                    width = 1.dp,
                    color = uiTheme.controls.panelBorder,
                    shape = RoundedCornerShape(ThumbRadius * 0.65f),
                )
                .clickable { expanded = !expanded }
                .testTag("parity:settings-section:${section.id}")
                .padding(horizontal = ThumbSize * 0.18f),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text(
                text = if (expanded) "\u25BE" else "\u25B8",
                modifier = Modifier.width(ThumbSize * 0.32f),
                color = uiTheme.controls.buttonFg,
                fontSize = SettingsPageRowTitleTextSize,
                fontWeight = FontWeight.Black,
                textAlign = TextAlign.Center,
            )
            Text(
                text = section.title.uppercase(),
                color = uiTheme.controls.buttonFg,
                fontSize = SettingsPageRowTitleTextSize,
                lineHeight = SettingsPageRowTitleTextSize,
                fontWeight = FontWeight.Black,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
        }
        if (expanded) {
            Column(verticalArrangement = Arrangement.spacedBy(ThumbSize * 0.1f)) {
                section.rows.forEach { row ->
                    SettingsPageRowView(row = row, onSettingsAction = onSettingsAction)
                }
            }
        }
    }
}

@Composable
private fun SettingsToggleRow(
    row: UiSettingsPageRow,
    onSettingsAction: (String, String) -> Unit,
) {
    val enabled = row.valueId == "on"
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .height(ThumbSize * 0.62f)
            .clip(RoundedCornerShape(ThumbRadius * 0.65f))
            .background(Color.White.copy(alpha = 0.92f))
            .toggleable(
                value = enabled,
                role = Role.Checkbox,
                onValueChange = {
                    onSettingsAction(row.actionId, if (enabled) "off" else "on")
                },
            )
            .testTag("parity:settings-toggle:${row.id}")
            .padding(horizontal = ThumbSize * 0.24f),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.SpaceBetween,
    ) {
        Text(
            text = row.title,
            modifier = Modifier.weight(1f),
            color = Color(0xFF101820),
            fontSize = SettingsPageRowTitleTextSize,
            lineHeight = SettingsPageRowTitleTextSize * 1.08f,
            fontWeight = FontWeight.Bold,
            maxLines = 2,
            overflow = TextOverflow.Ellipsis,
        )
        Checkbox(
            checked = enabled,
            onCheckedChange = null,
            modifier = Modifier.size(ThumbSize * 0.36f),
        )
    }
}

@Composable
private fun SettingsGridChoicesRow(
    row: UiSettingsPageRow,
    onSettingsAction: (String, String) -> Unit,
) {
    val gap = ThumbSize * 0.1f
    SettingsPageRowSurface(title = row.title) {
        BoxWithConstraints(modifier = Modifier.fillMaxWidth()) {
            val columnCount = floor(
                (maxWidth.value + gap.value) /
                    (FlightDataCellMinWidth.value + gap.value),
            ).toInt().coerceAtLeast(1)
            Column(verticalArrangement = Arrangement.spacedBy(gap)) {
                row.items.chunked(columnCount).forEach { items ->
                    Row(
                        modifier = Modifier.fillMaxWidth(),
                        horizontalArrangement = Arrangement.spacedBy(gap, Alignment.CenterHorizontally),
                    ) {
                        items.forEach { item ->
                            FlightDataSettingsCell(
                                item = item,
                                onClick = { onSettingsAction(row.actionId, item.cell.id) },
                            )
                        }
                    }
                }
            }
        }
    }
}

@Composable
private fun SettingsPageRowSurface(
    title: String,
    content: @Composable () -> Unit,
) {
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(ThumbRadius))
            .background(Color.White.copy(alpha = 0.92f))
            .padding(ThumbSize * 0.28f),
        verticalArrangement = Arrangement.spacedBy(ThumbSize * 0.14f),
    ) {
        Text(
            text = title,
            style = MaterialTheme.typography.titleLarge.copy(
                fontSize = SettingsPageRowTitleTextSize,
                lineHeight = SettingsPageRowTitleTextSize * 1.08f,
                fontWeight = FontWeight.Black,
            ),
            color = Color(0xFF101820),
            maxLines = 2,
            overflow = TextOverflow.Ellipsis,
        )
        content()
    }
}

@Composable
private fun SettingsSliderRow(
    row: UiSettingsPageRow,
    onSettingsAction: (String, String) -> Unit,
) {
    if (row.stops.isEmpty()) {
        return
    }
    val selectedIndex = row.stops.indexOfFirst { it.id == row.valueId }.takeIf { it >= 0 } ?: 0
    var sliderIndex by remember(row.id, row.valueId, row.stops) {
        mutableStateOf(selectedIndex.toFloat())
    }
    val maxIndex = (row.stops.size - 1).coerceAtLeast(0)
    SettingsPageRowSurface(title = row.title) {
        Box(
            modifier = Modifier.fillMaxWidth(),
            contentAlignment = Alignment.Center,
        ) {
            Column(
                modifier = if (row.stops.size == 2) {
                    Modifier.fillMaxWidth(0.4f)
                } else {
                    Modifier.fillMaxWidth()
                },
                verticalArrangement = Arrangement.spacedBy(ThumbSize * 0.14f),
            ) {
                Slider(
                    value = sliderIndex.coerceIn(0f, maxIndex.toFloat()),
                    onValueChange = { value ->
                        sliderIndex = value.roundToInt().coerceIn(0, maxIndex).toFloat()
                    },
                    onValueChangeFinished = {
                        val nextStop = row.stops.getOrNull(sliderIndex.roundToInt())
                        if (nextStop != null && nextStop.id != row.valueId) {
                            onSettingsAction(row.actionId, nextStop.id)
                        }
                    },
                    modifier = Modifier.fillMaxWidth(),
                    valueRange = 0f..maxIndex.toFloat(),
                    steps = (row.stops.size - 2).coerceAtLeast(0),
                )
                SettingsSliderStopLabels(row)
            }
        }
    }
}

@Composable
private fun SettingsSliderStopLabels(row: UiSettingsPageRow) {
    val maxIndex = (row.stops.size - 1).coerceAtLeast(1)
    BoxWithConstraints(
        modifier = Modifier
            .fillMaxWidth()
            .height(SettingsSliderStopLabelsHeight),
    ) {
        val slotWidth = SettingsSliderStopLabelSlotWidth.coerceAtMost(maxWidth)
        val travel = (maxWidth - slotWidth).coerceAtLeast(0.dp)
        row.stops.forEachIndexed { index, stop ->
            val fraction = index.toFloat() / maxIndex.toFloat()
            Text(
                text = stop.label,
                modifier = Modifier
                    .width(slotWidth)
                    .offset(x = travel * fraction),
                style = MaterialTheme.typography.bodySmall.copy(
                    fontSize = SettingsPageStopTextSize,
                    lineHeight = SettingsPageStopTextSize,
                    fontWeight = if (stop.id == row.valueId) FontWeight.Black else FontWeight.Bold,
                ),
                color = if (stop.id == row.valueId) {
                    Color(0xFF101820)
                } else {
                    Color(0xFF101820).copy(alpha = 0.72f)
                },
                textAlign = TextAlign.Center,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
        }
    }
}
