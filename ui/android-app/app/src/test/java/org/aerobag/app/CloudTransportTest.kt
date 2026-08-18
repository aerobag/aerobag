// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app

import java.util.concurrent.atomic.AtomicReference
import kotlinx.coroutines.runBlocking
import okhttp3.MediaType.Companion.toMediaType
import okhttp3.OkHttpClient
import okhttp3.Protocol
import okhttp3.Request
import okhttp3.Response
import okhttp3.ResponseBody.Companion.toResponseBody
import org.aerobag.app.generated.CloudHttpHeader
import org.aerobag.app.generated.CloudHttpMethod
import org.aerobag.app.generated.CloudHttpRequest
import org.aerobag.app.generated.CloudHttpResponse
import org.aerobag.app.generated.CloudProviderKind
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class CloudTransportTest {
    @Test
    fun cloudServerUrlPreservesCoreContractTrailingSlash() {
        assertEquals(
            "https://cloud.example/cloud/",
            parseAndroidCloudServerBaseUrl("  https://cloud.example/cloud/\n"),
        )
        assertEquals(null, parseAndroidCloudServerBaseUrl("  \n"))
    }

    @Test
    fun aerobagCloudPutPreservesCoreRequestWithoutOAuth() = runBlocking {
        val captured = AtomicReference<Request>()
        val client = recordingClient(captured)
        val body = "encrypted root".encodeToByteArray()
        val request = CloudHttpRequest(
            requestId = 7,
            provider = CloudProviderKind.AerobagCloud,
            method = CloudHttpMethod.Put,
            url = "https://cloud.example.test/cloud/v1/accounts/a/root",
            headers = listOf(
                CloudHttpHeader("content-type", "application/octet-stream"),
                CloudHttpHeader("x-aerobag-signature", "signed-by-core"),
            ),
            bodyBase64 = java.util.Base64.getUrlEncoder().withoutPadding().encodeToString(body),
            maxResponseBytes = 1024,
        )

        val response = executeCloudHttpRequest(request, client)

        val sent = captured.get()
        assertEquals("PUT", sent.method)
        assertArrayEquals(body, sent.body?.let { bodyBytes(it) })
        assertEquals("signed-by-core", sent.header("x-aerobag-signature"))
        assertNull(sent.header("Authorization"))
        val completed = response as CloudHttpResponse.Completed
        assertEquals(200, completed.statusCode)
        assertEquals("ok", java.util.Base64.getUrlDecoder().decode(completed.bodyBase64).decodeToString())
    }

    @Test
    fun aerobagCloudPostSendsExplicitEmptyBody() = runBlocking {
        val captured = AtomicReference<Request>()
        val request = CloudHttpRequest(
            requestId = 9,
            provider = CloudProviderKind.AerobagCloud,
            method = CloudHttpMethod.Post,
            url = "https://cloud.example.test/cloud/v1/accounts",
            headers = listOf(CloudHttpHeader("x-aerobag-signature", "signed-by-core")),
            bodyBase64 = null,
            maxResponseBytes = 1024,
        )

        val response = executeCloudHttpRequest(request, recordingClient(captured))

        val sent = captured.get()
        assertEquals("POST", sent.method)
        assertArrayEquals(ByteArray(0), sent.body?.let { bodyBytes(it) })
        assertNull(sent.header("Authorization"))
        assertEquals(200, (response as CloudHttpResponse.Completed).statusCode)
    }

    @Test
    fun sseAccumulatorIgnoresMetadataAndJoinsDataLines() {
        val accumulator = SseDataAccumulator()

        assertNull(accumulator.accept(": heartbeat"))
        assertNull(accumulator.accept("event: root-changed"))
        assertNull(accumulator.accept("data: {\"root\":"))
        assertNull(accumulator.accept("data: \"next\"}"))
        assertEquals("{\"root\":\n\"next\"}", accumulator.accept(""))
    }

    @Test
    fun sseAccumulatorFlushesPendingMessageAtEof() {
        val accumulator = SseDataAccumulator()

        assertNull(accumulator.accept("data: final"))
        assertEquals("final", accumulator.accept(null))
    }
}

private fun recordingClient(captured: AtomicReference<Request>): OkHttpClient =
    OkHttpClient.Builder()
        .addInterceptor { chain ->
            captured.set(chain.request())
            Response.Builder()
                .request(chain.request())
                .protocol(Protocol.HTTP_1_1)
                .code(200)
                .message("OK")
                .body("ok".toResponseBody("text/plain".toMediaType()))
                .build()
        }
        .build()

private fun bodyBytes(body: okhttp3.RequestBody): ByteArray {
    val buffer = okio.Buffer()
    body.writeTo(buffer)
    return buffer.readByteArray()
}
