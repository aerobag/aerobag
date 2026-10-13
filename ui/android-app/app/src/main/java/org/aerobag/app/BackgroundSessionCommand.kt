// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app

import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

/** Native session commands can wait behind feed installation and snapshot work.
 * Only their result delivery belongs on the caller's (UI) dispatcher. Awaiting
 * delivery also keeps source flows backpressured, instead of queuing old samples.
 */
internal suspend fun <T> deliverBackgroundSessionCommand(
    operation: () -> T,
    onResult: (T) -> Unit,
    onError: (Throwable) -> Unit,
): Boolean {
    return try {
        val result = withContext(Dispatchers.IO) { operation() }
        onResult(result)
        true
    } catch (error: CancellationException) {
        throw error
    } catch (error: Throwable) {
        onError(error)
        false
    }
}
