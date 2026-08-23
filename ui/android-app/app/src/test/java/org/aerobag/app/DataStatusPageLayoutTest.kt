// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app

import org.aerobag.app.domain.UiDataStatusPageFact
import org.junit.Assert.assertEquals
import org.junit.Test

class DataStatusPageLayoutTest {
    @Test
    fun fullWidthFactsOwnTheirRowsWithoutReorderingNeighbors() {
        val facts = listOf(
            fact("Package"),
            fact("Contract"),
            fact("File", fullWidth = true),
            fact("Cycle"),
            fact("Cycle version"),
            fact("Commit", fullWidth = true),
            fact("Expires"),
        )

        assertEquals(
            listOf(
                listOf("Package", "Contract"),
                listOf("File"),
                listOf("Cycle", "Cycle version"),
                listOf("Commit"),
                listOf("Expires"),
            ),
            dataStatusFactRows(facts).map { row -> row.map(UiDataStatusPageFact::label) },
        )
    }

    private fun fact(label: String, fullWidth: Boolean = false) = UiDataStatusPageFact(
        label = label,
        value = label,
        fullWidth = fullWidth,
        actionId = null,
        linkUrl = null,
        relativeValue = null,
    )
}
