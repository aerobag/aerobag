package org.aerobag.app

import android.content.Context
import android.graphics.BitmapFactory
import android.os.SystemClock
import android.util.Log
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.asImageBitmap
import java.util.LinkedHashMap
import java.util.concurrent.Executors
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.atomic.AtomicInteger
import java.util.concurrent.atomic.AtomicLong
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.asCoroutineDispatcher
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.ensureActive
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import org.aerobag.app.domain.RenderTile
import org.aerobag.app.domain.RenderTileKey
import org.aerobag.app.domain.SectionalPackages
import org.aerobag.app.domain.renderTileKey

internal data class TileRect(
    val leftPx: Int,
    val topPx: Int,
    val widthPx: Int,
    val heightPx: Int,
)

internal data class LoadedTileBitmap(
    val key: RenderTileKey,
    val bitmap: ImageBitmap?,
    val bytes: Int,
    val decodedBytes: Long,
    val readMs: Long,
    val decodeMs: Long,
)

internal data class LoadedRenderTileBitmap(
    val tile: RenderTile,
    val result: LoadedTileBitmap,
)

internal data class TileLoadWork(
    val generationId: Long,
    val mapId: String,
    val tile: RenderTile,
    val result: CompletableDeferred<LoadedRenderTileBitmap?>,
)

internal data class DecodedTileCacheEntry(
    val bitmap: ImageBitmap,
    val decodedBytes: Long,
)

internal class DecodedTileBitmapCache(
    private val maxBytes: Long,
) {
    private val entries = LinkedHashMap<String, DecodedTileCacheEntry>(256, 0.75f, true)
    private var currentBytes = 0L

    @Synchronized
    fun get(key: String): ImageBitmap? = entries[key]?.bitmap

    @Synchronized
    fun put(key: String, bitmap: ImageBitmap, decodedBytes: Long) {
        val previous = entries.remove(key)
        if (previous != null) {
            currentBytes -= previous.decodedBytes
        }
        entries[key] = DecodedTileCacheEntry(bitmap, decodedBytes.coerceAtLeast(1L))
        currentBytes += decodedBytes.coerceAtLeast(1L)
        trimToBudget()
    }

    @Synchronized
    fun clear() {
        entries.clear()
        currentBytes = 0L
    }

    @Synchronized
    fun stats(): DecodedTileCacheStats =
        DecodedTileCacheStats(entries = entries.size, bytes = currentBytes)

    private fun trimToBudget() {
        val iterator = entries.entries.iterator()
        while (currentBytes > maxBytes && iterator.hasNext()) {
            val eldest = iterator.next()
            currentBytes -= eldest.value.decodedBytes
            iterator.remove()
        }
    }
}

internal data class DecodedTileCacheStats(
    val entries: Int,
    val bytes: Long,
)

internal data class OverlaySurfaceUnits(
    val width: Float,
    val height: Float,
)

