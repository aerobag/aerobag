package org.aerobag.app

import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharedFlow
import kotlinx.coroutines.flow.StateFlow
import org.aerobag.app.domain.OwnshipSourceKind
import org.aerobag.app.domain.OwnshipSourceRegistration
import org.aerobag.app.domain.OwnshipSourceStatusUpdate
import org.aerobag.app.domain.SourceConnectionState
import org.aerobag.app.domain.SituationSample

object AndroidGpsSource {
    const val SourceId = "android-gps"

    private val mutableStatus = MutableStateFlow(searchingStatus("Waiting for permission"))
    private val mutableSamples = MutableSharedFlow<SituationSample>(
        extraBufferCapacity = 32,
    )
    private val mutableSourceSelectionRequests = MutableSharedFlow<String>(
        extraBufferCapacity = 8,
    )

    val status: StateFlow<OwnshipSourceStatusUpdate> = mutableStatus
    val samples: SharedFlow<SituationSample> = mutableSamples
    val sourceSelectionRequests: SharedFlow<String> = mutableSourceSelectionRequests

    fun registration() =
        OwnshipSourceRegistration(
            sourceId = SourceId,
            sourceKind = OwnshipSourceKind.DeviceGps,
            displayName = "Android GPS",
            selectable = true,
            autoEligible = true,
        )

    fun publishStatus(update: OwnshipSourceStatusUpdate) {
        mutableStatus.value = update
    }

    fun publishSample(sample: SituationSample) {
        mutableSamples.tryEmit(sample)
    }

    fun requestSourceSelection(sourceId: String) {
        mutableSourceSelectionRequests.tryEmit(sourceId)
    }

    fun searchingStatus(label: String = "Searching") =
        OwnshipSourceStatusUpdate(
            sourceId = SourceId,
            connectionState = SourceConnectionState.Searching,
            enabled = true,
            statusLabel = label,
        )

    fun connectedStatus(label: String = "GPS fix") =
        OwnshipSourceStatusUpdate(
            sourceId = SourceId,
            connectionState = SourceConnectionState.Connected,
            enabled = true,
            statusLabel = label,
        )

    fun pausedStatus(label: String = "Paused") =
        OwnshipSourceStatusUpdate(
            sourceId = SourceId,
            connectionState = SourceConnectionState.Unavailable,
            enabled = true,
            statusLabel = label,
        )

    fun unavailableStatus(label: String) =
        OwnshipSourceStatusUpdate(
            sourceId = SourceId,
            connectionState = SourceConnectionState.Unavailable,
            enabled = false,
            statusLabel = label,
        )

    fun failedStatus(label: String) =
        OwnshipSourceStatusUpdate(
            sourceId = SourceId,
            connectionState = SourceConnectionState.Failed,
            enabled = false,
            statusLabel = label,
        )
}
