// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app

import androidx.compose.runtime.Composable
import androidx.compose.runtime.ProduceStateScope
import androidx.compose.runtime.State
import androidx.compose.runtime.key
import androidx.compose.runtime.produceState

/** Resource identity owns both the coroutine and its state, not just the job.
 * produceState(keys) alone retains the old value until the new effect runs.
 */
@Composable
internal fun <T> produceKeyedResourceState(
    vararg resourceKeys: Any?,
    producer: suspend ProduceStateScope<T?>.() -> Unit,
): State<T?> = key(resourceKeys.toList()) {
    produceState(initialValue = null, producer = producer)
}
