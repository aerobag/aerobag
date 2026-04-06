package net.jonh.aerobag.prototype

import android.graphics.BitmapFactory
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
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
import androidx.compose.ui.Alignment
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.dp
import net.jonh.aerobag.prototype.domain.AppCoreAdapter
import net.jonh.aerobag.prototype.domain.AppState
import net.jonh.aerobag.prototype.domain.ContentPolicy
import net.jonh.aerobag.prototype.domain.MapChartFamily
import net.jonh.aerobag.prototype.domain.MapLookupAdapter
import net.jonh.aerobag.prototype.domain.MockAppCoreAdapter
import net.jonh.aerobag.prototype.domain.MockMapLookupAdapter
import net.jonh.aerobag.prototype.domain.NativeAppCoreAdapter
import net.jonh.aerobag.prototype.domain.NativeMapLookupAdapter
import net.jonh.aerobag.prototype.domain.SampleData
import net.jonh.aerobag.prototype.domain.tileAssetPath
import net.jonh.aerobag.prototype.domain.tileCells

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
    val context = LocalContext.current
    val fixture = remember(context) { SampleData.load(context.applicationContext) }
    val appCoreResult = remember(fixture) {
        runCatching {
            ContentBackend(
                label = "NATIVE",
                adapter = NativeAppCoreAdapter(fixture.catalog),
                mapLookup = NativeMapLookupAdapter(),
            )
        }.getOrElse {
            ContentBackend(
                label = "MOCK",
                adapter = MockAppCoreAdapter(),
                mapLookup = MockMapLookupAdapter(),
            )
        }
    }
    val appCore: AppCoreAdapter = appCoreResult.adapter
    val mapLookup: MapLookupAdapter = appCoreResult.mapLookup
    var state by remember {
        mutableStateOf(
            appCore.refreshContent(
                appCore.setContentPolicy(
                    appCore.replaceFlightPlan(
                        AppState(),
                        fixture.catalog,
                        fixture.samplePlan,
                    ),
                    ContentPolicy.StreamAllowed,
                ),
                fixture.remoteOnlyInventory,
            ),
        )
    }
    var inventoryMode by remember { mutableStateOf("remote") }
    var selectedFamily by remember { mutableStateOf(fixture.initialProbe.family) }
    var probeLat by remember { mutableStateOf(fixture.initialProbe.lat) }
    var probeLon by remember { mutableStateOf(fixture.initialProbe.lon) }
    var selectedChart by remember {
        mutableStateOf(
            mapLookup.chartForPosition(
                catalogJson = fixture.catalogJson,
                geometryJson = fixture.geometryJson,
                family = fixture.initialProbe.family,
                lat = fixture.initialProbe.lat,
                lon = fixture.initialProbe.lon,
            ),
        )
    }
    val listState = rememberLazyListState()
    val offshoreLat = fixture.initialProbe.lat + 4.0
    val offshoreLon = fixture.initialProbe.lon + 4.0

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
                    Text("Cycle: ${fixture.catalog.cycle}")
                    Text("Plan: ${fixture.samplePlan.name}")
                    Text("Route: ${fixture.samplePlan.departure} to ${fixture.samplePlan.destination}")
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
                            if (inventoryMode == "installed") fixture.installedInventory else fixture.remoteOnlyInventory,
                        )
                    }
                    PolicyOption(state.contentPolicy, ContentPolicy.PreferLocal) {
                        state = appCore.setContentPolicy(state, it)
                        state = appCore.refreshContent(
                            state,
                            if (inventoryMode == "installed") fixture.installedInventory else fixture.remoteOnlyInventory,
                        )
                    }
                    PolicyOption(state.contentPolicy, ContentPolicy.OfflineRequired) {
                        state = appCore.setContentPolicy(state, it)
                        state = appCore.refreshContent(
                            state,
                            if (inventoryMode == "installed") fixture.installedInventory else fixture.remoteOnlyInventory,
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
                        state = appCore.refreshContent(state, fixture.remoteOnlyInventory)
                    }
                    InventoryOption(inventoryMode, "installed", "Installed package") {
                        inventoryMode = it
                        state = appCore.refreshContent(state, fixture.installedInventory)
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

        item {
            Card(modifier = Modifier.fillMaxWidth()) {
                Column(modifier = Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
                    Text("Map lookup", fontWeight = FontWeight.Bold)
                    Text(
                        text = "The shared Rust core now picks a chart from catalog metadata and transformed cutline geometry.",
                        color = Color(0xFF5F6F76),
                    )

                    Text("Chart family", fontWeight = FontWeight.SemiBold)
                    MapFamilyOption(selectedFamily, MapChartFamily.Tac) {
                        selectedFamily = it
                        selectedChart = mapLookup.chartForPosition(
                            catalogJson = fixture.catalogJson,
                            geometryJson = fixture.geometryJson,
                            family = it,
                            lat = probeLat,
                            lon = probeLon,
                        )
                    }
                    MapFamilyOption(selectedFamily, MapChartFamily.Sectional) {
                        selectedFamily = it
                        selectedChart = mapLookup.chartForPosition(
                            catalogJson = fixture.catalogJson,
                            geometryJson = fixture.geometryJson,
                            family = it,
                            lat = probeLat,
                            lon = probeLon,
                        )
                    }

                    Text("Probe point", fontWeight = FontWeight.SemiBold)
                    InventoryOption(
                        selected = if (probeLat == fixture.initialProbe.lat && probeLon == fixture.initialProbe.lon) "inside" else "outside",
                        option = "inside",
                        label = "Boston center",
                    ) {
                        probeLat = fixture.initialProbe.lat
                        probeLon = fixture.initialProbe.lon
                        selectedChart = mapLookup.chartForPosition(
                            catalogJson = fixture.catalogJson,
                            geometryJson = fixture.geometryJson,
                            family = selectedFamily,
                            lat = probeLat,
                            lon = probeLon,
                        )
                    }
                    InventoryOption(
                        selected = if (probeLat == offshoreLat && probeLon == offshoreLon) "outside" else "inside",
                        option = "outside",
                        label = "Offshore gap",
                    ) {
                        probeLat = offshoreLat
                        probeLon = offshoreLon
                        selectedChart = mapLookup.chartForPosition(
                            catalogJson = fixture.catalogJson,
                            geometryJson = fixture.geometryJson,
                            family = selectedFamily,
                            lat = probeLat,
                            lon = probeLon,
                        )
                    }

                    Text("Lat: ${"%.4f".format(probeLat)}")
                    Text("Lon: ${"%.4f".format(probeLon)}")
                    Text("Chart: ${selectedChart?.displayName ?: "No matching chart"}")
                    if (selectedChart?.displayName == fixture.mapTileView.chartName) {
                        TileViewport(view = fixture.mapTileView)
                    } else {
                        Text(
                            text = "No tile viewport is loaded for this chart selection yet.",
                            color = Color(0xFF5F6F76),
                        )
                    }
                }
            }
        }
    }
}

