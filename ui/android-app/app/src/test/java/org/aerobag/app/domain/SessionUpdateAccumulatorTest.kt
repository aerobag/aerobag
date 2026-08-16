// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app.domain

import java.io.File
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Test

class SessionUpdateAccumulatorTest {
    private val json = Json { ignoreUnknownKeys = true }
    private val conformance = json.parseToJsonElement(
        sourceFile(
            "ui/core-rust/crates/app-ui-contracts/tests/goldens/" +
                "session-update-conformance.json",
        ).readText(),
    ).jsonObject
    private val contractVersion =
        conformance.getValue("expected_contract_version").jsonPrimitive.content.toInt()

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
    fun revisionGapLeavesSnapshotUntouchedForExplicitResync() {
        val accumulator = accumulator()
        val initial = conformance.getValue("initial_snapshot").jsonObject
        val update = json.parseToJsonElement(
            """{
                "ui_contract_version":$contractVersion,
                "session_revision":9,
                "map":{"version":3,"assignments":[
                    {"path":["map_layer_state"],"value":{"nexrad":true}}
                ]}
            }""".trimIndent(),
        )
        assertEquals(SessionUpdateDisposition.ResyncRequired, accumulator.apply(update))
        assertEquals(initial, accumulator.snapshot)
    }

    @Test
    fun revisionGapLoadsAndInstallsExplicitFullSnapshot() {
        val accumulator = accumulator()
        val initial = conformance.getValue("initial_snapshot").jsonObject
        val update = json.parseToJsonElement(
            """{
                "ui_contract_version":$contractVersion,
                "session_revision":9,
                "map":{"version":3,"assignments":[
                    {"path":["map_layer_state"],"value":{"nexrad":true}}
                ]}
            }""".trimIndent(),
        )
        val fullSnapshot = kotlinx.serialization.json.JsonObject(
            initial + mapOf(
                "session_revision" to json.parseToJsonElement("9"),
                "map_layer_state" to json.parseToJsonElement("{\"nexrad\":true}"),
            ),
        )
        var loads = 0

        assertEquals(
            SessionUpdateDisposition.ResyncRequired,
            accumulator.applyOrResync(update) {
                loads += 1
                fullSnapshot
            },
        )
        assertEquals(1, loads)
        assertEquals(fullSnapshot, accumulator.snapshot)
    }

    @Test
    fun detailedApplicationReportsOnlyValidatedChangedGroupsAndPaths() {
        val accumulator = accumulator()
        val firstUpdate = conformance.getValue("steps").jsonArray.first().jsonObject.getValue("update")

        val application = accumulator.applyDetailed(firstUpdate)

        assertEquals(SessionUpdateDisposition.Applied, application.disposition)
        assertEquals(setOf(org.aerobag.app.generated.UiSessionUpdateGroup.Map), application.changedGroups)
        assertEquals(setOf(listOf("map_layer_state")), application.changedPaths)
        assertFalse(application.installedFullSnapshot)
    }

    @Test
    fun detailedRevisionGapMarksOnlyTheExplicitFullSnapshotRecovery() {
        val accumulator = accumulator()
        val update = json.parseToJsonElement(
            """{
                "ui_contract_version":$contractVersion,
                "session_revision":9,
                "map":{"version":3,"assignments":[
                    {"path":["map_layer_state"],"value":{"nexrad":true}}
                ]}
            }""".trimIndent(),
        )

        val application = accumulator.applyOrResyncDetailed(update) {
            kotlinx.serialization.json.JsonObject(
                conformance.getValue("initial_snapshot").jsonObject + mapOf(
                    "session_revision" to json.parseToJsonElement("9"),
                ),
            )
        }

        assertEquals(SessionUpdateDisposition.ResyncRequired, application.disposition)
        assertTrue(application.installedFullSnapshot)
        assertTrue(application.changedGroups.isEmpty())
        assertTrue(application.changedPaths.isEmpty())
    }

    private fun accumulator() = SessionUpdateAccumulator(
        conformance.getValue("initial_snapshot").jsonObject,
        contractVersion,
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
