// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app

import androidx.compose.runtime.Stable
import androidx.compose.runtime.State
import androidx.compose.runtime.mutableStateOf
import java.util.concurrent.atomic.AtomicReference
import java.util.concurrent.atomic.AtomicInteger
import org.aerobag.app.domain.CoreMapViewport
import org.aerobag.app.domain.FlightDataBannerModel
import org.aerobag.app.domain.MapFollowUiState
import org.aerobag.app.domain.NativeUiSession
import org.aerobag.app.domain.OwnshipUiState
import org.aerobag.app.domain.PlaybackUiState
import org.aerobag.app.domain.UiPlaybackPanelState
import org.aerobag.app.domain.UiSessionSnapshot
import org.aerobag.app.generated.UiSessionUpdateGroup

internal val HighRateSessionUpdateGroups = setOf(
    UiSessionUpdateGroup.Ownship,
    UiSessionUpdateGroup.Situation,
    UiSessionUpdateGroup.FlightData,
)

internal enum class SessionRenderScope {
    Shell,
    HighRate,
}

internal fun sessionRenderScopes(
    changedGroups: Set<UiSessionUpdateGroup>,
    fullSnapshot: Boolean,
): Set<SessionRenderScope> = buildSet {
    if (fullSnapshot || changedGroups.any(HighRateSessionUpdateGroups::contains)) {
        add(SessionRenderScope.HighRate)
    }
    if (fullSnapshot || changedGroups.any { it !in HighRateSessionUpdateGroups }) {
        add(SessionRenderScope.Shell)
    }
}

internal data class HighRateSessionProjection(
    val sessionRevision: Long,
    val ownship: OwnshipUiState,
    val flightDataBanner: FlightDataBannerModel,
    val playbackUiState: PlaybackUiState,
    val playbackPanelState: UiPlaybackPanelState,
    val mapFollowUiState: MapFollowUiState,
    val mapFollowTargetViewport: CoreMapViewport?,
) {
    companion object {
        fun from(snapshot: UiSessionSnapshot) = HighRateSessionProjection(
            sessionRevision = snapshot.sessionRevision,
            ownship = snapshot.appUiState.ownship,
            flightDataBanner = snapshot.appUiState.flightDataBanner,
            playbackUiState = snapshot.playbackUiState,
            playbackPanelState = snapshot.playbackPanelState,
            mapFollowUiState = snapshot.mapFollowUiState,
            mapFollowTargetViewport = snapshot.mapFollowTargetViewport,
        )
    }
}

internal fun UiSessionSnapshot.withHighRateProjection(
    projection: HighRateSessionProjection,
): UiSessionSnapshot = copy(
    sessionRevision = maxOf(sessionRevision, projection.sessionRevision),
    appUiState = appUiState.copy(
        ownship = projection.ownship,
        flightDataBanner = projection.flightDataBanner,
    ),
    playbackUiState = projection.playbackUiState,
    playbackPanelState = projection.playbackPanelState,
    mapFollowUiState = projection.mapFollowUiState,
    mapFollowTargetViewport = projection.mapFollowTargetViewport,
)

@Stable
internal class SessionRenderModel(initialSnapshot: UiSessionSnapshot) {
    private val latestSnapshot = AtomicReference(initialSnapshot)
    private val shellSnapshot = mutableStateOf(initialSnapshot)
    private val highRateProjection = mutableStateOf(HighRateSessionProjection.from(initialSnapshot))

    val shellSnapshotState: State<UiSessionSnapshot> = shellSnapshot
    val highRateProjectionState: State<HighRateSessionProjection> = highRateProjection
    val currentRevision: Long
        get() = latestSnapshot.get().sessionRevision

    fun observe(snapshot: UiSessionSnapshot) {
        latestSnapshot.updateAndGet { current ->
            if (snapshot.sessionRevision >= current.sessionRevision) snapshot else current
        }
    }

    fun publish(publication: NativeUiSession.SnapshotPublication) {
        val snapshot = publication.snapshot
        if (snapshot.sessionRevision < latestSnapshot.get().sessionRevision) return
        latestSnapshot.set(snapshot)
        val scopes = sessionRenderScopes(publication.changedGroups, publication.fullSnapshot)
        if (SessionRenderScope.HighRate in scopes) {
            highRateProjection.value = HighRateSessionProjection.from(snapshot)
        }
        if (SessionRenderScope.Shell in scopes) {
            shellSnapshot.value = snapshot
        }
    }

    fun publishUnannouncedSnapshot(snapshot: UiSessionSnapshot): Boolean {
        val current = latestSnapshot.get()
        if (snapshot.sessionRevision < current.sessionRevision) return false
        if (snapshot.sessionRevision == current.sessionRevision) return true
        publish(
            NativeUiSession.SnapshotPublication(
                snapshot = snapshot,
                changedGroups = UiSessionUpdateGroup.entries.toSet(),
                fullSnapshot = true,
            ),
        )
        return true
    }
}

internal data class SessionRenderCounts(
    val root: Int,
    val highRateEffects: Int,
    val map: Int,
    val charts: Int,
)

internal class SessionRenderDiagnostics {
    private val root = AtomicInteger()
    private val highRateEffects = AtomicInteger()
    private val map = AtomicInteger()
    private val charts = AtomicInteger()

    fun recordRoot() = root.incrementAndGet()
    fun recordHighRateEffects() = highRateEffects.incrementAndGet()
    fun recordMap() = map.incrementAndGet()
    fun recordCharts() = charts.incrementAndGet()

    fun snapshot() = SessionRenderCounts(
        root = root.get(),
        highRateEffects = highRateEffects.get(),
        map = map.get(),
        charts = charts.get(),
    )
}
