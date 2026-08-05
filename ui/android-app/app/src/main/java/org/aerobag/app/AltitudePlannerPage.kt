// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.zIndex
import kotlinx.coroutines.CancellationException
import org.aerobag.app.domain.AltitudeComparisonPanelUiView
import org.aerobag.app.domain.AltitudePlannerUiView
import org.aerobag.app.domain.NativeUiSession
import org.aerobag.app.domain.NavElementUiView
import org.aerobag.app.domain.UiSessionSnapshot

@Composable
internal fun AltitudePlannerPage(
    page: AppPage,
    planner: AltitudePlannerUiView,
    planVersion: Long,
    uiSession: NativeUiSession,
    navElement: NavElementUiView?,
    mostRecentChartOrPlatePage: AppPage,
    onOpenPlan: () -> Unit,
    onOpenRecentChartOrPlate: () -> Unit,
    onSelectPage: (AppPage) -> Unit,
    onApplySessionSnapshot: (UiSessionSnapshot) -> Unit,
    onSessionCommandFailure: (Throwable) -> Unit,
) {
    val context = LocalContext.current
    val uiTheme = LocalAerobagUiTheme.current
    var comparisonPanel by remember { mutableStateOf<AltitudeComparisonPanelUiView?>(null) }
    var loading by remember { mutableStateOf(false) }
    var errorMessage by remember { mutableStateOf<String?>(null) }
    var refreshRevision by remember { mutableStateOf(0) }

    LaunchedEffect(page, planVersion, planner.estimateSummary.label, refreshRevision) {
        loading = true
        errorMessage = null
        try {
            comparisonPanel = uiSession.altitudeComparisons()
        } catch (error: CancellationException) {
            throw error
        } catch (error: Throwable) {
            comparisonPanel = null
            errorMessage = error.message ?: "Altitude comparison failed"
        } finally {
            loading = false
        }
    }

    fun performAction(actionUid: String) {
        try {
            onApplySessionSnapshot(uiSession.performAltitudePlannerAction(actionUid))
            refreshRevision += 1
        } catch (error: CancellationException) {
            throw error
        } catch (error: Throwable) {
            onSessionCommandFailure(error)
        }
    }

    Box(
        modifier = Modifier
            .fillMaxSize()
            .background(uiTheme.controls.chartSurfaceBg),
    ) {
        Column(
            modifier = Modifier
                .fillMaxSize()
                .padding(
                    start = ThumbGap,
                    end = ThumbGap,
                    top = ThumbGap,
                    bottom = ThumbSize + (ThumbGap * 2f),
                ),
            verticalArrangement = Arrangement.spacedBy(ThumbSize * 0.2f),
        ) {
            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.spacedBy(ThumbGap),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Text(
                    text = planner.title.uppercase(),
                    modifier = Modifier.weight(1f),
                    color = uiTheme.controls.panelFg,
                    style = MaterialTheme.typography.headlineSmall.copy(
                        fontSize = 16.sp,
                        lineHeight = 16.sp,
                        fontWeight = FontWeight.Black,
                    ),
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
                planner.controls.forEach { control ->
                    CompactSquareButton(
                        label = control.label,
                        modifier = Modifier
                            .width(ThumbSize * 2.2f)
                            .height(ThumbSize),
                        maxLines = 2,
                        enabled = control.enabled,
                        testTag = "parity:altitude-planner-control:${control.id}",
                        onDisabledClick = control.disabledReason?.let { reason ->
                            { showDisabledActionToast(context, reason) }
                        },
                        onClick = {
                            control.actionUid?.let(::performAction)
                        },
                    )
                }
            }

            if (planner.unavailableReasons.isNotEmpty()) {
                PlannerMessagePanel(
                    messages = planner.unavailableReasons.map { it.message },
                    foreground = uiTheme.controls.panelFg,
                    background = uiTheme.controls.panelBg,
                )
            }
            errorMessage?.let { message ->
                PlannerMessagePanel(
                    messages = listOf(message),
                    foreground = uiTheme.controls.panelFg,
                    background = uiTheme.controls.panelBg,
                )
            }
            Box(
                modifier = Modifier
                    .fillMaxWidth()
                    .weight(1f),
            ) {
                comparisonPanel?.let { panel ->
                    Column(modifier = Modifier.fillMaxSize()) {
                        Row(modifier = Modifier.fillMaxWidth()) {
                            panel.columns.forEach { column ->
                                Text(
                                    text = column.label,
                                    modifier = Modifier.weight(1f),
                                    color = uiTheme.controls.panelFg,
                                    style = MaterialTheme.typography.labelMedium,
                                    fontWeight = FontWeight.Bold,
                                    textAlign = TextAlign.Center,
                                )
                            }
                        }
                        LazyColumn(
                            modifier = Modifier
                                .fillMaxWidth()
                                .weight(1f)
                                .testTag("parity:altitude-comparison-panel"),
                            verticalArrangement = Arrangement.spacedBy(ThumbGap),
                        ) {
                            items(panel.rows) { row ->
                                val rowColor = when {
                                    !row.enabled -> uiTheme.controls.buttonDisabled
                                    row.selected -> uiTheme.controls.buttonChecked
                                    else -> uiTheme.controls.buttonUnchecked
                                }
                                Surface(
                                    modifier = Modifier
                                        .fillMaxWidth()
                                        .height(ThumbSize * 0.68f)
                                        .clickable {
                                            val actionUid = row.actionUid
                                            if (row.enabled && actionUid != null) {
                                                performAction(actionUid)
                                            } else {
                                                row.disabledReason?.let {
                                                    showDisabledActionToast(context, it)
                                                }
                                            }
                                        },
                                    color = rowColor,
                                    shape = RoundedCornerShape(ThumbRadius * 0.7f),
                                    border = BorderStroke(1.dp, uiTheme.controls.panelBorder),
                                ) {
                                    Row(
                                        modifier = Modifier.fillMaxSize(),
                                        verticalAlignment = Alignment.CenterVertically,
                                    ) {
                                        row.cells.forEach { cell ->
                                            Text(
                                                text = cell.value ?: "—",
                                                modifier = Modifier.weight(1f),
                                                color = uiTheme.controls.buttonFg,
                                                style = MaterialTheme.typography.bodyMedium,
                                                fontWeight = if (row.selected) FontWeight.Bold else FontWeight.Normal,
                                                textAlign = TextAlign.Center,
                                            )
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                if (loading) {
                    Row(
                        modifier = Modifier
                            .fillMaxSize()
                            .background(uiTheme.controls.chartSurfaceBg.copy(alpha = 0.72f))
                            .testTag("parity:altitude-comparison-loading"),
                        horizontalArrangement = Arrangement.spacedBy(ThumbGap * 1.5f, Alignment.CenterHorizontally),
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        CircularProgressIndicator(
                            modifier = Modifier
                                .width(ThumbSize * 0.42f)
                                .height(ThumbSize * 0.42f),
                            color = uiTheme.controls.panelFg,
                        )
                        Text(
                            text = "Calculating…",
                            color = uiTheme.controls.panelFg,
                            style = MaterialTheme.typography.bodyMedium,
                            fontWeight = FontWeight.Bold,
                        )
                    }
                }
            }
        }

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
    }
}

@Composable
private fun PlannerMessagePanel(
    messages: List<String>,
    foreground: Color,
    background: Color,
) {
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .background(background, RoundedCornerShape(ThumbRadius))
            .border(1.dp, LocalAerobagUiTheme.current.controls.panelBorder, RoundedCornerShape(ThumbRadius))
            .padding(ThumbSize * 0.2f),
        verticalArrangement = Arrangement.spacedBy(ThumbGap),
    ) {
        messages.forEach { message ->
            Text(
                text = message,
                color = foreground,
                style = MaterialTheme.typography.bodyMedium,
            )
        }
    }
}
