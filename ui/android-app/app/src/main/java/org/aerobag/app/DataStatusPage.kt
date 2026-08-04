// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app

import android.content.Context
import android.content.SharedPreferences
import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.layout.wrapContentSize
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.grid.GridCells
import androidx.compose.foundation.lazy.grid.LazyVerticalGrid
import androidx.compose.foundation.lazy.grid.items as lazyGridItems
import androidx.compose.foundation.lazy.items as lazyColumnItems
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableLongStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.lerp
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalUriHandler
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextDecoration
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.window.Popup
import androidx.compose.ui.window.PopupProperties
import androidx.compose.ui.zIndex
import kotlinx.coroutines.delay
import org.aerobag.app.domain.NavElementUiView
import org.aerobag.app.domain.UiDataStatusPageFact
import org.aerobag.app.domain.UiDataStatusPageRow
import org.aerobag.app.domain.UiDataStatusPageState
import org.aerobag.app.domain.UiDataStatusPageTimeDisplay
import org.aerobag.app.domain.UiDataStatusState
import org.aerobag.app.domain.UiStatusAction
import org.aerobag.app.domain.UiStatusActionStyle
import org.aerobag.app.domain.UiStatusSeverity
import org.aerobag.app.domain.NativeBindings
import kotlin.math.roundToInt

@Composable
internal fun DataStatusBadge(
    dataStatusState: UiDataStatusState,
    modifier: Modifier = Modifier,
    open: Boolean,
    onToggle: () -> Unit,
    onAction: (String) -> Unit = {},
) {
    val hasStatus = dataStatusState.boxes.isNotEmpty()
    if (!hasStatus) return

    val uiTheme = LocalAerobagUiTheme.current
    val density = LocalDensity.current
    val launcherSize = ThumbSize * 0.5f
    val panelWidth = ThumbSize * 4.25f
    val popupOffset = with(density) {
        IntOffset(
            x = (launcherSize - panelWidth).toPx().roundToInt(),
            y = (launcherSize + ThumbGap).toPx().roundToInt(),
        )
    }
    Box(
        modifier = modifier
            .size(launcherSize)
            .wrapContentSize(unbounded = true, align = Alignment.TopEnd),
    ) {
        DataStatusBadgeFace(
            count = dataStatusState.launcherCount,
            severity = dataStatusState.launcherSeverity,
            open = open,
            badgeSize = launcherSize,
            modifier = Modifier
                .clickable(
                    indication = null,
                    interactionSource = remember { MutableInteractionSource() },
                    onClick = onToggle,
                ),
        )
        if (open) {
            Popup(
                offset = popupOffset,
                onDismissRequest = onToggle,
                properties = PopupProperties(focusable = true),
            ) {
                Surface(
                    modifier = Modifier
                        .width(panelWidth)
                        .heightIn(max = ThumbSize * 7.2f),
                    shape = RoundedCornerShape(ThumbRadius),
                    color = uiTheme.controls.panelBg.copy(alpha = 0.96f),
                    contentColor = uiTheme.controls.panelFg,
                    shadowElevation = 8.dp,
                    border = BorderStroke(1.dp, uiTheme.controls.panelBorder.copy(alpha = 0.85f)),
                ) {
                    LazyColumn(
                        modifier = Modifier.padding(ThumbSize * 0.14f),
                        verticalArrangement = Arrangement.spacedBy(ThumbSize * 0.12f),
                    ) {
                        lazyColumnItems(dataStatusState.boxes) { box ->
                            DataStatusBoxRow(
                                label = box.label,
                                value = box.value ?: "\u2014",
                                detail = box.detail,
                                severity = box.severity,
                                hushed = box.hushed,
                                actions = box.actions,
                                onAction = onAction,
                            )
                        }
                    }
                }
            }
        }
    }
}

