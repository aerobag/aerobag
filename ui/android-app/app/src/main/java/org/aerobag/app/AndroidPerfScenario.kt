// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app

import android.os.Handler
import android.os.Looper
import android.os.Debug
import android.os.SystemClock
import android.util.Log
import android.view.Choreographer
import kotlin.math.ceil

internal const val AndroidPerfScenarioExtra = "aerobag_perf_scenario"
internal const val AndroidPerfScenarioMapSelectionFreeze = "map_selection_freeze"
internal const val AndroidPerfScenarioTerrainNexradMemoryStress = "terrain_nexrad_memory_stress"
internal const val AndroidPerfScenarioSessionRenderInvalidation = "session_render_invalidation"
internal const val AndroidPerfScenarioTag = "AerobagPerfScenario"

internal data class AndroidPerfScenario(
    val id: String,
    val mainThreadStallThresholdMs: Long = 750L,
    val frameGapThresholdMs: Long = 250L,
    val slowSelectionThresholdMs: Long = 250L,
    val overlayFanout: Int = 64,
    val memorySampleIntervalMs: Long = 1_000L,
    val memoryStressDurationMs: Long = 30_000L,
    val memoryGrowthThresholdBytes: Long = 384L * 1024L * 1024L,
)

internal class AndroidFrameGapMonitor(
    private val scenario: AndroidPerfScenario,
    private val choreographer: Choreographer = Choreographer.getInstance(),
) {
    private val intervalsMs = mutableListOf<Long>()
    private var running = false
    private var previousFrameTimeNs = 0L

    private val callback = object : Choreographer.FrameCallback {
        override fun doFrame(frameTimeNanos: Long) {
            if (!running) return
            if (previousFrameTimeNs != 0L) {
                val gapMs = ((frameTimeNanos - previousFrameTimeNs).coerceAtLeast(0L) + 500_000L) /
                    1_000_000L
                intervalsMs += gapMs
                if (gapMs > scenario.frameGapThresholdMs) {
                    Log.w(
                        AndroidPerfScenarioTag,
                        "threshold_violation scenario=${scenario.id} kind=frame_gap " +
                            "gapMs=$gapMs thresholdMs=${scenario.frameGapThresholdMs}",
                    )
                }
            }
            previousFrameTimeNs = frameTimeNanos
            choreographer.postFrameCallback(this)
        }
    }

    fun start() {
        if (running) return
        running = true
        previousFrameTimeNs = 0L
        intervalsMs.clear()
        choreographer.postFrameCallback(callback)
    }

    fun stop() {
        if (!running) return
        running = false
        choreographer.removeFrameCallback(callback)
        val sorted = intervalsMs.sorted()
        val p95Index = if (sorted.isEmpty()) {
            0
        } else {
            (ceil(sorted.size * 0.95).toInt() - 1).coerceIn(sorted.indices)
        }
        Log.i(
            AndroidPerfScenarioTag,
            "frame_summary scenario=${scenario.id} frames=${sorted.size} " +
                "p95Ms=${sorted.getOrNull(p95Index) ?: 0L} maxMs=${sorted.lastOrNull() ?: 0L} " +
                "thresholdMs=${scenario.frameGapThresholdMs}",
        )
    }
}

internal fun androidPerfScenarioFromIntentValue(value: String?): AndroidPerfScenario? =
    when (value) {
        AndroidPerfScenarioMapSelectionFreeze -> AndroidPerfScenario(id = value)
        AndroidPerfScenarioSessionRenderInvalidation -> AndroidPerfScenario(id = value)
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
    val summaryJavaHeapKb: Int,
    val summaryNativeHeapKb: Int,
    val summaryGraphicsKb: Int,
    val summaryCodeKb: Int,
    val summaryStackKb: Int,
    val summaryPrivateOtherKb: Int,
    val summarySystemKb: Int,
) {
    val footprintBytes: Long
        get() = totalPssKb.toLong() * 1024L
}

private fun Debug.MemoryInfo.summaryKb(key: String): Int =
    getMemoryStat(key)?.toIntOrNull() ?: 0

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
        summaryJavaHeapKb = memoryInfo.summaryKb("summary.java-heap"),
        summaryNativeHeapKb = memoryInfo.summaryKb("summary.native-heap"),
        summaryGraphicsKb = memoryInfo.summaryKb("summary.graphics"),
        summaryCodeKb = memoryInfo.summaryKb("summary.code"),
        summaryStackKb = memoryInfo.summaryKb("summary.stack"),
        summaryPrivateOtherKb = memoryInfo.summaryKb("summary.private-other"),
        summarySystemKb = memoryInfo.summaryKb("summary.system"),
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
            "summaryJavaHeapKb=${sample.summaryJavaHeapKb} summaryNativeHeapKb=${sample.summaryNativeHeapKb} " +
            "summaryGraphicsKb=${sample.summaryGraphicsKb} summaryCodeKb=${sample.summaryCodeKb} " +
            "summaryStackKb=${sample.summaryStackKb} summaryPrivateOtherKb=${sample.summaryPrivateOtherKb} " +
            "summarySystemKb=${sample.summarySystemKb} " +
            "rasterDecodedEntries=${cacheStats.rasterDecodedEntries} rasterDecodedBytes=${cacheStats.rasterDecodedBytes} " +
            "terrainEntries=${cacheStats.terrainEntries} terrainBytes=${cacheStats.terrainBytes} " +
            "nexradEntries=${cacheStats.nexradEntries} nexradBytes=${cacheStats.nexradBytes}",
    )
    return sample
}
