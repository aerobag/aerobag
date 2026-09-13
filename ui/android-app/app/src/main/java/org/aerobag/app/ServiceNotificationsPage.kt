// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalUriHandler
import androidx.compose.ui.unit.dp
import androidx.compose.ui.zIndex
import org.aerobag.app.domain.NavElementUiView
import org.aerobag.app.generated.UiServiceNoticeAction
import org.aerobag.app.generated.UiServiceNotificationsState
import org.aerobag.app.generated.UiStatusSeverity

@Composable
internal fun ServiceNotificationsPage(
    state: UiServiceNotificationsState,
    navElement: NavElementUiView?,
    mostRecentChartOrPlatePage: AppPage,
    onOpenPlan: () -> Unit,
    onOpenRecentChartOrPlate: () -> Unit,
    onSelectPage: (AppPage) -> Unit,
    onAction: (String) -> Unit,
) {
    val theme = LocalAerobagUiTheme.current.controls
    val uriHandler = LocalUriHandler.current
    Box(Modifier.fillMaxSize().e2ePageRoot("parity:page:service_notifications").background(theme.chartSurfaceBg)) {
        LazyColumn(
            modifier = Modifier.fillMaxSize().padding(
                start = ThumbGap, end = ThumbGap, top = ThumbGap,
                bottom = ThumbSize + ThumbGap * 2f,
            ),
            verticalArrangement = Arrangement.spacedBy(ThumbGap),
        ) {
            item {
                Text(state.title, style = MaterialTheme.typography.headlineSmall, color = theme.panelFg)
                Text(state.summary, color = theme.panelFg)
                state.markAllRead?.let { NoticeActionButton(it, onAction) }
            }
            items(state.items, key = { it.id }) { notice ->
                val accent = when (notice.severity) {
                    UiStatusSeverity.Ok -> theme.dataStatusOkStroke
                    UiStatusSeverity.Info -> theme.dataStatusInfoStroke
                    UiStatusSeverity.Caution -> theme.dataStatusCautionStroke
                    UiStatusSeverity.Warning -> theme.dataStatusWarningStroke
                    UiStatusSeverity.Unavailable -> theme.dataStatusUnavailableStroke
                }
                Column(
                    Modifier.fillMaxWidth().background(theme.panelBg).border(1.dp, accent).padding(ThumbGap),
                    verticalArrangement = Arrangement.spacedBy(ThumbGap),
                ) {
                    NoticeActionButton(notice.openAction, onAction)
                    Text(notice.stateLabel, color = accent)
                    Text(notice.timing, color = theme.panelFg)
                    if (notice.expanded) {
                        Text(notice.body, style = MaterialTheme.typography.bodyLarge, color = theme.panelFg)
                        notice.link?.let { link ->
                            CompactSquareButton(
                                label = link.label,
                                modifier = Modifier.fillMaxWidth().height(ThumbSize),
                                wide = true, maxLines = 2,
                                testTag = "parity:service-link:${notice.id}",
                                onClick = { uriHandler.openUri(link.url) },
                            )
                        }
                    }
                }
            }
            items(state.sourceStatus) { Text(it, color = theme.panelMuted) }
        }
        PrimaryNavigationDock(
            currentPage = AppPage.ServiceNotifications,
            navElement = navElement,
            chartPlateTargetPage = mostRecentChartOrPlatePage,
            onHomeClick = { onSelectPage(AppPage.Home) },
            onOpenPlan = onOpenPlan,
            onSelectPage = onSelectPage,
            onOpenChartOrPlate = onOpenRecentChartOrPlate,
            modifier = Modifier.align(Alignment.BottomCenter).padding(bottom = ThumbGap).zIndex(OverlayPlaneControls),
        )
    }
}

@Composable
private fun NoticeActionButton(action: UiServiceNoticeAction, onAction: (String) -> Unit) {
    CompactSquareButton(
        label = action.label,
        modifier = Modifier.fillMaxWidth().height(ThumbSize),
        maxLines = 2, wide = true,
        testTag = "parity:service-action:${action.actionId}",
        onClick = { onAction(action.actionId) },
    )
}
