package net.jonh.aerobag.prototype

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.material3.Card
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.RadioButton
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import net.jonh.aerobag.prototype.domain.AppCoreAdapter
import net.jonh.aerobag.prototype.domain.AppState
import net.jonh.aerobag.prototype.domain.ContentPolicy
import net.jonh.aerobag.prototype.domain.MockAppCoreAdapter
import net.jonh.aerobag.prototype.domain.NativeAppCoreAdapter
import net.jonh.aerobag.prototype.domain.SampleData

class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContent {
            MaterialTheme {
                Surface(
                    modifier = Modifier.fillMaxSize(),
                    color = Color(0xFFF3EFE4),
                ) {
                    ContentPrototypeScreen()
                }
            }
        }
    }
}

@Composable
private fun ContentPrototypeScreen() {
    val appCoreResult = remember {
        runCatching {
            ContentBackend(
                label = "NATIVE",
                adapter = NativeAppCoreAdapter(),
            )
        }.getOrElse {
            ContentBackend(
                label = "MOCK",
                adapter = MockAppCoreAdapter(),
            )
        }
    }
    val appCore: AppCoreAdapter = appCoreResult.adapter
    var state by remember {
        mutableStateOf(
            appCore.refreshContent(
                appCore.setContentPolicy(
                    appCore.replaceFlightPlan(
                        AppState(),
                        SampleData.catalog,
                        SampleData.samplePlan,
                    ),
                    ContentPolicy.StreamAllowed,
                ),
                SampleData.remoteOnlyInventory,
            ),
        )
    }
    var inventoryMode by remember { mutableStateOf("remote") }
    val listState = rememberLazyListState()

    LazyColumn(
        modifier = Modifier
            .fillMaxSize()
            .padding(20.dp),
        state = listState,
        verticalArrangement = Arrangement.spacedBy(18.dp),
    ) {
        item {
            Text(
                text = "Avare Android Prototype",
                style = MaterialTheme.typography.labelLarge,
                color = Color(0xFF0D6F67),
            )
        }
        item {
            Text(
                text = "Backend ${appCoreResult.label}",
                style = MaterialTheme.typography.labelMedium,
                color = Color(0xFF5F6F76),
            )
        }
        item {
            Text(
                text = "Content parity without offline symmetry",
                style = MaterialTheme.typography.headlineMedium,
                fontWeight = FontWeight.Bold,
            )
        }
        item {
            Text(
                text = "This mirrors the web content slice while keeping Android native and offline-first.",
                style = MaterialTheme.typography.bodyLarge,
                color = Color(0xFF5F6F76),
            )
        }

        item {
            Card(modifier = Modifier.fillMaxWidth()) {
                Column(modifier = Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
                    Text("Prototype inputs", fontWeight = FontWeight.Bold)
                    Text("Cycle: ${SampleData.catalog.cycle}")
                    Text("Plan: ${SampleData.samplePlan.name}")
                    Text("Route: ${SampleData.samplePlan.departure} to ${SampleData.samplePlan.destination}")
                }
            }
        }

        item {
            Card(modifier = Modifier.fillMaxWidth()) {
                Column(modifier = Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
                    Text("Content policy", fontWeight = FontWeight.Bold)
                    PolicyOption(state.contentPolicy, ContentPolicy.StreamAllowed) {
                        state = appCore.setContentPolicy(state, it)
                        state = appCore.refreshContent(
                            state,
                            if (inventoryMode == "installed") SampleData.installedInventory else SampleData.remoteOnlyInventory,
                        )
                    }
                    PolicyOption(state.contentPolicy, ContentPolicy.PreferLocal) {
                        state = appCore.setContentPolicy(state, it)
                        state = appCore.refreshContent(
                            state,
                            if (inventoryMode == "installed") SampleData.installedInventory else SampleData.remoteOnlyInventory,
                        )
                    }
                    PolicyOption(state.contentPolicy, ContentPolicy.OfflineRequired) {
                        state = appCore.setContentPolicy(state, it)
                        state = appCore.refreshContent(
                            state,
                            if (inventoryMode == "installed") SampleData.installedInventory else SampleData.remoteOnlyInventory,
                        )
                    }
                }
            }
        }

        item {
            Card(modifier = Modifier.fillMaxWidth()) {
                Column(modifier = Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
                    Text("Inventory mode", fontWeight = FontWeight.Bold)
                    InventoryOption(inventoryMode, "remote", "Remote only cache") {
                        inventoryMode = it
                        state = appCore.refreshContent(state, SampleData.remoteOnlyInventory)
                    }
                    InventoryOption(inventoryMode, "installed", "Installed package") {
                        inventoryMode = it
                        state = appCore.refreshContent(state, SampleData.installedInventory)
                    }
                }
            }
        }

        item {
            Card(
                modifier = Modifier
                    .fillMaxWidth()
                    .background(Color.White),
            ) {
                Column(modifier = Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
                    Text("Shared-state result", fontWeight = FontWeight.Bold)
                    Text(
                        text = if (state.lastContentReport?.fullySatisfied == true) {
                            "Ready for this policy"
                        } else {
                            "Coverage gap"
                        },
                        color = if (state.lastContentReport?.fullySatisfied == true) Color(0xFF0D6F67) else Color(0xFF935224),
                        fontWeight = FontWeight.SemiBold,
                    )
                    Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
                        state.lastContentReport?.items.orEmpty().forEach { item ->
                            Card(modifier = Modifier.fillMaxWidth()) {
                                Column(modifier = Modifier.padding(14.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
                                    Text(item.label, fontWeight = FontWeight.SemiBold)
                                    Text(item.availability.availability.name, color = Color(0xFF5F6F76))
                                    Text(
                                        "offline ${if (item.availability.offlineUsable) "yes" else "no"} • cached ${if (item.availability.cached) "yes" else "no"}",
                                        color = Color(0xFF5F6F76),
                                    )
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

private data class ContentBackend(
    val label: String,
    val adapter: AppCoreAdapter,
)

@Composable
private fun PolicyOption(
    selected: ContentPolicy,
    option: ContentPolicy,
    onSelected: (ContentPolicy) -> Unit,
) {
    Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        RadioButton(
            selected = selected == option,
            onClick = { onSelected(option) },
        )
        Text(option.name)
    }
}

@Composable
private fun InventoryOption(
    selected: String,
    option: String,
    label: String,
    onSelected: (String) -> Unit,
) {
    Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        RadioButton(
            selected = selected == option,
            onClick = { onSelected(option) },
        )
        Text(label)
    }
}
