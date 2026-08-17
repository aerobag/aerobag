// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app

import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.zIndex
import com.google.mlkit.vision.barcode.common.Barcode
import com.google.mlkit.vision.codescanner.GmsBarcodeScannerOptions
import com.google.mlkit.vision.codescanner.GmsBarcodeScanning
import java.util.concurrent.atomic.AtomicBoolean
import org.aerobag.app.domain.NavElementUiView
import org.aerobag.app.generated.CloudPlatformEffect
import org.aerobag.app.generated.CloudUiActionId
import org.aerobag.app.generated.CloudUiFieldId
import org.aerobag.app.generated.CloudUiFieldValue
import org.aerobag.app.generated.UiCloudAction
import org.aerobag.app.generated.UiCloudPageState
import org.aerobag.app.generated.UiCloudPanel
import org.aerobag.app.generated.UiCloudPanelControl
import org.aerobag.app.generated.UiCloudPanelState
import org.aerobag.app.generated.UiQrCode

@Composable
internal fun CloudPage(
    page: AppPage,
    state: UiCloudPageState,
    navElement: NavElementUiView?,
    mostRecentChartOrPlatePage: AppPage,
    onOpenPlan: () -> Unit,
    onOpenRecentChartOrPlate: () -> Unit,
    onSelectPage: (AppPage) -> Unit,
    onAction: (CloudUiActionId, List<CloudUiFieldValue>) -> Boolean,
) {
    val uiTheme = LocalAerobagUiTheme.current
    val context = LocalContext.current
    val fields = remember { mutableStateMapOf<CloudUiFieldId, String>() }
    val currentOnAction = rememberUpdatedState(onAction)
    val compositionActive = remember { AtomicBoolean(true) }
    DisposableEffect(Unit) {
        compositionActive.set(true)
        onDispose { compositionActive.set(false) }
    }
    val qrScanner = remember(context) {
        val options = GmsBarcodeScannerOptions.Builder()
            .setBarcodeFormats(Barcode.FORMAT_QR_CODE)
            .enableAutoZoom()
            .build()
        GmsBarcodeScanning.getClient(context, options)
    }

    fun invoke(action: UiCloudAction) {
        val values = fields.map { CloudUiFieldValue(it.key, it.value) }
        if (!onAction(action.id, values)) return
        when (val effect = action.platformEffect) {
            is CloudPlatformEffect.BeginAuthorization -> Unit
            is CloudPlatformEffect.ScanQrCode -> {
                qrScanner.startScan()
                    .addOnSuccessListener { barcode ->
                        if (!compositionActive.get()) return@addOnSuccessListener
                        val setupCode = barcode.rawValue?.trim().orEmpty()
                        if (setupCode.isNotEmpty()) {
                            currentOnAction.value(
                                effect.completionAction,
                                listOf(CloudUiFieldValue(effect.fieldId, setupCode)),
                            )
                        } else {
                            showActionToast(
                                context,
                                "The QR code contained no Device Setup Code.",
                                long = true,
                            )
                        }
                    }
                    .addOnFailureListener { error ->
                        if (!compositionActive.get()) return@addOnFailureListener
                        showActionToast(
                            context,
                            "Could not scan QR code: ${error.message ?: "scanner failed"}",
                            long = true,
                        )
                    }
            }
            is CloudPlatformEffect.CopyText -> {
                val clipboard = context.getSystemService(Context.CLIPBOARD_SERVICE) as ClipboardManager
                clipboard.setPrimaryClip(ClipData.newPlainText("Aerobag Device Setup Code", effect.text))
            }
            null -> Unit
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
                .verticalScroll(rememberScrollState()),
            verticalArrangement = Arrangement.spacedBy(ThumbGap),
        ) {
            Text(
                text = state.title.uppercase(),
                style = MaterialTheme.typography.headlineSmall.copy(
                    fontSize = 18.sp,
                    lineHeight = 20.sp,
                    fontWeight = FontWeight.Black,
                ),
                color = uiTheme.controls.panelFg,
            )
            Text(
                text = state.summary,
                style = MaterialTheme.typography.bodyLarge.copy(fontWeight = FontWeight.Bold),
                color = uiTheme.controls.panelMuted,
            )
            BoxWithConstraints(modifier = Modifier.fillMaxWidth()) {
                val twoColumns = maxWidth >= ThumbSize * 12f && state.providerCard != null
                if (twoColumns) {
                    Row(
                        modifier = Modifier.fillMaxWidth(),
                        horizontalArrangement = Arrangement.spacedBy(ThumbGap),
                    ) {
                        CloudPanelColumn(
                            heading = state.syncAccountHeading,
                            panels = state.syncAccountPanels,
                            fields = fields,
                            onInvoke = ::invoke,
                            modifier = Modifier.weight(1f),
                        )
                        CloudPanelColumn(
                            heading = state.providerHeading,
                            panels = listOfNotNull(state.providerCard),
                            fields = fields,
                            onInvoke = ::invoke,
                            modifier = Modifier.weight(1f),
                        )
                    }
                } else {
                    CloudPanelColumn(
                        heading = state.syncAccountHeading,
                        panels = state.syncAccountPanels + listOfNotNull(state.providerCard),
                        fields = fields,
                        onInvoke = ::invoke,
                        modifier = Modifier.fillMaxWidth(),
                    )
                }
            }
            CloudPanelView(
                panel = state.overallStatus,
                fields = fields,
                onInvoke = ::invoke,
                modifier = Modifier.fillMaxWidth(),
            )
        }
    }
}