@Composable
internal fun DataStatusBadgeFace(
    count: String?,
    severity: UiStatusSeverity,
    open: Boolean,
    badgeSize: androidx.compose.ui.unit.Dp,
    modifier: Modifier = Modifier,
) {
    val uiTheme = LocalAerobagUiTheme.current
    val hasCount = count != null
    val background = when {
        !hasCount -> uiTheme.controls.dataStatusQuietBg
        severity == UiStatusSeverity.Warning ->
            lerp(uiTheme.controls.dataStatusWarningBg, Color(0xFFD55B18), 0.22f)
        else -> uiTheme.controls.dataStatusWarningBg
    }
    val stroke = if (hasCount) {
        uiTheme.controls.dataStatusWarningStroke
    } else {
        uiTheme.controls.dataStatusQuietStroke
    }
    val resolvedBackground = if (open) lerp(background, Color.White, 0.18f) else background
    Box(
        modifier = modifier
            .size(badgeSize)
            .clip(RoundedCornerShape(ThumbRadius * 0.78f))
            .background(resolvedBackground)
            .border(1.dp, stroke, RoundedCornerShape(ThumbRadius * 0.78f)),
        contentAlignment = Alignment.Center,
    ) {
        Canvas(modifier = Modifier.size(badgeSize * 0.828f)) {
            val symbol = Path().apply {
                moveTo(size.width * 0.50f, size.height * 0.08f)
                lineTo(size.width * 0.92f, size.height * 0.84f)
                lineTo(size.width * 0.08f, size.height * 0.84f)
                close()
            }
            drawPath(symbol, stroke)
            drawLine(
                color = Color(0xFF111111),
                start = Offset(this.size.width * 0.50f, this.size.height * 0.32f),
                end = Offset(this.size.width * 0.50f, this.size.height * 0.58f),
                strokeWidth = this.size.width * 0.10f,
                cap = StrokeCap.Round,
            )
            drawCircle(
                color = Color(0xFF111111),
                radius = this.size.width * 0.045f,
                center = Offset(this.size.width * 0.50f, this.size.height * 0.71f),
            )
        }
        count?.let {
            Text(
                text = it,
                modifier = Modifier
                    .align(Alignment.BottomEnd)
                    .padding(end = badgeSize * 0.11f, bottom = badgeSize * 0.07f),
                style = MaterialTheme.typography.labelSmall.copy(
                    fontSize = (badgeSize.value / 3f).sp,
                    fontWeight = FontWeight.Black,
                    letterSpacing = 0.sp,
                ),
                color = Color(0xFF111111),
                maxLines = 1,
            )
        }
    }
}

@Composable
private fun DataStatusBoxRow(
    label: String,
    value: String,
    detail: String,
    severity: UiStatusSeverity,
    hushed: Boolean,
    actions: List<UiStatusAction>,
    onAction: (String) -> Unit,
) {
    val uiTheme = LocalAerobagUiTheme.current
    val accentColor = statusSeverityColor(severity)
    val background = Color.White.copy(alpha = if (hushed) 0.48f else 0.78f)
    val strokeWidth = if (severity == UiStatusSeverity.Caution || severity == UiStatusSeverity.Warning) 2.dp else 1.dp
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .alpha(if (hushed) 0.58f else 1f)
            .clip(RoundedCornerShape(ThumbRadius * 0.72f))
            .background(background)
            .border(strokeWidth, accentColor.copy(alpha = if (hushed) 0.42f else 0.74f), RoundedCornerShape(ThumbRadius * 0.72f))
            .padding(ThumbSize * 0.12f),
        verticalArrangement = Arrangement.spacedBy(ThumbSize * 0.06f),
    ) {
        Row(
            horizontalArrangement = Arrangement.spacedBy(ThumbSize * 0.12f),
            verticalAlignment = Alignment.CenterVertically,
            modifier = Modifier.fillMaxWidth(),
        ) {
            Text(
                text = label,
                modifier = Modifier.weight(1f),
                style = MaterialTheme.typography.labelSmall.copy(
                    fontSize = 11.sp,
                    fontWeight = FontWeight.Black,
                    letterSpacing = 0.6.sp,
                ),
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
                color = uiTheme.controls.panelFg,
            )
            Text(
                text = value,
                style = MaterialTheme.typography.labelMedium.copy(
                    fontSize = 15.sp,
                    fontWeight = FontWeight.Black,
                    letterSpacing = 0.4.sp,
                ),
                maxLines = 1,
                color = uiTheme.controls.panelFg,
            )
        }
        Text(
            text = detail,
            style = MaterialTheme.typography.bodySmall.copy(
                fontSize = 10.sp,
                lineHeight = 12.sp,
                fontWeight = FontWeight.SemiBold,
            ),
            maxLines = 3,
            overflow = TextOverflow.Ellipsis,
            color = uiTheme.controls.panelFg.copy(alpha = if (hushed) 0.68f else 0.9f),
        )
        if (actions.isNotEmpty()) {
            Row(
                horizontalArrangement = Arrangement.spacedBy(ThumbSize * 0.08f),
                modifier = Modifier.fillMaxWidth(),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Box(modifier = Modifier.weight(1f))
                actions.forEach { action ->
                    val actionBg = when (action.style) {
                        UiStatusActionStyle.Hush -> uiTheme.controls.panelFg.copy(alpha = 0.88f)
                        UiStatusActionStyle.Normal -> uiTheme.controls.buttonUnchecked
                    }
                    Surface(
                        modifier = Modifier
                            .widthIn(min = ThumbSize * 0.9f)
                            .height(ThumbSize * 0.42f)
                            .alpha(if (action.enabled) 1f else 0.45f)
                            .then(
                                if (action.enabled) {
                                    Modifier.clickable(
                                        indication = null,
                                        interactionSource = remember { MutableInteractionSource() },
                                    ) { onAction(action.id) }
                                } else {
                                    Modifier
                                },
                            ),
                        shape = RoundedCornerShape(ThumbRadius * 0.45f),
                        color = actionBg,
                        contentColor = Color.White,
                        border = BorderStroke(1.dp, uiTheme.controls.panelFg.copy(alpha = 0.16f)),
                    ) {
                        Box(
                            modifier = Modifier.padding(horizontal = ThumbSize * 0.14f),
                            contentAlignment = Alignment.Center,
                        ) {
                            Text(
                                text = action.label.uppercase(),
                                style = MaterialTheme.typography.labelSmall.copy(
                                    fontSize = 9.sp,
                                    fontWeight = FontWeight.ExtraBold,
                                    letterSpacing = 0.5.sp,
                                ),
                                maxLines = 1,
                                overflow = TextOverflow.Ellipsis,
                            )
                        }
                    }
                }
            }
        }
    }
}

