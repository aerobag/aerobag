// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app.domain

import java.io.File
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertThrows
import org.junit.Test

class SessionUpdateAccumulatorTest {
    private val json = Json { ignoreUnknownKeys = true }
    private val conformance = json.parseToJsonElement(
        sourceFile(
            "ui/core-rust/crates/app-ui-contracts/tests/goldens/" +
                "session-update-conformance.json",
        ).readText(),
    ).jsonObject

    @Test
    fun matchesSharedSessionUpdateConformanceSequence() {
        val accumulator = accumulator()
        for (step in conformance.getValue("steps").jsonArray) {
            val item = step.jsonObject
            assertEquals(
                item.getValue("name").jsonPrimitive.content,
                disposition(item.getValue("disposition").jsonPrimitive.content),
                accumulator.apply(item.getValue("update")),
            )
            assertEquals(item.getValue("expected_snapshot").jsonObject, accumulator.snapshot)
        }
    }

    @Test
    fun rejectsEveryInvalidSharedConformanceUpdate() {
        for (invalid in conformance.getValue("invalid_updates").jsonArray) {
            val item = invalid.jsonObject
            assertThrows(
                item.getValue("name").jsonPrimitive.content,
                SessionUpdateContractException::class.java,
            ) {
                accumulator().apply(item.getValue("update"))
            }
        }
    }

    @Test
    fun transitionalSnapshotMustMatchAppliedPatch() {
        val accumulator = accumulator()
        val initial = conformance.getValue("initial_snapshot").jsonObject
        val result = JsonObject(
            initial + mapOf(
                "session_revision" to json.parseToJsonElement("8"),
                "map_layer_state" to json.parseToJsonElement("{\"nexrad\":true}"),
                "session_update" to json.parseToJsonElement(
                    """{
                        "ui_contract_version":1,
                        "session_revision":8
                    }""".trimIndent(),
                ),
            ),
        )
        assertThrows(SessionUpdateProjectionMismatchException::class.java) {
            accumulator.applyTransitionalMutationSnapshot(result)
        }
    }

    private fun accumulator() = SessionUpdateAccumulator(
        conformance.getValue("initial_snapshot").jsonObject,
        conformance.getValue("expected_contract_version").jsonPrimitive.content.toInt(),
        json,
    )

    private fun disposition(value: String): SessionUpdateDisposition = when (value) {
        "applied" -> SessionUpdateDisposition.Applied
        "stale" -> SessionUpdateDisposition.Stale
        "resync_required" -> SessionUpdateDisposition.ResyncRequired
        else -> error("unknown disposition $value")
    }

    private fun sourceFile(path: String): File {
        val start = File(".").canonicalFile
        return generateSequence(start) { it.parentFile }
            .map { File(it, path) }
            .firstOrNull { it.isFile }
            ?: error("could not locate fixture $path from $start")
    }
}
