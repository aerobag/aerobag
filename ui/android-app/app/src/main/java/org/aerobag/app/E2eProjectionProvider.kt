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
    const val TouchReceiptResourceId = "org.aerobag.app:id/e2e_touch_receipt"

    val KnownViewIds = setOf(
        R.id.e2e_ownship_state_projection,
        R.id.e2e_live_overlay_projection,
        R.id.e2e_nexrad_state_projection,
        R.id.e2e_playback_widget_projection,
        R.id.e2e_viewport_projection,
        R.id.e2e_map_selection_projection,
        R.id.e2e_map_follow_projection,
        R.id.e2e_plate_viewport_projection,
        R.id.e2e_data_status_projection,
        R.id.e2e_startup_state_projection,
        R.id.e2e_flight_plan_rows_projection,
        R.id.e2e_flight_plan_state_projection,
        R.id.e2e_flight_plan_overlay_projection,
        R.id.e2e_map_family_projection,
        R.id.e2e_raster_state_projection,
        R.id.e2e_vector_state_projection,
        R.id.e2e_flight_plan_route_overlay_projection,
        R.id.e2e_flight_plan_route_entry_projection,
    )
    val KnownSemanticPrefixes = listOf(
        "parity:button:",
        "parity:home-button:",
        "parity:map-surface",
        "parity:map-selection-tray",
        "parity:ownship-launcher",
        "parity:ownship-source:",
        "parity:map-selection-action:",
        "parity:chart-search-input",
        "parity:plan-append-route-input",
        "parity:plan-control:",
        "parity:plan-row-action:",
        "parity:settings-section:",
        "parity:tray-option:",
    )

    private data class Entry(
        val owner: Any,
        val snapshot: E2eProjectionSnapshot,
    )

    private val revision = AtomicLong()
    private val touchSequences = ConcurrentHashMap<String, AtomicLong>()
    private val touchOwner = Any()
    private val entries = ConcurrentHashMap<String, Entry>()

    fun publish(resourceId: String, state: String, owner: Any, bounds: String? = null) {
        entries.compute(resourceId) { _, previous ->
            if (previous?.owner === owner &&
                previous.snapshot.state == state &&
                previous.snapshot.bounds == bounds
            ) {
                previous
            } else {
                Entry(
                    owner = owner,
                    snapshot = E2eProjectionSnapshot(state, bounds, revision.incrementAndGet()),
                )
            }
        }
    }

    fun remove(resourceId: String, owner: Any) {
        entries.computeIfPresent(resourceId) { _, entry ->
            if (entry.owner === owner) null else entry
        }
    }

    fun read(resourceId: String): E2eProjectionSnapshot? = entries[resourceId]?.snapshot

    fun publishTouchReceipt(
        rawX: Int = -1,
        rawY: Int = -1,
        handled: Boolean = true,
        semanticTag: String? = null,
    ) {
        val resourceId = touchReceiptResourceId(semanticTag)
        publish(
            resourceId = resourceId,
            state =
                "sequence:${touchSequences.computeIfAbsent(resourceId) { AtomicLong() }.incrementAndGet()}:" +
                    "x:$rawX:y:$rawY:handled:$handled",
            owner = touchOwner,
        )
    }

    fun touchReceiptResourceId(semanticTag: String?): String =
        if (semanticTag.isNullOrEmpty()) TouchReceiptResourceId
        else "$TouchReceiptResourceId:${Uri.encode(semanticTag)}"
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
        val resourceId = uri.getQueryParameter("resource_id") ?: return null
        val snapshot = E2eProjectionRegistry.read(resourceId)
        val resourceName = resourceId.removePrefix("org.aerobag.app:id/")
        val viewId = context?.resources?.getIdentifier(resourceName, "id", context?.packageName) ?: 0
        val knownSemanticControl = E2eProjectionRegistry.KnownSemanticPrefixes.any(resourceId::startsWith)
        if (viewId !in E2eProjectionRegistry.KnownViewIds && !knownSemanticControl && snapshot == null) {
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
