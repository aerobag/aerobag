// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app

import android.content.Context

internal fun createInitialLiveFeedPromotionGate(context: Context): InitialLiveFeedPromotionGate =
    createFileControlledInitialLiveFeedPromotionGate(context, enabled = true)
