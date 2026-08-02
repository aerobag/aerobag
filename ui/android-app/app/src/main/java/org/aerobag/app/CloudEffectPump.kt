// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app

import android.content.Context
import android.content.ContextWrapper
import android.util.Log
import androidx.activity.ComponentActivity
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.IntentSenderRequest
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.ui.platform.LocalContext
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.repeatOnLifecycle
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import org.aerobag.app.domain.NativeUiSession
import org.aerobag.app.domain.UiSessionSnapshot
import org.aerobag.app.generated.CloudAuthorizationRequest
import org.aerobag.app.generated.CloudAuthorizationResponse

private const val CloudEffectLogTag = "AerobagCloudEffect"

@Composable
internal fun CloudEffectPump(
    uiSession: NativeUiSession,
    onSnapshot: (UiSessionSnapshot) -> Unit,
) {
    val activity = requireNotNull(LocalContext.current.findComponentActivity()) {
        "Cloud authorization requires an Activity context"
    }
    val runtime = remember(activity) { AndroidCloudProviderRuntime(activity) }
    val currentOnSnapshot by rememberUpdatedState(onSnapshot)
    val scope = rememberCoroutineScope()
    var pendingInteractiveRequest by remember(uiSession) {
        mutableStateOf<CloudAuthorizationRequest?>(null)
    }

    suspend fun completeAuthorization(
        request: CloudAuthorizationRequest,
        response: org.aerobag.app.generated.CloudAuthorizationResponse,
    ) {
        val snapshot = withContext(Dispatchers.Default) {
            uiSession.completeCloudAuthorization(request.requestId, response, System.currentTimeMillis())
        }
        currentOnSnapshot(snapshot)
    }

    val authorizationLauncher = rememberLauncherForActivityResult(
        ActivityResultContracts.StartIntentSenderForResult(),
    ) { result ->
        val request = pendingInteractiveRequest ?: return@rememberLauncherForActivityResult
        pendingInteractiveRequest = null
        scope.launch {
            Log.i(
                CloudEffectLogTag,
                "authorization activity completed request=${request.requestId} " +
                    "resultCode=${result.resultCode} hasData=${result.data != null}",
            )
            // Google can return a useful ApiException in the intent even when the
            // Activity result is not RESULT_OK. Parse it before calling this a cancellation.
            val response = result.data?.let { runtime.completeAuthorizationIntent(it) }
                ?: runtime.authorizationCanceled()
            Log.i(
                CloudEffectLogTag,
                "authorization response request=${request.requestId} ${response.logSummary()}",
            )
            runCatching { completeAuthorization(request, response) }
                .onFailure { Log.w(CloudEffectLogTag, "authorization completion failed", it) }
        }
    }

    LaunchedEffect(uiSession, runtime) {
        activity.lifecycle.repeatOnLifecycle(Lifecycle.State.STARTED) {
            while (isActive) {
                if (pendingInteractiveRequest != null) {
                    delay(250)
                    continue
                }
                val authorizationRequest = withContext(Dispatchers.Default) {
                    uiSession.takeCloudAuthorizationRequest(System.currentTimeMillis())
                }
                if (authorizationRequest != null) {
                    Log.i(
                        CloudEffectLogTag,
                        "authorization requested request=${authorizationRequest.requestId} " +
                            "mode=${authorizationRequest.mode}",
                    )
                    when (val step = runtime.beginAuthorization(authorizationRequest)) {
                        is AndroidCloudAuthorizationStep.Complete -> {
                            Log.i(
                                CloudEffectLogTag,
                                "authorization response request=${authorizationRequest.requestId} " +
                                    step.response.logSummary(),
                            )
                            runCatching { completeAuthorization(authorizationRequest, step.response) }
                                .onFailure { Log.w(CloudEffectLogTag, "authorization failed", it) }
                        }

                        is AndroidCloudAuthorizationStep.NeedsResolution -> {
                            Log.i(
                                CloudEffectLogTag,
                                "authorization needs user resolution request=${authorizationRequest.requestId}",
                            )
                            pendingInteractiveRequest = authorizationRequest
                            authorizationLauncher.launch(
                                IntentSenderRequest.Builder(step.pendingIntent.intentSender).build(),
                            )
                        }
                    }
                    continue
                }

                val httpRequest = withContext(Dispatchers.Default) {
                    uiSession.takeCloudProviderRequest(System.currentTimeMillis())
                }
                if (httpRequest != null) {
                    val response = runtime.executeHttp(httpRequest)
                    runCatching {
                        withContext(Dispatchers.Default) {
                            uiSession.completeCloudProviderRequest(
                                httpRequest.requestId,
                                response,
                                System.currentTimeMillis(),
                            )
                        }
                    }.onSuccess(currentOnSnapshot)
                        .onFailure { Log.w(CloudEffectLogTag, "provider HTTP completion failed", it) }
                    continue
                }
                delay(1_000)
            }
        }
    }
}

private fun CloudAuthorizationResponse.logSummary(): String = when (this) {
    is CloudAuthorizationResponse.Authorized -> "state=authorized"
    is CloudAuthorizationResponse.InteractionRequired ->
        "state=interaction_required diagnostic=${diagnostic.orEmpty()}"
    is CloudAuthorizationResponse.Denied -> "state=denied diagnostic=${diagnostic.orEmpty()}"
    is CloudAuthorizationResponse.TransientFailure ->
        "state=transient_failure diagnostic=${diagnostic.orEmpty()}"
    is CloudAuthorizationResponse.PermanentFailure ->
        "state=permanent_failure diagnostic=${diagnostic.orEmpty()}"
}

private tailrec fun Context.findComponentActivity(): ComponentActivity? = when (this) {
    is ComponentActivity -> this
    is ContextWrapper -> baseContext.findComponentActivity()
    else -> null
}