internal class RasterTileBitmapLoader(
    private val context: Context,
    scope: CoroutineScope,
    workerCount: Int = MapTileLoadWorkerCount,
) {
    private val workerThreadIds = AtomicInteger()
    private val workerDispatcher = Executors.newFixedThreadPool(workerCount) { task ->
        Thread(task, "AerobagRasterTile-${workerThreadIds.incrementAndGet()}").apply {
            isDaemon = true
        }
    }.asCoroutineDispatcher()
    private val workerScope = CoroutineScope(SupervisorJob(scope.coroutineContext[Job]) + workerDispatcher)
    private val closed = AtomicBoolean(false)
    private val latestGenerationId = AtomicLong()
    private val queueSignal = Channel<Unit>(capacity = Channel.UNLIMITED)
    private val queueMutex = Mutex()
    private val pendingWork = ArrayDeque<TileLoadWork>()

    init {
        repeat(workerCount) { workerIndex ->
            workerScope.launch {
                Log.i(TileBudgetLogTag, "worker-start worker=$workerIndex")
                try {
                    while (true) {
                        if (queueSignal.receiveCatching().isClosed) {
                            break
                        }
                        while (true) {
                            val work = queueMutex.withLock {
                                while (pendingWork.isNotEmpty() && pendingWork.first().generationId != latestGenerationId.get()) {
                                    pendingWork.removeFirst().result.complete(null)
                                }
                                pendingWork.removeFirstOrNull()
                            } ?: break
                            if (work.generationId != latestGenerationId.get()) {
                                work.result.complete(null)
                                continue
                            }
                            try {
                                currentCoroutineContext().ensureActive()
                                val workerStartMs = SystemClock.elapsedRealtime()
                                val result = loadOneVisibleTileBitmap(context, work.mapId, work.generationId, work.tile)
                                val workerElapsedMs = SystemClock.elapsedRealtime() - workerStartMs
                                if (workerElapsedMs >= SlowTileLoadLogMs) {
                                    Log.w(
                                        TileBudgetLogTag,
                                        "tile-slow gen=${work.generationId} worker=$workerIndex elapsedMs=$workerElapsedMs loaded=${result.bitmap != null} bytes=${result.bytes} readMs=${result.readMs} decodeMs=${result.decodeMs} ${formatTileRef(work.tile)}",
                                    )
                                }
                                if (work.generationId == latestGenerationId.get()) {
                                    work.result.complete(LoadedRenderTileBitmap(work.tile, result))
                                } else {
                                    work.result.complete(null)
                                }
                            } catch (error: CancellationException) {
                                work.result.cancel()
                                throw error
                            } catch (error: Throwable) {
                                Log.e(TileBudgetLogTag, "worker failed worker=$workerIndex gen=${work.generationId} ${formatTileRef(work.tile)}", error)
                                work.result.complete(null)
                            }
                        }
                    }
                } finally {
                    Log.w(TileBudgetLogTag, "worker-stop worker=$workerIndex")
                }
            }
        }
    }

    fun close() {
        if (!closed.compareAndSet(false, true)) {
            return
        }
        queueSignal.close()
        workerScope.coroutineContext[Job]?.cancel()
        workerDispatcher.close()
    }

    suspend fun loadVisibleTileBitmaps(
        mapId: String,
        generationId: Long,
        missingTiles: List<RenderTile>,
        onTileLoaded: suspend (LoadedRenderTileBitmap) -> Unit = {},
    ): List<LoadedRenderTileBitmap> {
        if (missingTiles.isEmpty()) {
            return emptyList()
        }
        if (closed.get()) {
            return emptyList()
        }
        latestGenerationId.set(generationId)
        val batchStartMs = SystemClock.elapsedRealtime()
        val deferredResults = missingTiles.map { tile ->
            TileLoadWork(
                generationId = generationId,
                mapId = mapId,
                tile = tile,
                result = CompletableDeferred(),
            )
        }
        Log.i(
            TileBudgetLogTag,
            "load-start gen=$generationId map=$mapId missing=${missingTiles.size} workers=$MapTileLoadWorkerCount groups=[${formatTileBudgetSummary(missingTiles)}] first=${missingTiles.firstOrNull()?.let(::formatTileRef) ?: "none"}",
        )
        try {
            val droppedCount = queueMutex.withLock {
                val dropped = pendingWork.size
                while (pendingWork.isNotEmpty()) {
                    pendingWork.removeFirst().result.complete(null)
                }
                pendingWork.addAll(deferredResults)
                dropped
            }
            repeat(MapTileLoadWorkerCount) {
                if (queueSignal.trySend(Unit).isFailure) {
                    return emptyList()
                }
            }
            Log.i(
                TileBudgetLogTag,
                "load-enqueued gen=$generationId map=$mapId count=${missingTiles.size} droppedQueued=$droppedCount enqueueMs=${SystemClock.elapsedRealtime() - batchStartMs}",
            )
            val loadedTiles = mutableListOf<LoadedRenderTileBitmap>()
            deferredResults.forEach { work ->
                currentCoroutineContext().ensureActive()
                val loaded = work.result.await() ?: return@forEach
                loadedTiles += loaded
                onTileLoaded(loaded)
            }
            return loadedTiles
        } catch (error: CancellationException) {
            deferredResults.forEach { work ->
                work.result.cancel()
            }
            Log.w(
                TileBudgetLogTag,
                "load-cancel gen=$generationId map=$mapId missing=${missingTiles.size} elapsedMs=${SystemClock.elapsedRealtime() - batchStartMs}",
            )
            throw error
        }
    }
}

internal suspend fun loadOneVisibleTileBitmap(
    context: Context,
    mapId: String,
    generationId: Long,
    tile: RenderTile,
): LoadedTileBitmap {
    val key = renderTileKey(tile)
    return try {
        val readStartMs = SystemClock.elapsedRealtime()
        val bytes = SectionalPackages.loadTileBytes(context, tile)
        val readMs = SystemClock.elapsedRealtime() - readStartMs
        currentCoroutineContext().ensureActive()
        if (bytes == null) {
            LoadedTileBitmap(key, null, 0, 0L, readMs, 0L)
        } else {
            val decodeStartMs = SystemClock.elapsedRealtime()
            val bitmap = BitmapFactory.decodeByteArray(bytes, 0, bytes.size)
            val decodeMs = SystemClock.elapsedRealtime() - decodeStartMs
            currentCoroutineContext().ensureActive()
            LoadedTileBitmap(key, bitmap?.asImageBitmap(), bytes.size, bitmap?.byteCount?.toLong() ?: 0L, readMs, decodeMs)
        }
    } catch (error: CancellationException) {
        throw error
    } catch (error: Throwable) {
        Log.w(
            TileBudgetLogTag,
            "tile load failed gen=$generationId map=$mapId ${formatTileRef(tile)}",
            error,
        )
        LoadedTileBitmap(key, null, 0, 0L, 0L, 0L)
    }
}

internal fun formatTileBudgetSummary(
    tiles: List<RenderTile>,
): String {
    val counts = linkedMapOf<String, Int>()
    tiles.forEach { tile ->
        val packageLabel = tile.sources.firstOrNull()?.packageName ?: tile.mapViewId
        val key = "$packageLabel@z${tile.zoom}"
        counts[key] = (counts[key] ?: 0) + 1
    }
    return counts.entries
        .sortedBy { it.key }
        .joinToString(", ") { entry -> "${entry.key}=${entry.value}" }
}

internal fun formatTileRef(tile: RenderTile): String =
    "package=${tile.sources.firstOrNull()?.packageName ?: tile.mapViewId} storage=${tile.sources.firstOrNull()?.storageKind} z=${tile.zoom} x=${tile.x} y=${tile.yTms} candidates=${tile.sources.size}"

internal fun decodedTileCacheKey(tile: RenderTile): String {
    val candidates = tile.sources
        .distinctBy { "${it.packageName}:${it.storageKind}:${it.path}" }
        .joinToString("|") { source ->
            "${source.packageName}:${source.storageKind}:${source.path}"
        }
    return "${tile.zoom}:${tile.x}:${tile.yTms}:$candidates"
}
