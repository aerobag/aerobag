// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app.domain

import kotlinx.serialization.KSerializer
import kotlinx.serialization.Serializable
import kotlinx.serialization.SerializationException
import kotlinx.serialization.descriptors.SerialDescriptor
import kotlinx.serialization.encoding.Decoder
import kotlinx.serialization.encoding.Encoder
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonNull
import org.aerobag.app.generated.UiInvalidation
import org.junit.Assert.*
import org.junit.Test

class PagedSessionResultDecoderTest {
    @Serializable
    data class Point(val x: Double, val y: Double)

    @Serializable
    data class Payload(val points: List<Point>)

    private val json = Json { ignoreUnknownKeys = false }

    @Test
    fun payloadUsesStreamingDecoderEvenWhenStateIsLast() {
        val decoders = mutableListOf<String>()
        val delegate = Payload.serializer()
        val observed = object : KSerializer<Payload> {
            override val descriptor: SerialDescriptor = delegate.descriptor
            override fun serialize(encoder: Encoder, value: Payload) = delegate.serialize(encoder, value)
            override fun deserialize(decoder: Decoder): Payload {
                decoders += decoder.javaClass.simpleName
                return delegate.deserialize(decoder)
            }
        }
        val codec = PagedSessionResultDecoder(json, observed)
        val outcome = codec.decode("""{"result":{"points":[{"x":1.5,"y":2.5}]},"invalidations":["map_overlay"],"state":"complete"}""")
        assertEquals(Payload(listOf(Point(1.5, 2.5))), codec.result(outcome))
        assertEquals(listOf(UiInvalidation.MapOverlay), outcome.invalidations)
        // A DOM decoder here reinstates the measured allocation-heavy second pass.
        assertEquals(listOf("StreamingJsonDecoder"), decoders)
    }

    @Test
    fun typedResultRetainsStrictFieldAndTypeChecking() {
        val codec = PagedSessionResultDecoder(json, Payload.serializer())
        for (result in listOf(
            """{"points":[],"unknown":true}""",
            """{"points":[{"x":1}]}""",
            """{"points":"not an array"}""",
        )) {
            assertThrows(SerializationException::class.java) {
                codec.decode("""{"state":"complete","result":$result}""")
            }
        }
        assertThrows(IllegalStateException::class.java) {
            codec.result(codec.decode("""{"state":"complete"}"""))
        }
    }

    @Test
    fun resourceRequestsDoNotRequireOrDecodeAResult() {
        val codec = PagedSessionResultDecoder(json, Payload.serializer())
        for (state in listOf("need_resources", "need_snapshot_resources")) {
            val outcome = codec.decode("""{"state":"$state","resources":[{"id":"a","source":{"kind":"public_url","url":"/a"}}],"invalidations":["map_overlay"]}""")
            assertEquals(state, outcome.state)
            assertNull(outcome.result)
            assertEquals("a", parseCoreResourceRequest(outcome.resources.single()).id)
            assertEquals(listOf(UiInvalidation.MapOverlay), outcome.invalidations)
        }
    }

    @Test
    fun untypedCallersRetainNullResultSemantics() {
        val codec = PagedSessionResultDecoder(json, JsonElement.serializer()) { JsonNull }
        for (encoded in listOf("""{"state":"complete"}""", """{"state":"complete","result":null}""")) {
            assertEquals(JsonNull, codec.result(codec.decode(encoded)))
        }
    }
}