@Composable
private fun CloudPanelColumn(
    heading: String,
    panels: List<UiCloudPanel>,
    fields: MutableMap<CloudUiFieldId, String>,
    onInvoke: (UiCloudAction) -> Unit,
    modifier: Modifier,
) {
    val uiTheme = LocalAerobagUiTheme.current
    Column(modifier = modifier, verticalArrangement = Arrangement.spacedBy(ThumbGap)) {
        Text(
            text = heading.uppercase(),
            style = MaterialTheme.typography.titleMedium.copy(fontWeight = FontWeight.Black),
            color = uiTheme.controls.panelFg,
        )
        panels.forEach { panel ->
            CloudPanelView(panel, fields, onInvoke, Modifier.fillMaxWidth())
        }
    }
}

@Composable
private fun CloudPanelView(
    panel: UiCloudPanel,
    fields: MutableMap<CloudUiFieldId, String>,
    onInvoke: (UiCloudAction) -> Unit,
    modifier: Modifier,
) {
    val uiTheme = LocalAerobagUiTheme.current
    val accent = when (panel.state) {
        UiCloudPanelState.Error, UiCloudPanelState.Caution -> uiTheme.controls.dataStatusWarningStroke
        UiCloudPanelState.Complete -> uiTheme.controls.buttonChecked
        else -> uiTheme.controls.panelBorder
    }
    Column(
        modifier = modifier
            .border(2.dp, accent, RoundedCornerShape(ThumbRadius))
            .background(uiTheme.controls.panelBg, RoundedCornerShape(ThumbRadius))
            .padding(ThumbSize * 0.22f),
        verticalArrangement = Arrangement.spacedBy(ThumbGap),
    ) {
        Row(
            modifier = Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.SpaceBetween,
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text(
                text = panel.title,
                modifier = Modifier.weight(1f),
                style = MaterialTheme.typography.titleMedium.copy(fontWeight = FontWeight.Black),
                color = uiTheme.controls.panelFg,
                maxLines = 2,
                overflow = TextOverflow.Ellipsis,
            )
            panel.stateLabel?.let {
                Text(
                    text = it.uppercase(),
                    style = MaterialTheme.typography.labelLarge.copy(fontWeight = FontWeight.Black),
                    color = accent,
                )
            }
        }
        panel.summary?.let {
            Text(
                text = it,
                style = MaterialTheme.typography.bodyLarge.copy(fontWeight = FontWeight.Bold),
                color = uiTheme.controls.panelMuted,
            )
        }
        panel.timeFacts.forEach { fact ->
            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.spacedBy(ThumbGap),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Text(
                    text = fact.label,
                    modifier = Modifier.weight(1f),
                    style = MaterialTheme.typography.bodyMedium.copy(fontWeight = FontWeight.Bold),
                    color = uiTheme.controls.panelMuted,
                )
                Text(
                    text = fact.value,
                    style = MaterialTheme.typography.bodyMedium.copy(fontWeight = FontWeight.Black),
                    color = uiTheme.controls.panelFg,
                )
            }
        }
        when (val control = panel.control) {
            is UiCloudPanelControl.DeviceSetupCodeInput -> OutlinedTextField(
                value = fields[control.fieldId].orEmpty(),
                onValueChange = { fields[control.fieldId] = it },
                modifier = Modifier
                    .fillMaxWidth()
                    .height(ThumbSize * 1.8f),
                label = { Text(control.label) },
                placeholder = { Text(control.placeholder) },
                textStyle = MaterialTheme.typography.bodyMedium,
            )
            is UiCloudPanelControl.DeviceSetupCodeOutput -> {
                CloudQrCode(control.qrCode)
                Text(
                    text = control.setupCode,
                    style = MaterialTheme.typography.bodyMedium,
                    color = uiTheme.controls.panelFg,
                )
                CloudActionButton(control.copyAction, fields, onInvoke)
            }
            null -> Unit
        }
        panel.actions.forEach { CloudActionButton(it, fields, onInvoke) }
    }
}

@Composable
private fun CloudQrCode(code: UiQrCode) {
    BoxWithConstraints(modifier = Modifier.fillMaxWidth()) {
        val side = minOf(maxWidth, ThumbSize * 4.5f)
        val quietZone = code.quietZoneModules
        val moduleCount = code.rows.size + (quietZone * 2)
        Canvas(
            modifier = Modifier
                .width(side)
                .height(side)
                .align(Alignment.Center)
                .semantics { contentDescription = code.accessibilityLabel },
        ) {
            drawRect(Color.White)
            if (moduleCount <= 0) return@Canvas
            val moduleSize = (size.minDimension / moduleCount.toFloat())
                .toInt()
                .coerceAtLeast(1)
                .toFloat()
            val renderedSize = moduleSize * moduleCount
            val origin = (size.minDimension - renderedSize) / 2f
            code.rows.forEachIndexed { rowIndex, row ->
                row.forEachIndexed { columnIndex, value ->
                    if (value == '1') {
                        drawRect(
                            color = Color.Black,
                            topLeft = Offset(
                                origin + (columnIndex + quietZone) * moduleSize,
                                origin + (rowIndex + quietZone) * moduleSize,
                            ),
                            size = Size(moduleSize, moduleSize),
                        )
                    }
                }
            }
        }
    }
}

@Composable
private fun CloudActionButton(
    action: UiCloudAction,
    fields: Map<CloudUiFieldId, String>,
    onInvoke: (UiCloudAction) -> Unit,
) {
    val context = LocalContext.current
    val enabled = action.enabled && action.requiredFields.all { fields[it]?.isNotBlank() == true }
    CompactSquareButton(
        label = action.label,
        modifier = Modifier
            .fillMaxWidth()
            .height(ThumbSize),
        maxLines = 2,
        enabled = enabled,
        wide = true,
        onDisabledClick = action.disabledReason?.let { reason ->
            { showDisabledActionToast(context, reason) }
        },
        onClick = { onInvoke(action) },
    )
}
