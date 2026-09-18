// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.relocation.bringIntoViewRequester
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.ExperimentalComposeUiApi
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.RectangleShape
import androidx.compose.ui.graphics.lerp
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.testTagsAsResourceId
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.DialogProperties
import org.aerobag.app.domain.NotamBadgeUiView
import org.aerobag.app.domain.AirportNotamUiView
import org.aerobag.app.domain.NotamDetailUiView

@Composable
internal fun NotamBadgedControl(
    badge: NotamBadgeUiView?,
    modifier: Modifier = Modifier,
    overlayBadge: Boolean = false,
    onOpenChange: (Boolean) -> Unit = {},
    content: @Composable () -> Unit,
) {
    var open by remember(badge?.actionId) { mutableStateOf(false) }
    val currentOnOpenChange by rememberUpdatedState(onOpenChange)
    val readerOpen = open && badge != null
    var previousOpen by remember { mutableStateOf(false) }
    LaunchedEffect(readerOpen) {
        if (previousOpen != readerOpen) {
            previousOpen = readerOpen
            currentOnOpenChange(readerOpen)
        }
    }
    if (overlayBadge) {
        Box(modifier) {
            content()
            badge?.let {
                NotamBadgeButton(it, ThumbSize * 0.6f,
                    Modifier.align(Alignment.BottomEnd).padding(2.dp)) { open = true }
            }
        }
    } else {
        BoxWithConstraints(modifier, propagateMinConstraints = true) {
            val boundedWidth = constraints.hasBoundedWidth
            Row(verticalAlignment = Alignment.CenterVertically) {
                // Reserve badge space in bounded rows. Scrollable rows have
                // unbounded width: weighting there gives the control zero width.
                Box(if (boundedWidth) Modifier.weight(1f, fill = false) else Modifier) {
                    content()
                }
                badge?.let {
                    NotamBadgeButton(it, ThumbSize * 0.6f) { open = true }
                }
            }
        }
    }
    if (open && badge != null) {
        Dialog(onDismissRequest = { open = false }, properties = DialogProperties(usePlatformDefaultWidth = false)) {
            NotamModal(detail = badge.detail, modifier = Modifier.padding(ThumbGap))
        }
    }
}

@Composable
internal fun NotamBadgeButton(
    badge: NotamBadgeUiView,
    badgeSize: Dp,
    modifier: Modifier = Modifier,
    onClick: () -> Unit,
) {
    val uiTheme = LocalAerobagUiTheme.current
    Surface(
        modifier = modifier
            .size(badgeSize)
            .e2eIndexedControl(
                semanticTag = "parity:plate-notam:${badge.actionId}",
                enabled = true,
                text = badge.accessibilityLabel,
            )
            .semantics { contentDescription = badge.accessibilityLabel }
            .clickable(onClick = onClick),
        shape = RectangleShape,
        color = uiTheme.plateFolder.notamBadgeBg,
        contentColor = uiTheme.plateFolder.notamBadgeFg,
        border = BorderStroke(2.dp, uiTheme.plateFolder.notamBadgeStroke),
        shadowElevation = 2.dp,
    ) {
        Box(contentAlignment = Alignment.Center) {
            Text(
                text = "${badge.label}${badge.count}",
                style = MaterialTheme.typography.labelSmall.copy(
                    fontSize = (badgeSize.value * 0.32f).sp,
                    fontWeight = FontWeight.Black,
                    letterSpacing = 0.sp,
                ),
                maxLines = 1,
            )
        }
    }
}

