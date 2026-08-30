// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app

import org.aerobag.app.domain.OwnshipControlTone
import org.aerobag.app.domain.OwnshipSourceKind
import org.aerobag.app.domain.OwnshipSourceMenuItem
import org.aerobag.app.domain.OwnshipSourcePowerState
import org.aerobag.app.domain.SituationControlInput
import org.junit.Assert.assertEquals
import org.junit.Test

class AndroidGpsPowerTest {
    @Test
    fun dismissingActiveNotificationRequestsTheSameCorePauseAsItsButton() {
        assertEquals(
            SituationControlInput.Pause,
            AndroidGpsPower.controlInput(AndroidGpsPower.PauseAction),
        )
        assertEquals(
            SituationControlInput.Pause,
            AndroidGpsPower.controlInput(AndroidGpsPower.NotificationDismissedAction),
        )
    }

    @Test
    fun registrationCarriesPersistedPowerStateIntoCore() {
        assertEquals(
            OwnshipSourcePowerState.Running,
            AndroidGpsSource.registration(paused = false).powerState,
        )
        assertEquals(
            OwnshipSourcePowerState.Paused,
            AndroidGpsSource.registration(paused = true).powerState,
        )
    }

    @Test
    fun pausedAndroidGpsSourceKeepsModeLabelInTray() {
        val source = OwnshipSourceMenuItem(
            sourceId = AndroidGpsSource.SourceId,
            sourceKind = OwnshipSourceKind.DeviceGps,
            label = "GPS",
            launcherLabel = "GPS PAUSED",
            tone = OwnshipControlTone.Neutral,
            enabled = true,
            active = true,
            statusLabel = "Paused",
            powerState = OwnshipSourcePowerState.Paused,
        )

        assertEquals("GPS", situationSourceButtonLabel(source))
    }
}
