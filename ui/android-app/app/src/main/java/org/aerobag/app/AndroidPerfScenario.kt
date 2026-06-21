package org.aerobag.app

import android.os.Handler
import android.os.Looper
import android.os.Debug
import android.os.SystemClock
import android.util.Log

internal const val AndroidPerfScenarioExtra = "aerobag_perf_scenario"
internal const val AndroidPerfScenarioMapSelectionFreeze = "map_selection_freeze"
internal const val AndroidPerfScenarioTerrainNexradMemoryStress = "terrain_nexrad_memory_stress"
internal const val AndroidPerfScenarioTag = "AerobagPerfScenario"

internal data class AndroidPerfScenario(
    val id: String,
    val mainThreadStallThresholdMs: Long = 750L,
    val slowSelectionThresholdMs: Long = 250L,
    val overlayFanout: Int = 64,
    val memorySampleIntervalMs: Long = 1_000L,
    val memoryStressDurationMs: Long = 30_000L,
    val memoryGrowthThresholdBytes: Long = 384L * 1024L * 1024L,
)

internal fun androidPerfScenarioFromIntentValue(value: String?): AndroidPerfScenario? =
    when (value) {
        AndroidPerfScenarioMapSelectionFreeze -> AndroidPerfScenario(id = value)
        AndroidPerfScenarioTerrainNexradMemoryStress -> AndroidPerfScenario(
            id = value,
            mainThreadStallThresholdMs = 1_000L,
            memoryStressDurationMs = 45_000L,
        )
        null, "" -> null
        else -> {
            Log.w(AndroidPerfScenarioTag, "unknown scenario=$value")
            null
        }
    }

internal class AndroidMainThreadStallWatchdog(
    private val scenario: AndroidPerfScenario,
    private val intervalMs: Long = 250L,
) {
    private val handler = Handler(Looper.getMainLooper())
    private var running = false
    private var expectedAtMs = 0L

    private val tick = object : Runnable {
        override fun run() {
            if (!running) {
                return
            }
            val nowMs = SystemClock.elapsedRealtime()
            val lagMs = nowMs - expectedAtMs
            if (lagMs > scenario.mainThreadStallThresholdMs) {
                Log.w(
                    AndroidPerfScenarioTag,
                    "threshold_violation scenario=${scenario.id} kind=main_thread_stall lagMs=$lagMs thresholdMs=${scenario.mainThreadStallThresholdMs}",
                )
            }
            expectedAtMs = nowMs + intervalMs
            handler.postDelayed(this, intervalMs)
        }
    }

    fun start() {
        if (running) {
            return
        }
        running = true
        expectedAtMs = SystemClock.elapsedRealtime() + intervalMs
        handler.postDelayed(tick, intervalMs)
    }

    fun stop() {
        running = false
        handler.removeCallbacks(tick)
    }
}

internal data class AndroidPerfCacheStats(
    val rasterDecodedEntries: Int,
    val rasterDecodedBytes: Long,
    val terrainEntries: Int,
    val terrainBytes: Long,
    val nexradEntries: Int,
    val nexradBytes: Long,
)

internal data class AndroidPerfMemorySample(
    val javaUsedBytes: Long,
    val javaTotalBytes: Long,
    val javaMaxBytes: Long,
    val nativeAllocatedBytes: Long,
    val nativeHeapBytes: Long,
    val nativeFreeBytes: Long,
    val totalPssKb: Int,
    val totalPrivateDirtyKb: Int,
) {
    val footprintBytes: Long
        get() = totalPssKb.toLong() * 1024L
}

internal fun androidPerfMemorySample(): AndroidPerfMemorySample {
    val runtime = Runtime.getRuntime()
    val memoryInfo = Debug.MemoryInfo()
    Debug.getMemoryInfo(memoryInfo)
    return AndroidPerfMemorySample(
        javaUsedBytes = runtime.totalMemory() - runtime.freeMemory(),
        javaTotalBytes = runtime.totalMemory(),
        javaMaxBytes = runtime.maxMemory(),
        nativeAllocatedBytes = Debug.getNativeHeapAllocatedSize(),
        nativeHeapBytes = Debug.getNativeHeapSize(),
        nativeFreeBytes = Debug.getNativeHeapFreeSize(),
        totalPssKb = memoryInfo.totalPss,
        totalPrivateDirtyKb = memoryInfo.totalPrivateDirty,
    )
}

internal fun logAndroidPerfMemorySample(
    scenario: AndroidPerfScenario,
    phase: String,
    cacheStats: AndroidPerfCacheStats,
): AndroidPerfMemorySample {
    val sample = androidPerfMemorySample()
    Log.i(
        AndroidPerfScenarioTag,
        "memory_sample scenario=${scenario.id} phase=$phase " +
            "javaUsedBytes=${sample.javaUsedBytes} javaTotalBytes=${sample.javaTotalBytes} javaMaxBytes=${sample.javaMaxBytes} " +
            "nativeAllocatedBytes=${sample.nativeAllocatedBytes} nativeHeapBytes=${sample.nativeHeapBytes} nativeFreeBytes=${sample.nativeFreeBytes} " +
            "totalPssKb=${sample.totalPssKb} totalPrivateDirtyKb=${sample.totalPrivateDirtyKb} " +
            "rasterDecodedEntries=${cacheStats.rasterDecodedEntries} rasterDecodedBytes=${cacheStats.rasterDecodedBytes} " +
            "terrainEntries=${cacheStats.terrainEntries} terrainBytes=${cacheStats.terrainBytes} " +
            "nexradEntries=${cacheStats.nexradEntries} nexradBytes=${cacheStats.nexradBytes}",
    )
    return sample
}