@OptIn(androidx.compose.foundation.ExperimentalFoundationApi::class)
@Composable
internal fun AirportNotamSection(
    notams: List<AirportNotamUiView>,
    label: String = "NOTAM",
    trailingLabel: String = notams.size.toString(),
    emptyText: String,
) {
    val tour = LocalGuidedTour.current
    val bringIntoView = remember { androidx.compose.foundation.relocation.BringIntoViewRequester() }
    LaunchedEffect(tour?.generation) {
        if (tour?.surface == org.aerobag.app.generated.UiTourSurface.Notams) bringIntoView.bringIntoView()
    }
    val uiTheme = LocalAerobagUiTheme.current
    Column(
        modifier = Modifier
            .fillMaxWidth().guidedTourAnchor("tour:notams").bringIntoViewRequester(bringIntoView)
            .background(
                uiTheme.controls.mapSelectionDisplayBg.copy(alpha = 0.72f),
                RoundedCornerShape(ThumbRadius),
            )
            .border(
                1.dp,
                uiTheme.controls.panelBorder.copy(alpha = 0.4f),
                RoundedCornerShape(ThumbRadius),
            )
            .padding(ThumbSize * 0.13f),
        verticalArrangement = Arrangement.spacedBy(ThumbGap * 0.45f),
    ) {
        Row(
            modifier = Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.SpaceBetween,
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text(
                text = label,
                style = MaterialTheme.typography.labelSmall.copy(
                    fontWeight = FontWeight.Black,
                    letterSpacing = 0.6.sp,
                ),
                color = uiTheme.controls.panelFg,
            )
            Text(
                text = trailingLabel,
                style = MaterialTheme.typography.labelSmall.copy(fontWeight = FontWeight.Black),
                color = uiTheme.controls.panelFg,
            )
        }
        if (notams.isEmpty()) {
            Text(
                text = emptyText,
                style = MaterialTheme.typography.bodyMedium.copy(fontWeight = FontWeight.Bold),
                color = uiTheme.controls.panelFg.copy(alpha = 0.65f),
            )
        } else {
            Column(
                verticalArrangement = Arrangement.spacedBy(ThumbGap * 0.65f),
            ) {
                notams.forEach { notam ->
                    Column(
                        modifier = Modifier
                            .fillMaxWidth()
                            .background(
                                uiTheme.controls.mapSelectionDisplayBg,
                                RoundedCornerShape(ThumbRadius * 0.75f),
                            )
                            .border(
                                1.dp,
                                uiTheme.controls.panelBorder.copy(alpha = 0.5f),
                                RoundedCornerShape(ThumbRadius * 0.75f),
                            )
                            .padding(ThumbSize * 0.11f),
                        verticalArrangement = Arrangement.spacedBy(ThumbGap * 0.35f),
                    ) {
                        Text(
                            text = notam.label,
                            style = MaterialTheme.typography.labelSmall.copy(
                                fontWeight = FontWeight.Black,
                                letterSpacing = 0.6.sp,
                            ),
                            color = uiTheme.controls.panelFg,
                        )
                        Text(
                            text = notam.text,
                            style = MaterialTheme.typography.bodyMedium.copy(
                                fontSize = 15.sp,
                                lineHeight = 19.sp,
                                fontWeight = FontWeight.Bold,
                                fontFamily = FontFamily.Monospace,
                            ),
                            color = uiTheme.controls.panelFg,
                        )
                    }
                }
            }
        }
    }
}

@Composable
@OptIn(ExperimentalComposeUiApi::class)
internal fun NotamModal(
    detail: NotamDetailUiView,
    modifier: Modifier = Modifier,
) {
    val uiTheme = LocalAerobagUiTheme.current
    Surface(
        modifier = modifier
            .testTag("parity:procedure-notam-modal")
            .semantics { testTagsAsResourceId = true }
            .widthIn(max = ThumbSize * 10.5f)
            .heightIn(max = ThumbSize * 11.5f),
        shape = RoundedCornerShape(ThumbRadius + 4.dp),
        color = uiTheme.controls.panelBg.copy(alpha = 0.98f),
        contentColor = uiTheme.controls.panelFg,
        shadowElevation = 8.dp,
        border = BorderStroke(1.dp, uiTheme.controls.panelBorder.copy(alpha = 0.85f)),
    ) {
        Column(
            modifier = Modifier
                .verticalScroll(rememberScrollState())
                .padding(ThumbSize * 0.18f),
            verticalArrangement = Arrangement.spacedBy(ThumbGap * 0.85f),
        ) {
            Text(
                text = detail.title.uppercase(),
                style = MaterialTheme.typography.labelMedium.copy(
                    fontWeight = FontWeight.Black,
                    letterSpacing = 0.4.sp,
                ),
                color = uiTheme.controls.panelFg,
            )
            Text(
                text = detail.advisoryText,
                modifier = Modifier
                    .fillMaxWidth()
                    .background(uiTheme.controls.dataStatusWarningBg, RoundedCornerShape(ThumbRadius))
                    .border(1.dp, uiTheme.controls.dataStatusWarningStroke, RoundedCornerShape(ThumbRadius))
                    .padding(ThumbSize * 0.13f),
                style = MaterialTheme.typography.bodyMedium.copy(
                    fontWeight = FontWeight.Bold,
                    lineHeight = 18.sp,
                ),
                color = lerp(Color.Black, uiTheme.controls.dataStatusWarningStroke, 0.3f),
            )
            AirportNotamSection(
                notams = detail.notams,
                emptyText = detail.emptyText,
            )
        }
    }
}
