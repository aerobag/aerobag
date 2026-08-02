// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app

import android.app.Activity
import android.app.PendingIntent
import android.content.Intent
import android.util.Base64
import com.google.android.gms.auth.api.identity.AuthorizationRequest
import com.google.android.gms.auth.api.identity.AuthorizationResult
import com.google.android.gms.auth.api.identity.Identity
import com.google.android.gms.common.api.ApiException
import com.google.android.gms.common.api.Scope
import com.google.android.gms.tasks.Task
import java.io.IOException
import kotlin.coroutines.resume
import kotlin.coroutines.resumeWithException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.suspendCancellableCoroutine
import kotlinx.coroutines.withContext
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import okhttp3.Headers
import okhttp3.MediaType.Companion.toMediaTypeOrNull
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.RequestBody.Companion.toRequestBody
import okio.Buffer
import org.aerobag.app.generated.CloudAuthorizationMode
import org.aerobag.app.generated.CloudAuthorizationRequest
import org.aerobag.app.generated.CloudAuthorizationResponse
import org.aerobag.app.generated.CloudHttpMethod
import org.aerobag.app.generated.CloudHttpRequest
import org.aerobag.app.generated.CloudHttpResponse
import org.aerobag.app.generated.CloudProviderKind
import org.aerobag.app.generated.CloudProviderPrincipal

private const val DriveAboutUrl =
    "https://www.googleapis.com/drive/v3/about?fields=user(permissionId,displayName,emailAddress)"
private const val DrivePrincipalResponseLimit = 64L * 1024L

internal sealed interface AndroidCloudAuthorizationStep {
    data class Complete(val response: CloudAuthorizationResponse) : AndroidCloudAuthorizationStep
    data class NeedsResolution(val pendingIntent: PendingIntent) : AndroidCloudAuthorizationStep
}