private data class ContentBackend(
    val label: String,
    val adapter: AppCoreAdapter,
    val mapLookup: MapLookupAdapter,
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

@Composable
private fun MapFamilyOption(
    selected: MapChartFamily,
    option: MapChartFamily,
    onSelected: (MapChartFamily) -> Unit,
) {
    Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        RadioButton(
            selected = selected == option,
            onClick = { onSelected(option) },
        )
        Text(
            when (option) {
                MapChartFamily.Sectional -> "Sectional"
                MapChartFamily.Tac -> "TAC"
            },
        )
    }
}

@Composable
private fun TileViewport(view: net.jonh.aerobag.prototype.domain.MapTileView) {
    val context = LocalContext.current
    val density = LocalDensity.current
    val tiles = remember(view) { tileCells(view) }
    val tileSize = 128.dp
    val viewportSize = tileSize * (view.radius * 2 + 1)
    val tileSizePx = with(density) { tileSize.toPx() }

    Box(
        modifier = Modifier
            .horizontalScroll(rememberScrollState())
            .padding(top = 8.dp),
    ) {
        Box(
            modifier = Modifier
                .size(viewportSize)
                .border(1.dp, Color(0x1F182128), RoundedCornerShape(18.dp)),
        ) {
            Column {
                tiles.chunked(view.radius * 2 + 1).forEach { row ->
                    Row {
                        row.forEach { tile ->
                            val bitmap = remember(tile.x, tile.yTms) {
                                context.assets.open(tileAssetPath(view, tile)).use { stream ->
                                    BitmapFactory.decodeStream(stream)?.asImageBitmap()
                                }
                            }
                            if (bitmap != null) {
                                Image(
                                    bitmap = bitmap,
                                    contentDescription = "Chart tile ${tile.x}/${tile.yTms}",
                                    modifier = Modifier.size(tileSize),
                                )
                            } else {
                                Box(modifier = Modifier.size(tileSize).background(Color(0x14000000)))
                            }
                        }
                    }
                }
            }

            Box(
                modifier = Modifier
                    .align(Alignment.TopStart)
                    .offset {
                        IntOffset(
                            x = ((view.radius + view.probeOffsetX) * tileSizePx).toInt() - 7,
                            y = ((view.radius + view.probeOffsetY) * tileSizePx).toInt() - 7,
                        )
                    }
                    .size(14.dp)
                    .background(Color(0xFFE44D2E), CircleShape)
                    .border(2.dp, Color.White, CircleShape),
            )
        }
    }
}
