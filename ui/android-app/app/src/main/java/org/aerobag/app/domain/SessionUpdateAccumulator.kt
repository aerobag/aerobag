// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app.domain

import kotlinx.serialization.json.Json
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

internal class SessionUpdateContractException(message: String) : IllegalArgumentException(message)

internal class SessionUpdateProjectionMismatchException(message: String) : IllegalStateException(message)

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

    fun apply(value: JsonElement): SessionUpdateDisposition = apply(parseSessionUpdate(value))

    fun applyTransitionalMutationSnapshot(value: JsonElement): SessionUpdateDisposition {
        val raw = value as? JsonObject
            ?: throw SessionUpdateContractException("session mutation result must be a JSON object")
        val fullSnapshot = sanitizeFullSnapshot(raw)
        val updateElement = raw["session_update"]
            ?: throw SessionUpdateContractException("session mutation result is missing session_update")
        val update = parseSessionUpdate(updateElement)
        if (fullSnapshot.wireLong("session_revision") != update.sessionRevision) {
            throw SessionUpdateContractException("session mutation snapshot and update revisions differ")
        }
        return when (val disposition = apply(update)) {
            SessionUpdateDisposition.ResyncRequired -> {
                replaceFullSnapshot(fullSnapshot)
                disposition
            }
            SessionUpdateDisposition.Applied -> {
                if (snapshot != fullSnapshot) {
                    throw SessionUpdateProjectionMismatchException(
                        "session update revision ${update.sessionRevision} does not reproduce core's full snapshot",
                    )
                }
                disposition
            }
            SessionUpdateDisposition.Stale -> disposition
        }
    }

    private fun apply(update: UiSessionUpdate): SessionUpdateDisposition {
        if (update.uiContractVersion != expectedContractVersion) {
            throw SessionUpdateContractException(
                "UI wire contract ${update.uiContractVersion} is unsupported; " +
                    "client requires $expectedContractVersion",
            )
        }
        val currentRevision = snapshot.wireLong("session_revision")
        if (update.sessionRevision <= currentRevision) return SessionUpdateDisposition.Stale
        if (update.sessionRevision != currentRevision + 1) return SessionUpdateDisposition.ResyncRequired

        val nextFields = linkedMapOf<String, JsonElement>()
        val nextVersions = mutableListOf<Pair<UiSessionUpdateGroup, Long>>()
        for ((group, patch) in update.projectionPatches()) {
            val fields = patch.fields
            val previousVersion = groupVersions[group]
            if (previousVersion != null && patch.version <= previousVersion) {
                throw SessionUpdateContractException(
                    "session update ${group.wireName} version ${patch.version} " +
                        "does not advance $previousVersion",
                )
            }
            for ((field, fieldValue) in fields) {
                if (field in ENVELOPE_FIELDS) {
                    throw SessionUpdateContractException(
                        "session update ${group.wireName} cannot replace envelope field $field",
                    )
                }
                if (nextFields.put(field, fieldValue) != null) {
                    throw SessionUpdateContractException(
                        "session update field $field appears in multiple groups",
                    )
                }
            }
            nextVersions += group to patch.version
        }

        snapshot = JsonObject(
            snapshot + nextFields + mapOf(
                "ui_contract_version" to JsonPrimitive(update.uiContractVersion),
                "session_revision" to JsonPrimitive(update.sessionRevision),
            ),
        )
        nextVersions.forEach { (group, version) -> groupVersions[group] = version }
        return SessionUpdateDisposition.Applied
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
                setOf("version", "fields"),
                "session update ${group.wireName} patch",
            )
            patchObject.wireLong("version")
            if (patchObject["fields"] !is JsonObject) {
                throw SessionUpdateContractException(
                    "session update ${group.wireName} fields must be a JSON object",
                )
            }
        }
        return json.decodeFromJsonElement(UiSessionUpdate.serializer(), update)
    }

    private fun sanitizeFullSnapshot(value: JsonElement): JsonObject {
        val raw = value as? JsonObject
            ?: throw SessionUpdateContractException("session snapshot must be a JSON object")
        val snapshot = JsonObject(raw - "session_update")
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
