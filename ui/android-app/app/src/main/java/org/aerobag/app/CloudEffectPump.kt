// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app

import android.util.Log
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.compose.LocalLifecycleOwner
import androidx.lifecycle.repeatOnLifecycle
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.delay
import kotlinx.coroutines.isActive
import kotlinx.coroutines.withContext
import org.aerobag.app.domain.NativeUiSession
import org.aerobag.app.domain.UiSessionSnapshot
import org.aerobag.app.generated.CloudEventStreamEvent

private const val CloudEffectLogTag = "AerobagCloudEffect"

@Composable
internal fun CloudEffectPump(
    uiSession: NativeUiSession,
    onSnapshot: (UiSessionSnapshot) -> Unit,
) {
    val runtime = remember(uiSession) { AndroidCloudProviderRuntime() }
    val eventStreamTransport = remember(uiSession) { AndroidCloudEventStreamTransport() }
    val eventStreamEvents = remember(uiSession) { Channel<CloudEventStreamEvent>(Channel.UNLIMITED) }
    val currentOnSnapshot by rememberUpdatedState(onSnapshot)
    val lifecycleOwner = LocalLifecycleOwner.current

    DisposableEffect(eventStreamTransport, eventStreamEvents) {
        onDispose {
            eventStreamTransport.close()
            eventStreamEvents.close()
        }
    }

    LaunchedEffect(uiSession, runtime, lifecycleOwner) {
        lifecycleOwner.lifecycle.repeatOnLifecycle(Lifecycle.State.STARTED) {
            try {
                while (isActive) {
                    var didWork = false
                    while (true) {
                        val event = eventStreamEvents.tryReceive().getOrNull() ?: break
                        didWork = true
                        runCatching {
                            withContext(Dispatchers.Default) {
                                uiSession.reportCloudEventStreamEvent(event, System.currentTimeMillis())
                            }
                        }.onSuccess(currentOnSnapshot)
                            .onFailure { Log.w(CloudEffectLogTag, "event stream report failed", it) }
                    }

                    val streamPlan = withContext(Dispatchers.Default) {
                        uiSession.cloudEventStreamPlan()
                    }
                    eventStreamTransport.reconcile(streamPlan, this) { event ->
                        eventStreamEvents.trySend(event)
                    }

                    val httpRequest = withContext(Dispatchers.Default) {
                        uiSession.takeCloudProviderRequest(System.currentTimeMillis())
                    }
                    if (httpRequest != null) {
                        didWork = true
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
                            .onFailure {
                                Log.w(CloudEffectLogTag, "provider HTTP completion failed", it)
                            }
                        }
                    delay(if (didWork) 1 else 250)
                }
            } finally {
                eventStreamTransport.close()
            }
        }
    }
}
