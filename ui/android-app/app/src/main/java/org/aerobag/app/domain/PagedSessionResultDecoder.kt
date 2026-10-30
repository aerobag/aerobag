// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app.domain

import kotlinx.serialization.KSerializer
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import org.aerobag.app.generated.UiInvalidation

@Serializable
internal data class WirePagedSessionOutcome<T>(
    val state: String,
    val result: T? = null,
    val resources: List<JsonObject> = emptyList(),
    val invalidations: List<UiInvalidation> = emptyList(),
)

internal class PagedSessionResultDecoder<T>(
    private val json: Json,
    resultSerializer: KSerializer<T>,
    private val missingResult: () -> T = { error("complete paged operation has no result") },
) {
    private val serializer = WirePagedSessionOutcome.serializer(resultSerializer)

    fun decode(encoded: String): WirePagedSessionOutcome<T> =
        json.decodeFromString(serializer, encoded)

    fun result(outcome: WirePagedSessionOutcome<T>): T = outcome.result ?: missingResult()
}
