// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app

import org.junit.Test

class RuntimeFetchTest {
    @Test
    fun androidAllowsPublicationPublicUrlResources() {
        requireAndroidPublicUrlAllowed("publication/current_artifacts")
    }

    @Test
    fun androidAllowsLiveFeedPublicUrlResources() {
        requireAndroidPublicUrlAllowed("live_feeds/nexrad/current")
    }

    @Test(expected = IllegalArgumentException::class)
    fun androidRejectsChartAssetPublicUrlResources() {
        requireAndroidPublicUrlAllowed("chart_asset/asset/KYKM/IAP-WA-ILS OR LOC RWY 27")
    }
}
