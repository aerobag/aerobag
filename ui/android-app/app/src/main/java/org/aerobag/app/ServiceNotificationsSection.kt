// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app

import android.net.Uri
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.platform.LocalUriHandler
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.selected
import androidx.compose.ui.unit.dp
import org.aerobag.app.domain.ControlsTheme
import org.aerobag.app.generated.UiServiceNoticeAction
import org.aerobag.app.generated.UiServiceNoticeTone
import org.aerobag.app.generated.UiServiceNotificationsState

internal data class ServiceNoticeStyle(
    val background: Color,
    val foreground: Color,
    val highlight: Color?,
)

internal fun serviceNoticeStyle(tone: UiServiceNoticeTone, theme: ControlsTheme): ServiceNoticeStyle =
    when (tone) {
        UiServiceNoticeTone.Info -> ServiceNoticeStyle(
            theme.dataStatusInfoBg, theme.panelFg, theme.dataStatusInfoStroke,
        )
        UiServiceNoticeTone.Caution -> ServiceNoticeStyle(
            theme.dataStatusCautionBg, theme.panelFg, theme.dataStatusCautionStroke,
        )
        UiServiceNoticeTone.Warning -> ServiceNoticeStyle(
            theme.dataStatusWarningBg, theme.panelFg, theme.dataStatusWarningStroke,
        )
        UiServiceNoticeTone.History -> ServiceNoticeStyle(theme.dataStatusQuietBg, theme.panelFg, null)
    }

@Composable
@OptIn(ExperimentalLayoutApi::class)
internal fun ServiceNotificationsSection(
    state: UiServiceNotificationsState,
    onAction: (String) -> Unit,
) {
    val theme = LocalAerobagUiTheme.current.controls
    val uriHandler = LocalUriHandler.current
    Column(
        modifier = Modifier.fillMaxWidth().e2eIndexedElement(
            semanticTag = "parity:service:section",
            state = "expanded:${state.expanded}",
        ),
        verticalArrangement = Arrangement.spacedBy(ThumbGap),
    ) {
        Row(
            modifier = Modifier.fillMaxWidth().heightIn(min = ThumbSize)
                .background(theme.controlGroupBg)
                .e2eIndexedControl(
                    semanticTag = "parity:service:toggle",
                    enabled = true,
                    selected = state.expanded,
                    text = "${state.title} ${state.summary}",
                    state = "expanded:${state.expanded}",
                )
                .clickable(role = Role.Button, onClickLabel = state.toggleAction.label) {
                    onAction(state.toggleAction.actionId)
                }
                .semantics {
                    contentDescription = state.toggleAction.label
                    selected = state.expanded
                }
                .padding(ThumbGap),
            horizontalArrangement = Arrangement.spacedBy(ThumbGap),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Canvas(Modifier.size(ThumbSize * 0.25f)) {
                val unit = size.width / 12f
                val triangle = Path().apply {
                    if (state.expanded) {
                        moveTo(unit, 3f * unit)
                        lineTo(11f * unit, 3f * unit)
                        lineTo(6f * unit, 9f * unit)
                    } else {
                        moveTo(3f * unit, unit)
                        lineTo(9f * unit, 6f * unit)
                        lineTo(3f * unit, 11f * unit)
                    }
                    close()
                }
                drawPath(triangle, theme.panelFg)
            }
            FlowRow(
                modifier = Modifier.weight(1f),
                horizontalArrangement = Arrangement.SpaceBetween,
                verticalArrangement = Arrangement.spacedBy(ThumbGap * 0.5f),
            ) {
                Text(state.title, style = MaterialTheme.typography.titleMedium, color = theme.panelFg)
                Text(state.summary, style = MaterialTheme.typography.bodySmall, color = theme.panelMuted)
            }
        }
        if (state.expanded) {
            state.markAllRead?.let {
                NoticeActionButton(
                    it, "parity:service:mark-all-read", onAction,
                    modifier = Modifier.align(Alignment.End).width(ThumbSize * 3.5f),
                )
            }
            state.items.forEach { notice ->
                val style = serviceNoticeStyle(notice.tone, theme)
                Column(
                    modifier = Modifier.fillMaxWidth().background(style.background)
                        .then(style.highlight?.let { Modifier.border(1.dp, it) } ?: Modifier)
                        .padding(ThumbGap),
                    verticalArrangement = Arrangement.spacedBy(ThumbGap),
                ) {
                    NoticeActionButton(
                        notice.openAction, "parity:service:notice:${notice.id}", onAction, style,
                    )
                    Text(notice.stateLabel, color = style.highlight ?: style.foreground)
                    Text(notice.timing, color = style.foreground)
                    if (notice.expanded) {
                        Text(
                            notice.body,
                            modifier = Modifier.e2eIndexedElement(
                                semanticTag = "parity:service:body:${notice.id}",
                                state = "enabled:true:text:${Uri.encode(notice.body)}",
                            ),
                            style = MaterialTheme.typography.bodyLarge,
                            color = style.foreground,
                        )
                        notice.link?.let { link ->
                            CompactSquareButton(
                                label = link.label,
                                modifier = Modifier.fillMaxWidth().height(ThumbSize),
                                wide = true, maxLines = 2,
                                testTag = "parity:service-link:${notice.id}",
                                backgroundColor = style.background,
                                foregroundColor = style.foreground,
                                onClick = { uriHandler.openUri(link.url) },
                            )
                        }
                    }
                }
            }
            state.sourceStatus.forEach { Text(it, color = theme.panelMuted) }
        }
    }
}

@Composable
private fun NoticeActionButton(
    action: UiServiceNoticeAction,
    testTag: String,
    onAction: (String) -> Unit,
    style: ServiceNoticeStyle? = null,
    modifier: Modifier = Modifier.fillMaxWidth(),
) {
    CompactSquareButton(
        label = action.label,
        modifier = modifier.height(ThumbSize),
        maxLines = 2, wide = true,
        testTag = testTag,
        backgroundColor = style?.background,
        foregroundColor = style?.foreground,
        onClick = { onAction(action.actionId) },
    )
}
