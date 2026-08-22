// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
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
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalFocusManager
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.zIndex
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext
import org.aerobag.app.domain.AltitudeComparisonPanelUiView
import org.aerobag.app.domain.AltitudePlannerDepartureEditorUiView
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
    val focusManager = LocalFocusManager.current
    val uiTheme = LocalAerobagUiTheme.current
    val plannerScope = rememberCoroutineScope()
    val plannerWorkMutex = remember(uiSession) { Mutex() }
    var comparisonPanel by remember { mutableStateOf<AltitudeComparisonPanelUiView?>(null) }
    var loading by remember { mutableStateOf(false) }
    var errorMessage by remember { mutableStateOf<String?>(null) }
    var userActionsInFlight by remember { mutableIntStateOf(0) }
    var comparisonRefreshRevision by remember { mutableIntStateOf(0) }
    var pendingUserRefreshRevision by remember { mutableStateOf<Int?>(null) }
    var departureTimeInput by remember { mutableStateOf(planner.departure.timeValue) }
    var departureWhenInput by remember { mutableStateOf(planner.departure.whenValue) }
    var openControlId by remember { mutableStateOf<String?>(null) }
    var departureTimeFocused by remember { mutableStateOf(false) }
    var departureWhenFocused by remember { mutableStateOf(false) }

    LaunchedEffect(planner.departure.timeValue, planner.departure.whenValue) {
        if (!departureTimeFocused) departureTimeInput = planner.departure.timeValue
        if (!departureWhenFocused) departureWhenInput = planner.departure.whenValue
    }

    val plannerProjectionKey = planVersion to planner
    LaunchedEffect(page, plannerProjectionKey, comparisonRefreshRevision) {
        if (userActionsInFlight > 0 && pendingUserRefreshRevision == null) {
            return@LaunchedEffect
        }
        val requestRefreshRevision = comparisonRefreshRevision
        loading = true
        errorMessage = null
        try {
            comparisonPanel = withContext(Dispatchers.IO) {
                plannerWorkMutex.withLock {
                    uiSession.altitudeComparisons()
                }
            }
        } catch (error: CancellationException) {
            throw error
        } catch (error: Throwable) {
            comparisonPanel = null
            errorMessage = error.message ?: "Altitude comparison failed"
        } finally {
            loading = false
            if (pendingUserRefreshRevision?.let { it <= requestRefreshRevision } == true) {
                pendingUserRefreshRevision = null
            }
        }
    }

    fun performPlannerMutation(
        operation: () -> UiSessionSnapshot,
        onFailure: (Throwable) -> Unit = onSessionCommandFailure,
    ) {
        if (userActionsInFlight > 0) return
        userActionsInFlight += 1
        errorMessage = null
        plannerScope.launch {
            try {
                val snapshot = withContext(Dispatchers.IO) {
                    plannerWorkMutex.withLock {
                        operation()
                    }
                }
                onApplySessionSnapshot(snapshot)
                comparisonRefreshRevision += 1
                pendingUserRefreshRevision = comparisonRefreshRevision
            } catch (error: CancellationException) {
                throw error
            } catch (error: Throwable) {
                pendingUserRefreshRevision = null
                onFailure(error)
            } finally {
                userActionsInFlight -= 1
            }
        }
    }

    fun performAction(actionUid: String) {
        openControlId = null
        performPlannerMutation(
            operation = { uiSession.performAltitudePlannerAction(actionUid) },
        )
    }

    fun setDepartureInput(field: String, input: String) {
        performPlannerMutation(
            operation = { uiSession.setAltitudePlannerDepartureInput(field, input) },
            onFailure = { error ->
                departureTimeInput = planner.departure.timeValue
                departureWhenInput = planner.departure.whenValue
                errorMessage = error.message ?: "Invalid departure time"
            },
        )
    }

    fun toggleDepartureTimeBasis() {
        performPlannerMutation(
            operation = {
                uiSession.performTimeDisplayAction(planner.departure.timeDisplayActionId)
            },
        )
    }

    val userActionLoading = userActionsInFlight > 0 || pendingUserRefreshRevision != null

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
            Column(
                modifier = Modifier.fillMaxWidth(),
                verticalArrangement = Arrangement.spacedBy(ThumbGap),
            ) {
                Text(
                    text = planner.title.uppercase(),
                    color = uiTheme.controls.panelFg,
                    style = MaterialTheme.typography.headlineSmall.copy(
                        fontSize = 16.sp,
                        lineHeight = 16.sp,
                        fontWeight = FontWeight.Black,
                    ),
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
                Row(
                    modifier = Modifier
                        .fillMaxWidth()
                        .horizontalScroll(rememberScrollState()),
                    horizontalArrangement = Arrangement.spacedBy(ThumbGap),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    planner.controls.forEach { control ->
                        if (control.options.isNotEmpty()) {
                            val open = openControlId == control.id
                            MenuDock(
                                launcherLabel = control.label,
                                launcherTestTag = "parity:altitude-planner-control:${control.id}",
                                optionTestTagPrefix = "parity:altitude-planner-option:${control.id}",
                                open = open,
                                onToggle = {
                                    openControlId = if (open) null else control.id
                                },
                                style = MenuDockStyle.AltitudePlanner,
                                disabled = !control.enabled,
                                disabledReason = control.disabledReason,
                                options = control.options.map { option ->
                                    MenuDockOption(
                                        key = option.actionUid,
                                        label = option.label,
                                        active = option.selected,
                                        aircraftSymbol = option.trailingSymbol,
                                        onSelect = { performAction(option.actionUid) },
                                    )
                                },
                            )
                        } else {
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
                                onClick = { control.actionUid?.let(::performAction) },
                            )
                        }
                    }
                    DepartureEditorRow(
                        departure = planner.departure,
                        timeValue = departureTimeInput,
                        whenValue = departureWhenInput,
                        onTimeValueChange = { departureTimeInput = it },
                        onWhenValueChange = { departureWhenInput = it },
                        onTimeFocusChange = { focused ->
                            if (departureTimeFocused && !focused &&
                                departureTimeInput != planner.departure.timeValue
                            ) {
                                setDepartureInput("time", departureTimeInput)
                            }
                            departureTimeFocused = focused
                        },
                        onWhenFocusChange = { focused ->
                            if (departureWhenFocused && !focused &&
                                departureWhenInput != planner.departure.whenValue
                            ) {
                                setDepartureInput("when", departureWhenInput)
                            }
                            departureWhenFocused = focused
                        },
                        onDone = { focusManager.clearFocus() },
                        onToggleBasis = {
                            focusManager.clearFocus()
                            toggleDepartureTimeBasis()
                        },
                        onDisabledClick = planner.departure.disabledReason?.let { reason ->
                            { showDisabledActionToast(context, reason) }
                        },
                    )
                }
            }

            planner.forecast?.let { forecast ->
                PlannerMessagePanel(
                    messages = listOf(forecast.summary),
                    foreground = uiTheme.controls.panelFg,
                    background = uiTheme.controls.panelBg,
                    actionLabel = forecast.action?.label,
                    actionEnabled = forecast.action?.enabled ?: false,
                    onAction = forecast.action?.actionUid?.let { actionUid ->
                        { performAction(actionUid) }
                    },
                    onDisabledAction = forecast.action?.disabledReason?.let { reason ->
                        { showDisabledActionToast(context, reason) }
                    },
                )
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
                        if (panel.advisories.isNotEmpty()) {
                            PlannerMessagePanel(
                                messages = panel.advisories,
                                foreground = uiTheme.controls.dataStatusWarningStroke,
                                background = uiTheme.controls.dataStatusWarningBg,
                            )
                        }
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
                if (userActionLoading || (loading && comparisonPanel == null)) {
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

        if (openControlId != null) {
            Scrim(
                modifier = Modifier.zIndex(OverlayPlaneModalScrim),
                onDismiss = { openControlId = null },
            )
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
private fun DepartureEditorRow(
    departure: AltitudePlannerDepartureEditorUiView,
    timeValue: String,
    whenValue: String,
    onTimeValueChange: (String) -> Unit,
    onWhenValueChange: (String) -> Unit,
    onTimeFocusChange: (Boolean) -> Unit,
    onWhenFocusChange: (Boolean) -> Unit,
    onDone: () -> Unit,
    onToggleBasis: () -> Unit,
    onDisabledClick: (() -> Unit)?,
) {
    val uiTheme = LocalAerobagUiTheme.current
    Surface(
        modifier = Modifier
            .width((ThumbSize * 6.2f) + DepartureWhenFieldWidth - (ThumbSize * 0.9f))
            .height(ThumbSize),
        color = uiTheme.controls.controlGroupBg,
        shape = RoundedCornerShape(ThumbRadius),
        border = BorderStroke(1.dp, uiTheme.controls.panelBorder),
    ) {
        Row(
            modifier = Modifier.padding(horizontal = ThumbSize * 0.12f),
            horizontalArrangement = Arrangement.spacedBy(ThumbGap),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            DepartureLabel(departure.title)
            DepartureLabel(departure.timeLabel)
            DepartureTextField(
                value = timeValue,
                enabled = departure.enabled,
                onValueChange = onTimeValueChange,
                onFocusChange = onTimeFocusChange,
                onDone = onDone,
            )
            CompactSquareButton(
                label = departure.basisLabel,
                modifier = Modifier
                    .width(ThumbSize * 1.45f)
                    .height(ThumbSize * 0.58f),
                maxLines = 1,
                onClick = onToggleBasis,
            )
            DepartureLabel(departure.whenLabel)
            DepartureTextField(
                value = whenValue,
                width = DepartureWhenFieldWidth,
                enabled = departure.enabled,
                warning = departure.whenIsPast,
                onValueChange = onWhenValueChange,
                onFocusChange = onWhenFocusChange,
                onDone = onDone,
            )
            DepartureLabel(departure.whenSuffix)
        }
    }
}

@Composable
private fun DepartureLabel(label: String) {
    if (label.isEmpty()) return
    val uiTheme = LocalAerobagUiTheme.current
    Text(
        text = label,
        color = uiTheme.controls.panelFg,
        style = MaterialTheme.typography.labelSmall.copy(
            fontSize = 10.sp,
            lineHeight = 10.sp,
            fontWeight = FontWeight.Bold,
        ),
        maxLines = 1,
    )
}

@Composable
private fun DepartureTextField(
    value: String,
    width: Dp = ThumbSize * 0.9f,
    enabled: Boolean,
    warning: Boolean = false,
    onValueChange: (String) -> Unit,
    onFocusChange: (Boolean) -> Unit,
    onDone: () -> Unit,
) {
    val uiTheme = LocalAerobagUiTheme.current
    val doneAction = rememberCurrentAction(onDone)
    Surface(
        modifier = Modifier
            .width(width)
            .height(ThumbSize * 0.58f),
        color = uiTheme.controls.textInputBg,
        shape = RoundedCornerShape(ThumbRadius),
        border = BorderStroke(
            if (warning) 2.dp else 1.dp,
            if (warning) uiTheme.controls.dataStatusWarningStroke else uiTheme.controls.panelBorder,
        ),
    ) {
        BasicTextField(
            value = value,
            onValueChange = onValueChange,
            modifier = Modifier
                .fillMaxSize()
                .onFocusChanged { onFocusChange(it.isFocused) }
                .padding(horizontal = ThumbSize * 0.1f),
            enabled = enabled,
            singleLine = true,
            textStyle = DepartureInputTextStyle.copy(
                color = uiTheme.controls.panelFg,
                textAlign = TextAlign.Center,
            ),
            keyboardOptions = KeyboardOptions(imeAction = ImeAction.Done),
            keyboardActions = KeyboardActions(onDone = { doneAction() }),
            decorationBox = { inner ->
                Box(modifier = Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
                    inner()
                }
            },
        )
    }
}

private val DepartureWhenFieldWidth = ThumbSize * 1.25f

private val DepartureInputTextStyle = TextStyle(
    fontSize = 12.sp,
    fontWeight = FontWeight.Bold,
)

@Composable
private fun PlannerMessagePanel(
    messages: List<String>,
    foreground: Color,
    background: Color,
    actionLabel: String? = null,
    actionEnabled: Boolean = false,
    onAction: (() -> Unit)? = null,
    onDisabledAction: (() -> Unit)? = null,
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
        if (actionLabel != null) {
            CompactSquareButton(
                label = actionLabel,
                modifier = Modifier
                    .width(ThumbSize * 3f)
                    .height(ThumbSize * 0.72f),
                enabled = actionEnabled,
                maxLines = 2,
                testTag = "parity:altitude-planner-forecast-action",
                onDisabledClick = onDisabledAction,
                onClick = { onAction?.invoke() },
            )
        }
    }
}
