package org.aerobag.app

import android.os.Handler
import android.os.Looper
import android.os.SystemClock
import android.util.Log

internal const val AndroidPerfScenarioExtra = "aerobag_perf_scenario"
internal const val AndroidPerfScenarioMapSelectionFreeze = "map_selection_freeze"
internal const val AndroidPerfScenarioTag = "AerobagPerfScenario"

internal data class AndroidPerfScenario(
    val id: String,
    val mainThreadStallThresholdMs: Long = 750L,
    val slowSelectionThresholdMs: Long = 250L,
    val overlayFanout: Int = 64,
)

internal fun androidPerfScenarioFromIntentValue(value: String?): AndroidPerfScenario? =
    when (value) {
        AndroidPerfScenarioMapSelectionFreeze -> AndroidPerfScenario(id = value)
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
