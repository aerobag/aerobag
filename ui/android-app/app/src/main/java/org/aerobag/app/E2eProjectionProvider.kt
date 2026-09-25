// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app

import android.content.ContentProvider
import android.content.ContentValues
import android.database.Cursor
import android.database.MatrixCursor
import android.net.Uri
import android.os.Bundle
import java.util.UUID

internal data class E2eProjectionSnapshot(
    val state: String,
    val bounds: String?,
    val revision: Long,
)

internal object E2eProjectionRegistry {
    val incarnation: String = UUID.randomUUID().toString()
    private var revision = 0L
    private val entries = mutableMapOf<String, MutableMap<Any, E2eProjectionSnapshot>>()

    @Synchronized
    fun publish(resourceId: String, state: String, owner: Any, bounds: String? = null) {
        val owners = entries.getOrPut(resourceId) { mutableMapOf() }
        val previous = owners[owner]
        if (previous?.state != state || previous.bounds != bounds) {
            owners[owner] = E2eProjectionSnapshot(state, bounds, ++revision)
        }
    }

    @Synchronized
    fun remove(resourceId: String, owner: Any) {
        val owners = entries[resourceId] ?: return
        if (owners.remove(owner) != null) revision++
        if (owners.isEmpty()) entries.remove(resourceId)
    }

    @Synchronized
    fun read(resourceId: String): E2eProjectionSnapshot? =
        entries[resourceId]?.let { unique(resourceId, it) }

    @Synchronized
    fun readPrefix(resourceIdPrefix: String): List<Pair<String, E2eProjectionSnapshot>> =
        entries.entries
            .asSequence()
            .filter { (resourceId, _) -> resourceId.startsWith(resourceIdPrefix) }
            .mapNotNull { (resourceId, owners) ->
                unique(resourceId, owners)?.let { resourceId to it }
            }
            .sortedBy { (resourceId, _) -> resourceId }
            .toList()

    private fun unique(id: String, owners: Map<Any, E2eProjectionSnapshot>): E2eProjectionSnapshot? {
        check(owners.size <= 1) { "Ambiguous observation identity $id (${owners.size} mounted owners)" }
        return owners.values.singleOrNull()
    }

    /** A window publishes one completed layout, never half of a composition. */
    @Synchronized
    fun replaceFrame(previous: Map<Any, String>, next: Map<Any, Pair<String, E2eProjectionSnapshot>>) {
        previous.forEach { (owner, id) -> if (next[owner]?.first != id) remove(id, owner) }
        next.forEach { (owner, value) -> publish(value.first, value.second.state, owner, value.second.bounds) }
    }

    @Synchronized
    fun query(id: String?, prefix: String?): Cursor {
        require((id == null) != (prefix == null)) { "Specify one observation selector" }
        val selector = id ?: prefix!!
        require((prefix != null && selector.isEmpty()) || selector.startsWith("parity:") ||
            selector.startsWith("flight-data-cell:") || selector.startsWith("org.aerobag.app:id/e2e_")) {
            "Invalid observation namespace: $selector"
        }
        val snapshots = if (prefix != null) readPrefix(prefix) else listOfNotNull(read(id!!)?.let { id to it })
        return MatrixCursor(arrayOf("resource_id", "state", "bounds", "revision", "present")).apply {
            snapshots.forEach { (key, snapshot) ->
                addRow(arrayOf(key, snapshot.state, snapshot.bounds, snapshot.revision, 1))
            }
            // An empty cursor is a successful absence. A null cursor is a broken source.
            extras = Bundle().apply {
                putInt("schema", 1)
                putString("incarnation", incarnation)
                putLong("revision", revision)
            }
        }
    }
}

/** E2E-only state channel that cannot block behind Compose accessibility traversal. */
class E2eProjectionProvider : ContentProvider() {
    override fun onCreate(): Boolean = BuildConfig.AEROBAG_E2E_ENABLED

    override fun query(
        uri: Uri,
        projection: Array<out String>?,
        selection: String?,
        selectionArgs: Array<out String>?,
        sortOrder: String?,
    ): Cursor? {
        check(BuildConfig.AEROBAG_E2E_ENABLED) { "Observation provider disabled" }
        require(uri.path == "/projection") { "Invalid observation endpoint" }
        val resourceId = uri.getQueryParameter("resource_id")
        val resourceIdPrefix = uri.getQueryParameter("resource_id_prefix")
        return E2eProjectionRegistry.query(resourceId, resourceIdPrefix)
    }

    override fun getType(uri: Uri): String = "vnd.android.cursor.item/aerobag-e2e-projection"

    override fun insert(uri: Uri, values: ContentValues?): Uri? = null

    override fun delete(uri: Uri, selection: String?, selectionArgs: Array<out String>?): Int = 0

    override fun update(
        uri: Uri,
        values: ContentValues?,
        selection: String?,
        selectionArgs: Array<out String>?,
    ): Int = 0

}
