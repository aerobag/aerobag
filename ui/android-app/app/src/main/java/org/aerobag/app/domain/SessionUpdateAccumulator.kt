// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app.domain

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.longOrNull
import org.aerobag.app.generated.UiSessionUpdate
import org.aerobag.app.generated.UiSessionUpdateGroup
import org.aerobag.app.generated.projectionPatches

internal enum class SessionUpdateDisposition {
    Applied,
    Stale,
    ResyncRequired,
}

internal data class SessionUpdateApplication(
    val disposition: SessionUpdateDisposition,
    val changedGroups: Set<UiSessionUpdateGroup> = emptySet(),
    val changedPaths: Set<List<String>> = emptySet(),
    val installedFullSnapshot: Boolean = false,
)

internal class SessionUpdateContractException(message: String) : IllegalArgumentException(message)

internal class SessionUpdateAccumulator(
    initialSnapshot: JsonObject,
    private val expectedContractVersion: Int,
    private val json: Json,
) {
    private val groupVersions = mutableMapOf<UiSessionUpdateGroup, Long>()

    var snapshot: JsonObject = sanitizeFullSnapshot(initialSnapshot)
        private set

    fun replaceFullSnapshot(value: JsonElement): JsonObject {
        val nextSnapshot = sanitizeFullSnapshot(value)
        if (nextSnapshot.wireLong("session_revision") < snapshot.wireLong("session_revision")) {
            return snapshot
        }
        snapshot = nextSnapshot
        groupVersions.clear()
        return snapshot
    }

    fun apply(value: JsonElement): SessionUpdateDisposition = applyDetailed(value).disposition

    fun applyDetailed(value: JsonElement): SessionUpdateApplication =
        applyDetailed(parseSessionUpdate(value))

    fun applyOrResync(
        value: JsonElement,
        loadFullSnapshot: () -> JsonElement,
    ): SessionUpdateDisposition = applyOrResyncDetailed(value, loadFullSnapshot).disposition

    fun applyOrResyncDetailed(
        value: JsonElement,
        loadFullSnapshot: () -> JsonElement,
    ): SessionUpdateApplication {
        val application = applyDetailed(value)
        if (application.disposition == SessionUpdateDisposition.ResyncRequired) {
            replaceFullSnapshot(loadFullSnapshot())
            return application.copy(installedFullSnapshot = true)
        }
        return application
    }

    private fun applyDetailed(update: UiSessionUpdate): SessionUpdateApplication {
        if (update.uiContractVersion != expectedContractVersion) {
            throw SessionUpdateContractException(
                "UI wire contract ${update.uiContractVersion} is unsupported; " +
                    "client requires $expectedContractVersion",
            )
        }
        val currentRevision = snapshot.wireLong("session_revision")
        if (update.sessionRevision <= currentRevision) {
            return SessionUpdateApplication(SessionUpdateDisposition.Stale)
        }
        if (update.sessionRevision != currentRevision + 1) {
            return SessionUpdateApplication(SessionUpdateDisposition.ResyncRequired)
        }

        var nextSnapshot: JsonElement = snapshot
        val assignedPaths = mutableListOf<List<String>>()
        val nextVersions = mutableListOf<Pair<UiSessionUpdateGroup, Long>>()
        val changedGroups = mutableSetOf<UiSessionUpdateGroup>()
        for ((group, patch) in update.projectionPatches()) {
            val previousVersion = groupVersions[group]
            if (previousVersion != null && patch.version <= previousVersion) {
                throw SessionUpdateContractException(
                    "session update ${group.wireName} version ${patch.version} " +
                        "does not advance $previousVersion",
                )
            }
            for ((index, assignment) in patch.assignments.withIndex()) {
                val envelopeField = assignment.path.first()
                if (envelopeField in ENVELOPE_FIELDS) {
                    throw SessionUpdateContractException(
                        "session update ${group.wireName} cannot replace envelope field $envelopeField",
                    )
                }
                val overlap = assignedPaths.firstOrNull { pathsOverlap(it, assignment.path) }
                if (overlap != null) {
                    throw SessionUpdateContractException(
                        "session update path ${assignment.path.joinToString("/")} " +
                            "overlaps ${overlap.joinToString("/")}",
                    )
                }
                nextSnapshot = replaceAtPath(
                    nextSnapshot,
                    assignment.path,
                    0,
                    assignment.value,
                    "session update ${group.wireName} assignment $index",
                )
                assignedPaths += assignment.path
            }
            nextVersions += group to patch.version
            changedGroups += group
        }

        snapshot = JsonObject(
            (nextSnapshot as? JsonObject
                ?: throw SessionUpdateContractException("session update result must be an object")) + mapOf(
                "ui_contract_version" to JsonPrimitive(update.uiContractVersion),
                "session_revision" to JsonPrimitive(update.sessionRevision),
            ),
        )
        nextVersions.forEach { (group, version) -> groupVersions[group] = version }
        return SessionUpdateApplication(
            disposition = SessionUpdateDisposition.Applied,
            changedGroups = changedGroups,
            changedPaths = assignedPaths.toSet(),
        )
    }

    private fun parseSessionUpdate(value: JsonElement): UiSessionUpdate {
        val update = value as? JsonObject
            ?: throw SessionUpdateContractException("session update must be a JSON object")
        rejectUnknownKeys(
            update,
            setOf("ui_contract_version", "session_revision") +
                UiSessionUpdateGroup.entries.map(UiSessionUpdateGroup::wireName),
            "session update",
        )
        update.wireLong("ui_contract_version")
        update.wireLong("session_revision")
        for (group in UiSessionUpdateGroup.entries) {
            val patch = update[group.wireName] ?: continue
            if (patch is JsonNull) continue
            val patchObject = patch as? JsonObject
                ?: throw SessionUpdateContractException(
                    "session update ${group.wireName} patch must be a JSON object",
                )
            rejectUnknownKeys(
                patchObject,
                setOf("version", "assignments"),
                "session update ${group.wireName} patch",
            )
            patchObject.wireLong("version")
            val assignments = patchObject["assignments"] as? JsonArray
            if (assignments == null) {
                throw SessionUpdateContractException(
                    "session update ${group.wireName} assignments must be an array",
                )
            }
            assignments.forEachIndexed { index, assignmentValue ->
                val label = "session update ${group.wireName} assignment $index"
                val assignment = assignmentValue as? JsonObject
                    ?: throw SessionUpdateContractException("$label must be an object")
                rejectUnknownKeys(assignment, setOf("path", "value"), label)
                val path = assignment["path"] as? JsonArray
                    ?: throw SessionUpdateContractException("$label path must be a nonempty array")
                if (path.isEmpty()) {
                    throw SessionUpdateContractException("$label path must be a nonempty array")
                }
                if (path.any { segment ->
                        segment !is JsonPrimitive || !segment.isString || segment.content.isEmpty()
                    }
                ) {
                    throw SessionUpdateContractException(
                        "$label path segments must be nonempty strings",
                    )
                }
                if ("value" !in assignment) {
                    throw SessionUpdateContractException("$label must contain value")
                }
            }
        }
        return json.decodeFromJsonElement(UiSessionUpdate.serializer(), update)
    }

    private fun pathsOverlap(left: List<String>, right: List<String>): Boolean =
        (0 until minOf(left.size, right.size)).all { index -> left[index] == right[index] }

    private fun replaceAtPath(
        current: JsonElement,
        path: List<String>,
        offset: Int,
        value: JsonElement,
        label: String,
    ): JsonElement {
        if (offset == path.size) return value
        val segment = path[offset]
        return when (current) {
            is JsonObject -> {
                val child = current[segment]
                    ?: throw SessionUpdateContractException(
                        "$label path does not exist at $segment",
                    )
                JsonObject(current + (segment to replaceAtPath(child, path, offset + 1, value, label)))
            }
            is JsonArray -> {
                val index = segment.toIntOrNull()
                    ?.takeIf { it >= 0 && it.toString() == segment }
                    ?: throw SessionUpdateContractException(
                        "$label path segment $segment is not an array index",
                    )
                if (index !in current.indices) {
                    throw SessionUpdateContractException("$label array index $index is out of range")
                }
                JsonArray(current.mapIndexed { childIndex, child ->
                    if (childIndex == index) {
                        replaceAtPath(child, path, offset + 1, value, label)
                    } else {
                        child
                    }
                })
            }
            else -> throw SessionUpdateContractException("$label path parent must be an object or array")
        }
    }

    private fun sanitizeFullSnapshot(value: JsonElement): JsonObject {
        val raw = value as? JsonObject
            ?: throw SessionUpdateContractException("session snapshot must be a JSON object")
        if ("session_update" in raw) {
            throw SessionUpdateContractException(
                "full session snapshot must not contain session_update",
            )
        }
        val snapshot = raw
        val contractVersion = snapshot.wireLong("ui_contract_version")
        snapshot.wireLong("session_revision")
        if (contractVersion != expectedContractVersion.toLong()) {
            throw SessionUpdateContractException(
                "UI wire contract $contractVersion is unsupported; client requires $expectedContractVersion",
            )
        }
        return snapshot
    }

    private fun JsonObject.wireLong(field: String): Long {
        val primitive = get(field) as? JsonPrimitive
        val value = primitive
            ?.takeUnless { it.isString }
            ?.longOrNull
            ?: throw SessionUpdateContractException("$field must be a non-negative integer")
        if (value < 0) throw SessionUpdateContractException("$field must be a non-negative integer")
        return value
    }

    private fun rejectUnknownKeys(value: JsonObject, allowed: Set<String>, label: String) {
        val unknown = value.keys - allowed
        if (unknown.isNotEmpty()) {
            throw SessionUpdateContractException("$label has unknown fields: ${unknown.joinToString()}")
        }
    }

    private companion object {
        val ENVELOPE_FIELDS = setOf("ui_contract_version", "session_revision", "session_update")
    }
}
