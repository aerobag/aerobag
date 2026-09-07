// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app

import org.aerobag.app.generated.UiInvalidation
import org.junit.Assert.assertEquals
import org.junit.Test

class UiInvalidationRevisionsTest {
    @Test
    fun everyGeneratedInvalidationCanDriveAQueryRevision() {
        val revisions = UiInvalidationRevisions().bumped(UiInvalidation.entries)

        UiInvalidation.entries.forEach { invalidation ->
            assertEquals(1, revisions[invalidation])
        }
    }
}