private fun statusSeverityColor(severity: UiStatusSeverity): Color = when (severity) {
    UiStatusSeverity.Ok -> Color(0xFF7ED6A7)
    UiStatusSeverity.Info -> Color(0xFF8FB7FF)
    UiStatusSeverity.Caution -> Color(0xFFFFD35A)
    UiStatusSeverity.Warning -> Color(0xFFFF8B5A)
    UiStatusSeverity.Unavailable -> Color(0xFFB7BDC7)
}

private val DataStatusPageTitleTextSize = 15.sp
private val DataStatusPageSummaryTextSize = 10.sp
private val DataStatusPageRowHeaderTextSize = 13.sp
private val DataStatusPageDetailTextSize = 10.sp
private val DataStatusPageFactTextSize = 9.sp

@Composable
internal fun DataStatusPage(
    page: AppPage,
    state: UiDataStatusPageState,
    dataSourcesRow: UiDataStatusPageRow,
    navElement: NavElementUiView?,
    mostRecentChartOrPlatePage: AppPage,
    onOpenPlan: () -> Unit,
    onOpenRecentChartOrPlate: () -> Unit,
    onSelectPage: (AppPage) -> Unit,
) {
    val uiTheme = LocalAerobagUiTheme.current
    var nowMs by remember { mutableLongStateOf(System.currentTimeMillis()) }
    LaunchedEffect(page) {
        while (true) {
            nowMs = System.currentTimeMillis()
            delay(10_000)
        }
    }
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
                .padding(ThumbSize * 0.28f),
            verticalArrangement = Arrangement.spacedBy(ThumbSize * 0.2f),
        ) {
            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.spacedBy(ThumbSize * 0.25f),
                verticalAlignment = Alignment.Bottom,
            ) {
                Text(
                    text = state.title.uppercase(),
                    modifier = Modifier.weight(1f),
                    style = MaterialTheme.typography.headlineSmall.copy(
                        fontSize = DataStatusPageTitleTextSize,
                        lineHeight = DataStatusPageTitleTextSize,
                        fontWeight = FontWeight.Black,
                    ),
                    color = uiTheme.controls.buttonFg,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
                Text(
                    text = state.summary.uppercase(),
                    style = MaterialTheme.typography.titleMedium.copy(
                        fontSize = DataStatusPageSummaryTextSize,
                        lineHeight = DataStatusPageSummaryTextSize * 1.08f,
                        fontWeight = FontWeight.Black,
                    ),
                    color = uiTheme.controls.buttonFg.copy(alpha = 0.76f),
                    textAlign = TextAlign.End,
                    maxLines = 2,
                    overflow = TextOverflow.Ellipsis,
                )
            }
            LazyVerticalGrid(
                columns = GridCells.Adaptive(ThumbSize * 7f),
                modifier = Modifier.fillMaxSize(),
                horizontalArrangement = Arrangement.spacedBy(ThumbSize * 0.22f),
                verticalArrangement = Arrangement.spacedBy(ThumbSize * 0.22f),
            ) {
                lazyGridItems(listOf(dataSourcesRow) + state.rows, key = { it.id }) { row ->
                    DataStatusPageRowCard(row = row, nowMs = nowMs)
                }
            }
        }
    }
}