internal class AndroidCloudProviderRuntime(
    activity: Activity,
    private val httpClient: OkHttpClient = OkHttpClient(),
) {
    private val authorizationClient = Identity.getAuthorizationClient(activity)
    private val json = Json { ignoreUnknownKeys = true }

    @Volatile
    private var credential: CloudCredential? = null

    suspend fun beginAuthorization(request: CloudAuthorizationRequest): AndroidCloudAuthorizationStep {
        if (request.provider != CloudProviderKind.GoogleDrive) {
            return AndroidCloudAuthorizationStep.Complete(
                CloudAuthorizationResponse.PermanentFailure("Aerobag Cloud is not available in this build."),
            )
        }
        return try {
            val result = authorizationClient.authorize(
                AuthorizationRequest.builder()
                    .setRequestedScopes(request.scopes.map(::Scope))
                    .build(),
            ).await()
            if (result.hasResolution()) {
                if (request.mode == CloudAuthorizationMode.Silent) {
                    AndroidCloudAuthorizationStep.Complete(
                        CloudAuthorizationResponse.InteractionRequired(
                            "Google Drive requires user authorization.",
                        ),
                    )
                } else {
                    AndroidCloudAuthorizationStep.NeedsResolution(requireNotNull(result.pendingIntent))
                }
            } else {
                AndroidCloudAuthorizationStep.Complete(completeAuthorization(result))
            }
        } catch (error: Throwable) {
            AndroidCloudAuthorizationStep.Complete(classifyAuthorizationFailure(error))
        }
    }

    suspend fun completeAuthorizationIntent(data: Intent?): CloudAuthorizationResponse =
        try {
            completeAuthorization(authorizationClient.getAuthorizationResultFromIntent(requireNotNull(data)))
        } catch (error: Throwable) {
            classifyAuthorizationFailure(error)
        }

    fun authorizationCanceled(): CloudAuthorizationResponse =
        CloudAuthorizationResponse.Denied("Google Drive authorization was canceled.")

    suspend fun executeHttp(request: CloudHttpRequest): CloudHttpResponse = withContext(Dispatchers.IO) {
        val activeCredential = credential
        if (activeCredential?.provider != request.provider) {
            return@withContext CloudHttpResponse.TransportError(
                "Cloud provider authorization is unavailable.",
            )
        }
        try {
            val headers = Headers.Builder().apply {
                request.headers.forEach { add(it.name, it.value) }
                set("Authorization", "Bearer ${activeCredential.accessToken}")
            }.build()
            val requestBody = request.bodyBase64?.let(::decodeBase64Url)?.toRequestBody(
                request.headers
                    .firstOrNull { it.name.equals("content-type", ignoreCase = true) }
                    ?.value
                    ?.toMediaTypeOrNull(),
            )
            val call = Request.Builder()
                .url(request.url)
                .headers(headers)
                .method(
                    when (request.method) {
                        CloudHttpMethod.Get -> "GET"
                        CloudHttpMethod.Post -> "POST"
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

    private suspend fun completeAuthorization(result: AuthorizationResult): CloudAuthorizationResponse {
        val accessToken = result.accessToken?.takeIf(String::isNotBlank)
            ?: return CloudAuthorizationResponse.PermanentFailure(
                "Google Drive authorization returned no access token.",
            )
        return try {
            val principal = readDrivePrincipal(accessToken)
            credential = CloudCredential(CloudProviderKind.GoogleDrive, accessToken)
            CloudAuthorizationResponse.Authorized(
                expiresAtEpochMs = null,
                principal = principal,
            )
        } catch (error: Throwable) {
            classifyAuthorizationFailure(error)
        }
    }

    private suspend fun readDrivePrincipal(accessToken: String): CloudProviderPrincipal =
        withContext(Dispatchers.IO) {
            val request = Request.Builder()
                .url(DriveAboutUrl)
                .header("Authorization", "Bearer $accessToken")
                .build()
            httpClient.newCall(request).execute().use { response ->
                if (!response.isSuccessful) {
                    throw DrivePrincipalException(
                        response.code,
                        "Could not identify the authorized Google Drive account: HTTP ${response.code}",
                    )
                }
                val bytes = readBoundedResponse(response.body, DrivePrincipalResponseLimit)
                    ?: throw IOException("Google Drive principal response exceeded $DrivePrincipalResponseLimit bytes")
                val user = json.parseToJsonElement(bytes.decodeToString())
                    .jsonObject["user"]
                    ?.jsonObject
                    ?: error("Google Drive did not provide account identity")
                val stableId = user["permissionId"]?.jsonPrimitive?.content?.trim().orEmpty()
                require(stableId.isNotEmpty()) { "Google Drive did not provide a stable account identifier" }
                CloudProviderPrincipal(
                    stableId = stableId,
                    displayLabel = user["emailAddress"]?.jsonPrimitive?.content?.trim()
                        ?.takeIf(String::isNotEmpty)
                        ?: user["displayName"]?.jsonPrimitive?.content?.trim()
                            ?.takeIf(String::isNotEmpty)
                        ?: "Google Drive user",
                )
            }
        }
}

private data class CloudCredential(
    val provider: CloudProviderKind,
    val accessToken: String,
)

private class DrivePrincipalException(val statusCode: Int, message: String) : IOException(message)

private fun classifyAuthorizationFailure(error: Throwable): CloudAuthorizationResponse {
    val detail = error.message ?: error::class.simpleName ?: "Google Drive authorization failed"
    if (error is DrivePrincipalException) {
        return when {
            error.statusCode == 401 || error.statusCode == 403 ->
                CloudAuthorizationResponse.InteractionRequired(detail)
            error.statusCode == 408 || error.statusCode == 425 || error.statusCode == 429 || error.statusCode >= 500 ->
                CloudAuthorizationResponse.TransientFailure(detail)
            else -> CloudAuthorizationResponse.PermanentFailure(detail)
        }
    }
    if (error is IOException) {
        return CloudAuthorizationResponse.TransientFailure(detail)
    }
    if (error is ApiException && error.statusCode == com.google.android.gms.common.api.CommonStatusCodes.CANCELED) {
        return CloudAuthorizationResponse.Denied(detail)
    }
    return CloudAuthorizationResponse.PermanentFailure(detail)
}

private suspend fun <T> Task<T>.await(): T = suspendCancellableCoroutine { continuation ->
    addOnSuccessListener { continuation.resume(it) }
    addOnFailureListener { continuation.resumeWithException(it) }
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
    Base64.decode(value, Base64.URL_SAFE or Base64.NO_WRAP or Base64.NO_PADDING)

private fun encodeBase64Url(bytes: ByteArray): String =
    Base64.encodeToString(bytes, Base64.URL_SAFE or Base64.NO_WRAP or Base64.NO_PADDING)
