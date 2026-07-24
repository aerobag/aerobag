// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app

import org.aerobag.app.domain.FlightPlanEntryPreview

internal data class RouteEntryPreviewUiState(
    val preview: FlightPlanEntryPreview,
    val loading: Boolean,
    val error: String?,
)

internal data class RouteEntryPreviewRequest(
    val id: Int,
    val shouldFetch: Boolean,
    val state: RouteEntryPreviewUiState,
)

internal class RouteEntryPreviewController {
    private var activeRequestId = 0

    fun begin(
        input: String,
        current: RouteEntryPreviewUiState,
    ): RouteEntryPreviewRequest {
        val requestId = activeRequestId + 1
        activeRequestId = requestId
        val trimmed = input.trim()
        if (trimmed.isEmpty()) {
            return RouteEntryPreviewRequest(
                id = requestId,
                shouldFetch = false,
                state = current.copy(
                    preview = emptyFlightPlanEntryPreview(),
                    loading = false,
                    error = null,
                ),
            )
        }
        return RouteEntryPreviewRequest(
            id = requestId,
            shouldFetch = true,
            state = current.copy(loading = true),
        )
    }

    fun complete(
        requestId: Int,
        preview: FlightPlanEntryPreview,
        current: RouteEntryPreviewUiState,
    ): RouteEntryPreviewUiState =
        publishIfCurrent(requestId, current) {
            it.copy(
                preview = preview,
                loading = false,
                error = null,
            )
        }

    fun fail(
        requestId: Int,
        error: Throwable,
        current: RouteEntryPreviewUiState,
    ): RouteEntryPreviewUiState =
        publishIfCurrent(requestId, current) {
            it.copy(
                preview = emptyFlightPlanEntryPreview(),
                loading = false,
                error = error.message ?: error.toString(),
            )
        }

    fun finish(
        requestId: Int,
        current: RouteEntryPreviewUiState,
    ): RouteEntryPreviewUiState =
        publishIfCurrent(requestId, current) {
            it.copy(loading = false)
        }

    private inline fun publishIfCurrent(
        requestId: Int,
        current: RouteEntryPreviewUiState,
        publish: (RouteEntryPreviewUiState) -> RouteEntryPreviewUiState,
    ): RouteEntryPreviewUiState =
        if (requestId == activeRequestId) {
            publish(current)
        } else {
            current
        }
}