@Composable
private fun DataStatusPageRowCard(
    row: UiDataStatusPageRow,
    nowMs: Long,
) {
    val accentColor = statusSeverityColor(row.severity)
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(ThumbRadius))
            .background(Color.White.copy(alpha = 0.90f))
            .border(
                width = if (row.severity == UiStatusSeverity.Ok || row.severity == UiStatusSeverity.Info) 2.dp else 3.dp,
                color = accentColor.copy(alpha = 0.84f),
                shape = RoundedCornerShape(ThumbRadius),
            )
            .padding(ThumbSize * 0.24f),
        verticalArrangement = Arrangement.spacedBy(ThumbSize * 0.13f),
    ) {
        Row(
            modifier = Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.spacedBy(ThumbSize * 0.2f),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text(
                text = row.label.uppercase(),
                modifier = Modifier.weight(1f),
                style = MaterialTheme.typography.titleLarge.copy(
                    fontSize = DataStatusPageRowHeaderTextSize,
                    lineHeight = DataStatusPageRowHeaderTextSize,
                    fontWeight = FontWeight.Black,
                ),
                color = Color(0xFF101820),
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
            Text(
                text = row.value.uppercase(),
                style = MaterialTheme.typography.titleLarge.copy(
                    fontSize = DataStatusPageRowHeaderTextSize,
                    lineHeight = DataStatusPageRowHeaderTextSize,
                    fontWeight = FontWeight.Black,
                ),
                color = Color(0xFF101820),
                textAlign = TextAlign.End,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
        }
        Text(
            text = row.detail,
            style = MaterialTheme.typography.bodyLarge.copy(
                fontSize = DataStatusPageDetailTextSize,
                lineHeight = DataStatusPageDetailTextSize * 1.25f,
                fontWeight = FontWeight.Bold,
            ),
            color = Color(0xFF101820),
            maxLines = 3,
            overflow = TextOverflow.Ellipsis,
        )
        if (row.facts.isNotEmpty()) {
            Column(verticalArrangement = Arrangement.spacedBy(ThumbSize * 0.1f)) {
                row.facts.chunked(2).forEach { factRow ->
                    Row(
                        modifier = Modifier.fillMaxWidth(),
                        horizontalArrangement = Arrangement.spacedBy(ThumbSize * 0.18f),
                    ) {
                        factRow.forEach { fact ->
                            DataStatusFactView(
                                fact = fact,
                                nowMs = nowMs,
                                modifier = Modifier.weight(1f),
                            )
                        }
                        if (factRow.size == 1) {
                            Box(modifier = Modifier.weight(1f))
                        }
                    }
                }
            }
        }
    }
}

internal fun dataSourcesStatusRow(
    context: Context,
    prefs: SharedPreferences,
): UiDataStatusPageRow {
    val appContext = context.applicationContext
    val configuredCycleDataBaseUrl = readPackageSourceBaseUrl(appContext, prefs)
    val cycleDataBaseUrl = runCatching {
        resolvePublicationRootUrl(configuredCycleDataBaseUrl)
    }.getOrElse {
        configuredCycleDataBaseUrl.trim().trimEnd('/')
    }
    val liveFeedsRootUrl = runCatching {
        configuredLiveFeedSourceRootUrl(
            appContext,
            prefs,
            loadAndroidDevServerBaseUrl(appContext),
        )
    }.getOrElse {
        cycleDataBaseUrl.trimEnd('/').removeSuffix("/$PublicationPackageRootPath")
    }
    val liveFeedsStatusUrl = runCatching {
        NativeBindings.liveFeedStatusUrl(liveFeedsRootUrl)
    }.getOrElse {
        liveFeedsRootUrl
    }
    return UiDataStatusPageRow(
        id = "data_sources",
        label = "Data Sources",
        value = "Config",
        severity = UiStatusSeverity.Info,
        detail = "Base URLs used for remote aviation data.",
        facts = listOf(
            UiDataStatusPageFact(
                label = "Cycle Data",
                value = cycleDataBaseUrl,
                linkUrl = cycleDataBaseUrl,
                timeUtc = null,
                timeDisplay = null,
            ),
            UiDataStatusPageFact(
                label = "Live Feeds",
                value = liveFeedsStatusUrl,
                linkUrl = liveFeedsStatusUrl,
                timeUtc = null,
                timeDisplay = null,
            ),
        ),
    )
}

@Composable
private fun DataStatusFactView(
    fact: UiDataStatusPageFact,
    nowMs: Long,
    modifier: Modifier = Modifier,
) {
    val uriHandler = LocalUriHandler.current
    val textColor = Color(0xFF101820)
    val value = dataStatusFactDisplayValue(fact, nowMs)
    Column(
        modifier = modifier
            .fillMaxWidth()
            .then(
                if (fact.linkUrl.isNullOrBlank()) {
                    Modifier
                } else {
                    Modifier.clickable { uriHandler.openUri(fact.linkUrl) }
                },
            ),
        verticalArrangement = Arrangement.spacedBy(1.dp),
    ) {
        Text(
            text = fact.label.uppercase(),
            style = MaterialTheme.typography.bodyLarge.copy(
                fontSize = DataStatusPageFactTextSize,
                lineHeight = DataStatusPageFactTextSize,
                fontWeight = FontWeight.Black,
            ),
            color = textColor.copy(alpha = 0.64f),
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
        )
        Text(
            text = value,
            style = MaterialTheme.typography.bodyLarge.copy(
                fontSize = DataStatusPageFactTextSize,
                lineHeight = DataStatusPageFactTextSize * 1.18f,
                fontWeight = FontWeight.ExtraBold,
            ),
            color = textColor,
            maxLines = 2,
            overflow = TextOverflow.Ellipsis,
            textDecoration = if (fact.linkUrl.isNullOrBlank()) TextDecoration.None else TextDecoration.Underline,
        )
    }
}

private fun dataStatusFactDisplayValue(fact: UiDataStatusPageFact, nowMs: Long): String {
    val timeUtc = fact.timeUtc ?: return fact.value
    val display = fact.timeDisplay ?: return fact.value
    val instantMs = runCatching { java.time.Instant.parse(timeUtc).toEpochMilli() }.getOrNull()
        ?: return fact.value
    val suffix = dataStatusRelativeTimeSuffix(instantMs, nowMs, display)
    return if (suffix.isBlank()) fact.value else "${fact.value}\n($suffix)"
}

private fun dataStatusRelativeTimeSuffix(
    instantMs: Long,
    nowMs: Long,
    display: UiDataStatusPageTimeDisplay,
): String {
    val deltaMs = instantMs - nowMs
    val magnitude = formatDataStatusDuration(kotlin.math.abs(deltaMs))
    return when (display) {
        UiDataStatusPageTimeDisplay.Old -> "$magnitude old"
        UiDataStatusPageTimeDisplay.Until -> if (deltaMs >= 0) "in $magnitude" else "$magnitude ago"
        UiDataStatusPageTimeDisplay.Ago -> if (deltaMs >= 0) "in $magnitude" else "$magnitude ago"
    }
}

private fun formatDataStatusDuration(durationMs: Long): String {
    val minutes = durationMs / 60_000L
    if (minutes < 60L) return "${minutes}m"
    val hours = minutes / 60L
    if (hours < 48L) return "${hours}h"
    val days = hours / 24L
    if (days < 60L) return "${days}d"
    val months = days / 30L
    if (months < 24L) return "${months}mo"
    val years = days / 365L
    return "${years}y"
}
