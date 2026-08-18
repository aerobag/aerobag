// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app

import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import okhttp3.Headers
import okhttp3.MediaType.Companion.toMediaTypeOrNull
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.RequestBody.Companion.toRequestBody
import okio.Buffer
import org.aerobag.app.generated.CloudHttpMethod
import org.aerobag.app.generated.CloudHttpRequest
import org.aerobag.app.generated.CloudHttpResponse

internal class AndroidCloudProviderRuntime(
    private val httpClient: OkHttpClient = OkHttpClient(),
) {
    suspend fun executeHttp(request: CloudHttpRequest): CloudHttpResponse =
        executeCloudHttpRequest(request, httpClient)
}

internal suspend fun executeCloudHttpRequest(
    request: CloudHttpRequest,
    httpClient: OkHttpClient,
): CloudHttpResponse = withContext(Dispatchers.IO) {
    try {
        val headers = Headers.Builder().apply {
            request.headers.forEach { add(it.name, it.value) }
        }.build()
        val contentType = request.headers
            .firstOrNull { it.name.equals("content-type", ignoreCase = true) }
            ?.value
            ?.toMediaTypeOrNull()
        val requestBody = when {
            request.bodyBase64 != null -> decodeBase64Url(request.bodyBase64).toRequestBody(contentType)
            request.method == CloudHttpMethod.Post || request.method == CloudHttpMethod.Put ->
                ByteArray(0).toRequestBody(contentType)
            else -> null
        }
        val call = Request.Builder()
            .url(request.url)
            .headers(headers)
            .method(
                when (request.method) {
                    CloudHttpMethod.Get -> "GET"
                    CloudHttpMethod.Post -> "POST"
                    CloudHttpMethod.Put -> "PUT"
                    CloudHttpMethod.Delete -> "DELETE"
                },
                requestBody,
            )
            .build()
        httpClient.newCall(call).execute().use { response ->
            val bytes = readBoundedResponse(response.body, request.maxResponseBytes)
                ?: return@withContext CloudHttpResponse.ResponseTooLarge(request.maxResponseBytes)
            CloudHttpResponse.Completed(
                statusCode = response.code,
                bodyBase64 = encodeBase64Url(bytes),
            )
        }
    } catch (error: Throwable) {
        CloudHttpResponse.TransportError(error.message ?: error::class.simpleName ?: "HTTP request failed")
    }
}

private fun readBoundedResponse(body: okhttp3.ResponseBody?, limit: Long): ByteArray? {
    if (body == null) return ByteArray(0)
    if (body.contentLength() > limit) return null
    val source = body.source()
    val buffer = Buffer()
    while (true) {
        val read = source.read(buffer, 8_192L)
        if (read == -1L) break
        if (buffer.size > limit) return null
    }
    return buffer.readByteArray()
}

private fun decodeBase64Url(value: String): ByteArray =
    java.util.Base64.getUrlDecoder().decode(value)

private fun encodeBase64Url(bytes: ByteArray): String =
    java.util.Base64.getUrlEncoder().withoutPadding().encodeToString(bytes)
