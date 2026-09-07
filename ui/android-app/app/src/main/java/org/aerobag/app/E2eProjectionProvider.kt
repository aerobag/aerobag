// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app

import android.content.ContentProvider
import android.content.ContentValues
import android.database.Cursor
import android.database.MatrixCursor
import android.net.Uri
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.atomic.AtomicLong

internal data class E2eProjectionSnapshot(
    val state: String,
    val bounds: String?,
    val revision: Long,
)

internal object E2eProjectionRegistry {
    private val revision = AtomicLong()
    private val entries =
        ConcurrentHashMap<String, ConcurrentHashMap<Any, E2eProjectionSnapshot>>()

    fun publish(resourceId: String, state: String, owner: Any, bounds: String? = null) {
        val owners = entries.computeIfAbsent(resourceId) { ConcurrentHashMap() }
        owners.compute(owner) { _, previous ->
            if (previous?.state == state && previous.bounds == bounds
            ) {
                previous
            } else {
                E2eProjectionSnapshot(state, bounds, revision.incrementAndGet())
            }
        }
    }

    fun remove(resourceId: String, owner: Any) {
        entries.computeIfPresent(resourceId) { _, owners ->
            owners.remove(owner)
            owners.takeUnless { it.isEmpty() }
        }
    }

    fun read(resourceId: String): E2eProjectionSnapshot? =
        entries[resourceId]?.values?.maxByOrNull(E2eProjectionSnapshot::revision)

    fun readPrefix(resourceIdPrefix: String): List<Pair<String, E2eProjectionSnapshot>> =
        entries.entries
            .asSequence()
            .filter { (resourceId, _) -> resourceId.startsWith(resourceIdPrefix) }
            .mapNotNull { (resourceId, owners) ->
                owners.values.maxByOrNull(E2eProjectionSnapshot::revision)?.let { resourceId to it }
            }
            .sortedBy { (resourceId, _) -> resourceId }
            .toList()

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
        if (!BuildConfig.AEROBAG_E2E_ENABLED || uri.path != "/projection") return null
        val resourceId = uri.getQueryParameter("resource_id")
        val resourceIdPrefix = uri.getQueryParameter("resource_id_prefix")
        if ((resourceId == null) == (resourceIdPrefix == null)) return null
        if (resourceIdPrefix != null) {
            val snapshots = E2eProjectionRegistry.readPrefix(resourceIdPrefix)
            if (snapshots.isEmpty()) return null
            return MatrixCursor(Columns).apply {
                snapshots.forEach { (id, snapshot) ->
                    addRow(arrayOf(id, snapshot.state, snapshot.bounds, snapshot.revision, 1))
                }
            }
        }
        checkNotNull(resourceId)
        val snapshot = E2eProjectionRegistry.read(resourceId)
        val resourceName = resourceId.removePrefix("org.aerobag.app:id/")
        val viewId = context?.resources?.getIdentifier(resourceName, "id", context?.packageName) ?: 0
        val knownProjection = viewId != 0 && resourceName.startsWith("e2e_")
        if (!knownProjection && snapshot == null) {
            return null
        }
        return MatrixCursor(Columns).apply {
            addRow(
                arrayOf(
                    resourceId,
                    snapshot?.state,
                    snapshot?.bounds,
                    snapshot?.revision ?: 0L,
                    if (snapshot == null) 0 else 1,
                ),
            )
        }
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

    private companion object {
        val Columns = arrayOf("resource_id", "state", "bounds", "revision", "present")
    }
}
