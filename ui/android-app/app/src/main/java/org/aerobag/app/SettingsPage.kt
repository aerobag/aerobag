// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
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
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Checkbox
import androidx.compose.material3.OutlinedTextField
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
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.selected
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.zIndex
import kotlin.math.floor
import kotlin.math.roundToInt
import org.aerobag.app.domain.NavElementUiView
import org.aerobag.app.domain.UiAircraftLibraryAction
import org.aerobag.app.domain.UiAircraftLibraryState
import org.aerobag.app.domain.UiSettingsPageRow
import org.aerobag.app.domain.UiSettingsPageSection
import org.aerobag.app.domain.UiSettingsPageState
import org.aerobag.app.domain.UiSettingsSyncIndicator

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
    onAircraftLibraryAction: (String, String) -> Unit,
) {
    val uiTheme = LocalAerobagUiTheme.current
    Box(
        modifier = Modifier
            .fillMaxSize()
            .testTag("parity:page:settings")
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
            if (state.rows.isEmpty() && state.sections.isEmpty() && state.aircraftLibrary == null) {
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
                    state.aircraftLibrary?.let { library ->
                        item(key = "aircraft-library") {
                            SettingsAircraftLibrary(
                                state = library,
                                onAction = onAircraftLibraryAction,
                            )
                        }
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
private fun SettingsAircraftLibrary(
    state: UiAircraftLibraryState,
    onAction: (String, String) -> Unit,
) {
    val context = LocalContext.current
    val uiTheme = LocalAerobagUiTheme.current
    var sourceJson by remember(state.editor?.sourceJson) {
        mutableStateOf(state.editor?.sourceJson.orEmpty())
    }
    fun invoke(action: UiAircraftLibraryAction, source: String = "") {
        if (action.enabled) {
            onAction(action.actionId, source)
        } else {
            showDisabledActionToast(context, action.disabledReason)
        }
    }
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(ThumbRadius))
            .background(uiTheme.controls.panelBg)
            .padding(ThumbSize * 0.22f),
        verticalArrangement = Arrangement.spacedBy(ThumbSize * 0.12f),
    ) {
        Row(
            modifier = Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.SpaceBetween,
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Column(modifier = Modifier.weight(1f)) {
                Text(
                    text = state.title.uppercase(),
                    fontSize = SettingsPageRowTitleTextSize,
                    lineHeight = SettingsPageRowTitleTextSize * 1.08f,
                    fontWeight = FontWeight.Black,
                    color = uiTheme.controls.panelFg,
                )
                Text(
                    text = state.summary,
                    fontSize = SettingsPageStopTextSize,
                    lineHeight = SettingsPageStopTextSize * 1.15f,
                    fontWeight = FontWeight.Bold,
                    color = uiTheme.controls.panelFg,
                )
            }
            state.syncIndicator?.let { indicator ->
                SettingsSyncIndicatorView(indicator, "aircraft-library")
            }
            if (state.editor == null) {
                CompactSquareButton(
                    label = state.addAction.label,
                    wide = true,
                    modifier = Modifier
                        .width(ThumbSize * 1.8f)
                        .height(ThumbSize * 0.72f),
                    onClick = { invoke(state.addAction) },
                )
            }
        }
        state.entries.forEach { entry ->
            Box(
                modifier = Modifier
                    .fillMaxWidth()
                    .clip(RoundedCornerShape(ThumbRadius * 0.7f))
                    .background(uiTheme.controls.buttonUnchecked)
                    .padding(ThumbSize * 0.12f),
            ) {
                Column(verticalArrangement = Arrangement.spacedBy(ThumbSize * 0.08f)) {
                    Row(
                        modifier = Modifier.fillMaxWidth(),
                        horizontalArrangement = Arrangement.SpaceBetween,
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        Column(modifier = Modifier.weight(1f)) {
                            Text(
                                text = entry.label,
                                maxLines = 1,
                                overflow = TextOverflow.Ellipsis,
                                fontSize = SettingsPageRowTitleTextSize,
                                fontWeight = FontWeight.Black,
                                color = uiTheme.controls.buttonFg,
                            )
                            Text(
                                text = entry.sourceLabel,
                                fontSize = SettingsPageStopTextSize,
                                fontWeight = FontWeight.Black,
                                color = uiTheme.controls.buttonFg,
                            )
                        }
                        AircraftPlanViewIcon(
                            symbol = entry.symbol,
                            modifier = Modifier.size(ThumbSize * 0.72f),
                        )
                    }
                    Row(horizontalArrangement = Arrangement.spacedBy(ThumbGap)) {
                        CompactSquareButton(
                            label = entry.toggleAction.label,
                            wide = true,
                            modifier = Modifier
                                .width(ThumbSize * 1.4f)
                                .height(ThumbSize * 0.66f),
                            onClick = { invoke(entry.toggleAction) },
                        )
                        entry.editAction?.let { editAction ->
                            CompactSquareButton(
                                label = editAction.label,
                                wide = true,
                                modifier = Modifier
                                    .width(ThumbSize * 1.4f)
                                    .height(ThumbSize * 0.66f),
                                onClick = { invoke(editAction) },
                            )
                        }
                    }
                }
                if (!entry.included) {
                    Box(
                        modifier = Modifier
                            .matchParentSize()
                            .background(uiTheme.controls.buttonDisabled.copy(alpha = 0.4f)),
                    )
                }
            }
        }
        state.editor?.let { editor ->
            Text(
                text = editor.title.uppercase(),
                fontSize = SettingsPageRowTitleTextSize,
                fontWeight = FontWeight.Black,
                color = uiTheme.controls.panelFg,
            )
            OutlinedTextField(
                value = sourceJson,
                onValueChange = { sourceJson = it },
                label = { Text(editor.fieldLabel) },
                modifier = Modifier
                    .fillMaxWidth()
                    .height(ThumbSize * 7f),
                textStyle = MaterialTheme.typography.bodySmall,
            )
            editor.validationError?.let { error ->
                Text(
                    text = error,
                    color = uiTheme.controls.dataStatusWarningStroke,
                    fontSize = SettingsPageStopTextSize,
                    fontWeight = FontWeight.Black,
                )
            }
            Row(horizontalArrangement = Arrangement.spacedBy(ThumbGap)) {
                CompactSquareButton(
                    label = editor.saveAction.label,
                    wide = true,
                    modifier = Modifier
                        .width(ThumbSize * 2f)
                        .height(ThumbSize * 0.72f),
                    onClick = { invoke(editor.saveAction, sourceJson) },
                )
                CompactSquareButton(
                    label = editor.cancelAction.label,
                    wide = true,
                    modifier = Modifier
                        .width(ThumbSize * 1.6f)
                        .height(ThumbSize * 0.72f),
                    onClick = { invoke(editor.cancelAction) },
                )
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
                .e2eIndexedControl(
                    semanticTag = "parity:settings-section:${section.id}",
                    state = "enabled:true:selected:$expanded",
                )
                .testTag("parity:settings-section:${section.id}")
                .semantics { selected = expanded }
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
        row.syncIndicator?.let { indicator ->
            SettingsSyncIndicatorView(indicator, row.id)
        }
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
    SettingsPageRowSurface(row = row) {
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
                                modifier = Modifier.testTag(
                                    "parity:settings-choice:${row.id}:${item.cell.id}",
                                ),
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
    row: UiSettingsPageRow,
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
        SettingsPageRowHeader(row)
        content()
    }
}

@Composable
private fun SettingsPageRowHeader(row: UiSettingsPageRow) {
    val context = LocalContext.current
    val foreground = Color(0xFF101820)
    Row(
        modifier = Modifier.fillMaxWidth(),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(ThumbSize * 0.16f),
    ) {
        Text(
            text = row.title,
            modifier = Modifier.weight(1f),
            style = MaterialTheme.typography.titleLarge.copy(
                fontSize = SettingsPageRowTitleTextSize,
                lineHeight = SettingsPageRowTitleTextSize * 1.08f,
                fontWeight = FontWeight.Black,
            ),
            color = foreground,
            maxLines = 2,
            overflow = TextOverflow.Ellipsis,
        )
        row.syncIndicator?.let { indicator ->
            SettingsSyncIndicatorView(indicator, row.id)
        }
        row.helpText?.takeIf(String::isNotBlank)?.let { helpText ->
            Box(
                modifier = Modifier
                    .size(26.dp)
                    .clip(CircleShape)
                    .border(1.dp, foreground, CircleShape)
                    .clickable(
                        interactionSource = remember { MutableInteractionSource() },
                        indication = null,
                        role = Role.Button,
                    ) { showActionToast(context, helpText, long = true) }
                    .testTag("parity:settings-help:${row.id}"),
                contentAlignment = Alignment.Center,
            ) {
                Text(
                    text = "?",
                    style = MaterialTheme.typography.labelMedium,
                    fontWeight = FontWeight.ExtraBold,
                    color = foreground,
                )
            }
        }
    }
}

@Composable
private fun SettingsSyncIndicatorView(
    indicator: UiSettingsSyncIndicator,
    testId: String,
) {
    val context = LocalContext.current
    Box(
        modifier = Modifier
            .size(26.dp)
            .clickable(
                interactionSource = remember { MutableInteractionSource() },
                indication = null,
                role = Role.Button,
            ) { showActionToast(context, indicator.helpText, long = true) }
            .testTag("parity:settings-sync:$testId"),
        contentAlignment = Alignment.Center,
    ) {
        Text(
            text = indicator.symbol,
            style = MaterialTheme.typography.labelLarge,
            fontWeight = FontWeight.ExtraBold,
            color = Color(0xFF101820),
        )
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
    SettingsPageRowSurface(row = row) {
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
                    modifier = Modifier
                        .fillMaxWidth()
                        .testTag("parity:settings-slider:${row.id}:${row.valueId}"),
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
