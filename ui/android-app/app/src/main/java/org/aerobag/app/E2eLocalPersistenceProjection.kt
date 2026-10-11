// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app

import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.channels.Channel
import org.json.JSONObject

@Composable
internal fun E2eLocalPersistenceProjection(
    owner: Any,
    subscribeRevisions: ((Long) -> Unit) -> AutoCloseable,
    readStatus: suspend () -> String,
) {
    if (!BuildConfig.AEROBAG_E2E_ENABLED) return
    // Shell rendering intentionally ignores clock/revision-only updates. The
    // operational publisher must still observe the writer's completion checks.
    val revision by produceKeyedResourceState<Long>(owner) {
        val updates = Channel<Long>(Channel.CONFLATED)
        val subscription = subscribeRevisions { updates.trySend(it) }
        try {
            for (next in updates) value = maxOf(value ?: 0L, next)
        } finally {
            subscription.close()
            updates.close()
        }
    }
    revision?.let { E2eLocalPersistenceProjection(owner, it, readStatus) }
}

/** Observe receipts off the input thread, then publish through the ordinary
 * rendered index. Snapshot refreshes also run while a document write is pending.
 * A new session/revision must never inherit a previous owner's saved receipt.
 */
@Composable
internal fun E2eLocalPersistenceProjection(
    owner: Any,
    sessionRevision: Long,
    readStatus: suspend () -> String,
) {
    if (!BuildConfig.AEROBAG_E2E_ENABLED) return
    val status by produceKeyedResourceState<String>(owner, sessionRevision) {
        value = try {
            readStatus()
        } catch (error: CancellationException) {
            throw error
        } catch (error: Exception) {
            JSONObject().put("observation_error", error.toString()).toString()
        }
    }
    status?.let { E2eProjectionView(R.id.e2e_local_persistence_projection, it) }
}
