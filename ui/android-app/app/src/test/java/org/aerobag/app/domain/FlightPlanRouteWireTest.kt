// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app.domain

import kotlinx.serialization.json.Json
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class FlightPlanRouteWireTest {
    private val json = Json { ignoreUnknownKeys = false }

    @Test
    fun decodesTheCompleteCoreRouteSegmentContract() {
        val projection = json.decodeFromString<WireFlightPlanRouteProjection>(
            """
            {
              "flight_plan_route_revision": 7,
              "segments": [
                {
                  "id": "route-0",
                  "leg_id": "leg-0",
                  "from": {"lat": 47.49, "lon": -122.22},
                  "to": {"lat": 47.56, "lon": -122.63},
                  "path": [
                    {"lat": 47.49, "lon": -122.22},
                    {"lat": 47.56, "lon": -122.63}
                  ],
                  "style": "solid",
                  "geometry": {
                    "kind": "arc",
                    "center": {"lat": 47.50, "lon": -122.40},
                    "radius_nm": 8.5,
                    "start": {"lat": 47.49, "lon": -122.22},
                    "end": {"lat": 47.56, "lon": -122.63},
                    "clockwise": true,
                    "sweep_degrees": 42.0
                  },
                  "distance_nm": 18.2,
                  "course_deg": 286.0,
                  "status": "active",
                  "finish_lines": [
                    {
                      "start": {"lat": 47.55, "lon": -122.64},
                      "end": {"lat": 47.57, "lon": -122.62}
                    }
                  ]
                }
              ],
              "distance_annotations": []
            }
            """.trimIndent(),
        )

        val segment = projection.segments.single()
        assertTrue(segment.geometry is WireGuidanceRouteGeometry.Arc)
        assertEquals(42.0, (segment.geometry as WireGuidanceRouteGeometry.Arc).sweep_degrees, 0.0)
        assertEquals(1, segment.finish_lines.size)
        assertEquals(47.57, segment.finish_lines.single().end.lat, 0.0)
    }
}
