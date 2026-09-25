// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app.e2e;

import android.accessibilityservice.AccessibilityService;
import android.accessibilityservice.GestureDescription;
import android.database.Cursor;
import android.graphics.Path;
import android.graphics.Point;
import android.graphics.Rect;
import android.net.Uri;
import android.os.Bundle;
import android.os.CancellationSignal;
import android.os.OperationCanceledException;
import android.util.Log;
import android.view.WindowManager;
import android.view.accessibility.AccessibilityEvent;
import android.view.accessibility.AccessibilityNodeInfo;
import android.view.accessibility.AccessibilityWindowInfo;
import java.io.BufferedReader;
import java.io.IOException;
import java.io.OutputStream;
import java.io.InputStreamReader;
import java.net.InetAddress;
import java.net.ServerSocket;
import java.net.Socket;
import java.net.URLDecoder;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.Comparator;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import java.util.concurrent.ConcurrentHashMap;
import java.util.concurrent.atomic.AtomicBoolean;
import java.util.concurrent.atomic.AtomicLong;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;
import java.util.concurrent.ScheduledExecutorService;
import java.util.concurrent.ScheduledFuture;
import java.util.concurrent.TimeUnit;
import org.json.JSONArray;
import org.json.JSONException;
import org.json.JSONObject;

/** Test-only semantic driver that remains independent of Aerobag's process lifecycle. */
public final class SemanticDriverService extends AccessibilityService {
    private static final String LOG_TAG = "AerobagSemanticDriver";
    private static final String TARGET_PACKAGE = "org.aerobag.app";
    private static final int DRIVER_PORT = 19_191;
    private static final String DRIVER_PROTOCOL = "aerobag-semantic-driver/32";
    private static final long PROVIDER_QUERY_TIMEOUT_MS = 500;
    private static final long SLOW_PROVIDER_QUERY_MS = 100;
    private final AtomicBoolean running = new AtomicBoolean(false);
    private final AtomicBoolean semanticRequestActive = new AtomicBoolean(false);
    private final Object semanticRequestMonitor = new Object();
    private final AtomicLong accessibilityEventSequence = new AtomicLong();
    private final Object accessibilityEventMonitor = new Object();
    private final AtomicLong activeSemanticRequestStartedNanos = new AtomicLong();
    private volatile String activeSemanticRequest = "";
    private ServerSocket server;
    private Thread serverThread;
    private ExecutorService clientExecutor;
    private ScheduledExecutorService providerQueryTimeoutExecutor;

    @Override
    protected void onServiceConnected() {
        if (!running.compareAndSet(false, true)) return;
        clientExecutor = Executors.newFixedThreadPool(4);
        providerQueryTimeoutExecutor = Executors.newSingleThreadScheduledExecutor();
        serverThread = new Thread(this::serve, "aerobag-e2e-semantic-driver");
        serverThread.setDaemon(true);
        serverThread.start();
    }

    @Override
    public void onAccessibilityEvent(AccessibilityEvent event) {
        accessibilityEventSequence.incrementAndGet();
        synchronized (accessibilityEventMonitor) {
            accessibilityEventMonitor.notifyAll();
        }
    }

    @Override
    public void onInterrupt() {}

    @Override
    public void onDestroy() {
        running.set(false);
        if (server != null) {
            try {
                server.close();
            } catch (IOException ignored) {}
        }
        if (serverThread != null) serverThread.interrupt();
        if (clientExecutor != null) clientExecutor.shutdownNow();
        if (providerQueryTimeoutExecutor != null) providerQueryTimeoutExecutor.shutdownNow();
        super.onDestroy();
    }

    private void serve() {
        try (ServerSocket socket = new ServerSocket(
            DRIVER_PORT,
            1,
            InetAddress.getByName("127.0.0.1")
        )) {
            server = socket;
            while (running.get()) {
                Socket client = socket.accept();
                clientExecutor.execute(() -> handleClient(client));
            }
        } catch (IOException error) {
            if (running.get()) Log.e(LOG_TAG, "semantic server stopped", error);
        } finally {
            server = null;
            running.set(false);
        }
    }

    private void handleClient(Socket client) {
        try (client) {
            try {
                handleRequest(client);
            } catch (IOException error) {
                // A timed-out probe may close its socket while a changing
                // accessibility tree is still being rendered.
                if (running.get()) Log.w(LOG_TAG, "semantic client disconnected", error);
            } catch (ProjectionProviderBusyException error) {
                Log.w(LOG_TAG, error.getMessage());
                respondUnavailableBestEffort(client);
            } catch (RuntimeException error) {
                Log.e(LOG_TAG, "semantic request failed", error);
                respondFailureBestEffort(client, error);
            }
        } catch (IOException error) {
            if (running.get()) Log.w(LOG_TAG, "semantic client close failed", error);
        }
    }

    private void handleRequest(Socket socket) throws IOException {
        socket.setSoTimeout(5_000);
        BufferedReader reader = new BufferedReader(new InputStreamReader(
            socket.getInputStream(),
            StandardCharsets.US_ASCII
        ));
        String requestLine = reader.readLine();
        String[] request = requestLine == null ? new String[0] : requestLine.split(" ");
        consumeHeaders(reader);
        String path = request.length > 1 ? request[1] : "/";
        String endpoint = path.contains("?") ? path.substring(0, path.indexOf('?')) : path;
        boolean ownsSemanticRequest = false;
        if (requiresSerializedAccessibility(endpoint, path)) {
            ownsSemanticRequest = semanticRequestActive.compareAndSet(false, true);
            if (!ownsSemanticRequest) {
                respond(
                    socket.getOutputStream(),
                    "text/plain; charset=utf-8",
                    "semantic request busy\n",
                    503
                );
                return;
            }
            activeSemanticRequest = path;
            activeSemanticRequestStartedNanos.set(System.nanoTime());
        }
        try {
            switch (endpoint) {
                case "/health":
                    respond(
                        socket.getOutputStream(),
                        "text/plain; charset=utf-8",
                        DRIVER_PROTOCOL + "\n",
                        200
                    );
                    return;
                case "/request-state":
                    respond(
                        socket.getOutputStream(),
                        "application/json; charset=utf-8",
                        renderRequestState().toString() + "\n",
                        200
                    );
                    return;
                case "/dump":
                    respond(
                        socket.getOutputStream(),
                        "application/xml; charset=utf-8",
                        renderHierarchy(),
                        200
                    );
                    return;
                case "/query":
                    handleQuery(socket, path);
                    return;
                case "/exact-projection":
                    handleExactProjection(socket, path);
                    return;
                case "/await-event":
                    handleAwaitEvent(socket, path);
                    return;
                case "/await-idle":
                    handleAwaitIdle(socket, path);
                    return;
                case "/set-text":
                    handleSetText(socket, path);
                    return;
                case "/set-progress":
                    handleSetProgress(socket, path);
                    return;
                case "/tap":
                    handleTap(socket, path);
                    return;
                case "/scroll":
                    handleScroll(socket, path);
                    return;
                default:
                    respond(
                        socket.getOutputStream(),
                        "text/plain; charset=utf-8",
                        "not found\n",
                        404
                    );
            }
        } finally {
            if (ownsSemanticRequest) {
                synchronized (semanticRequestMonitor) {
                    activeSemanticRequest = "";
                    activeSemanticRequestStartedNanos.set(0);
                    semanticRequestActive.set(false);
                    semanticRequestMonitor.notifyAll();
                }
            }
        }
    }

    private JSONObject renderRequestState() {
        JSONObject state = new JSONObject();
        long startedNanos = activeSemanticRequestStartedNanos.get();
        try {
            state.put("active", semanticRequestActive.get());
            state.put("request", activeSemanticRequest);
            state.put(
                "elapsed_ms",
                startedNanos == 0
                    ? 0
                    : TimeUnit.NANOSECONDS.toMillis(System.nanoTime() - startedNanos)
            );
        } catch (JSONException error) {
            throw new IllegalStateException("failed to encode semantic request state", error);
        }
        return state;
    }

    private static boolean isSemanticEndpoint(String endpoint) {
        return switch (endpoint) {
            case "/dump", "/query", "/exact-projection", "/set-text", "/set-progress", "/tap", "/scroll" ->
                true;
            default -> false;
        };
    }

    private static boolean requiresSerializedAccessibility(String endpoint, String path) {
        if (!isSemanticEndpoint(endpoint)) return false;
        Map<String, String> query = queryOf(path);
        if ("/exact-projection".equals(endpoint) || "/query".equals(endpoint) || "/scroll".equals(endpoint)) {
            return false;
        }
        if ("/tap".equals(endpoint) &&
            query.getOrDefault("path", "").startsWith("projection-provider:")) {
            return false;
        }
        return true;
    }

    private void handleSetText(Socket socket, String path) throws IOException {
        Map<String, String> query = queryOf(path);
        String tag = query.getOrDefault("tag", "");
        String value = query.getOrDefault("value", "");
        String semanticPath = query.getOrDefault("path", "");
        Rect expectedBounds = parseBounds(query.getOrDefault("bounds", ""));
        boolean changed = !tag.isEmpty() && !semanticPath.isEmpty() && expectedBounds != null &&
            setRenderedText(tag, value, expectedBounds, semanticPath);
        respondAction(socket, changed, "text action rejected\n");
    }

    private void handleSetProgress(Socket socket, String path) throws IOException {
        Map<String, String> query = queryOf(path);
        String tag = query.getOrDefault("tag", "");
        String semanticPath = query.getOrDefault("path", "");
        Rect expectedBounds = parseBounds(query.getOrDefault("bounds", ""));
        float value;
        try {
            value = Float.parseFloat(query.getOrDefault("value", ""));
        } catch (NumberFormatException error) {
            value = Float.NaN;
        }
        boolean changed = !tag.isEmpty() && !semanticPath.isEmpty() && expectedBounds != null &&
            Float.isFinite(value) && setRenderedProgress(tag, value, expectedBounds, semanticPath);
        respondAction(socket, changed, "progress action rejected\n");
    }

    private void handleQuery(Socket socket, String path) throws IOException {
        Map<String, String> query = queryOf(path);
        String tag = query.getOrDefault("tag", "");
        JSONArray values = "true".equals(query.get("prefix"))
            ? providerProjectionPrefix(tag).values
            : providerProjection(tag, false).values;
        respond(socket.getOutputStream(), "application/json; charset=utf-8", values.toString() + "\n", 200);
    }

    private void handleExactProjection(Socket socket, String path) throws IOException {
        Map<String, String> query = queryOf(path);
        JSONArray values = providerProjection(
            query.getOrDefault("tag", ""),
            "true".equals(query.get("verify_reachable")),
            "true".equals(query.get("avoid_navigation"))
        ).values;
        respond(socket.getOutputStream(), "application/json; charset=utf-8", values.toString() + "\n", 200);
    }

    private void handleAwaitEvent(Socket socket, String path) throws IOException {
        long timeoutMs;
        try {
            timeoutMs = Long.parseLong(queryOf(path).getOrDefault("timeout_ms", "250"));
        } catch (NumberFormatException error) {
            timeoutMs = 250;
        }
        timeoutMs = Math.max(1, Math.min(1_000, timeoutMs));
        long sequence = accessibilityEventSequence.get();
        boolean changed = awaitAccessibilityEventAfter(sequence, timeoutMs);
        respond(
            socket.getOutputStream(),
            "text/plain; charset=utf-8",
            changed ? "changed\n" : "unchanged\n",
            200
        );
    }

    private void handleAwaitIdle(Socket socket, String path) throws IOException {
        long timeoutMs;
        try {
            timeoutMs = Long.parseLong(queryOf(path).getOrDefault("timeout_ms", "1000"));
        } catch (NumberFormatException error) {
            timeoutMs = 1000;
        }
        timeoutMs = Math.max(1, Math.min(2_000, timeoutMs));
        long deadlineNanos = System.nanoTime() + TimeUnit.MILLISECONDS.toNanos(timeoutMs);
        synchronized (semanticRequestMonitor) {
            while (semanticRequestActive.get()) {
                long remainingNanos = deadlineNanos - System.nanoTime();
                if (remainingNanos <= 0) break;
                try {
                    TimeUnit.NANOSECONDS.timedWait(semanticRequestMonitor, remainingNanos);
                } catch (InterruptedException error) {
                    Thread.currentThread().interrupt();
                    break;
                }
            }
        }
        respond(
            socket.getOutputStream(),
            "text/plain; charset=utf-8",
            semanticRequestActive.get() ? "busy\n" : "idle\n",
            200
        );
    }

    private void handleTap(Socket socket, String path) throws IOException {
        Map<String, String> query = queryOf(path);
        String tag = query.getOrDefault("tag", "");
        String semanticPath = query.getOrDefault("path", "");
        Rect bounds = parseBounds(query.getOrDefault("bounds", ""));
        Rect renderedBounds = !tag.isEmpty() && !semanticPath.isEmpty() &&
            bounds != null && !bounds.isEmpty()
                ? renderedTapBounds(tag, bounds, semanticPath)
                : null;
        if (renderedBounds == null) {
            respond(
                socket.getOutputStream(),
                "text/plain; charset=utf-8",
                "physical tap target rejected\n",
                409
            );
            return;
        }
        if (!dispatchTapGesture(renderedBounds)) {
            respond(
                socket.getOutputStream(),
                "text/plain; charset=utf-8",
                "physical tap gesture rejected\n",
                409
            );
            return;
        }
        respondAction(socket, true, "physical tap gesture rejected\n");
    }

    private boolean dispatchTapGesture(Rect bounds) {
        Path path = new Path();
        path.moveTo(bounds.exactCenterX(), bounds.exactCenterY());
        GestureDescription gesture = new GestureDescription.Builder()
            .addStroke(new GestureDescription.StrokeDescription(path, 0, 80))
            .build();
        // The surrounding journey transition requires the app-visible result;
        // this method is responsible only for validating and dispatching the
        // user's one physical gesture.
        return dispatchGesture(gesture, null, null);
    }

    private void handleScroll(Socket socket, String path) throws IOException {
        Map<String, String> query = queryOf(path);
        String tag = query.getOrDefault("tag", "");
        String evidence = query.getOrDefault("path", "");
        Rect bounds = parseBounds(query.getOrDefault("bounds", ""));
        String direction = query.getOrDefault("direction", "");
        ProviderSnapshot snapshot = providerSnapshot(tag);
        Map<String, String> fields = projectionStateFields(snapshot.state);
        boolean forward = "forward".equals(direction);
        if (bounds == null || !("forward".equals(direction) || "backward".equals(direction)) ||
            !"scroll".equals(fields.get("kind")) ||
            !"true".equals(fields.get(forward ? "forward" : "backward")) ||
            !currentProviderTargetMatches(tag, bounds, evidence, false, false)) {
            respondAction(socket, false, "scroll readiness changed\n");
            return;
        }
        boolean horizontal = "horizontal".equals(fields.get("orientation"));
        Rect gestureBounds = new Rect(bounds);
        gestureBounds.intersect(physicalDisplayBounds());
        Rect dock = indexedBounds("parity:primary-navigation");
        if (dock != null && Rect.intersects(gestureBounds, dock)) {
            gestureBounds.bottom = Math.min(gestureBounds.bottom, dock.top);
        }
        if (gestureBounds.isEmpty()) throw new IllegalStateException("Scroll surface has no unobscured gesture area");
        float distance = (horizontal ? gestureBounds.width() : gestureBounds.height()) * 0.35f;
        float cx = gestureBounds.exactCenterX(), cy = gestureBounds.exactCenterY();
        float sign = forward ? 1 : -1;
        Path gesturePath = new Path();
        gesturePath.moveTo(cx + (horizontal ? distance * sign : 0), cy + (horizontal ? 0 : distance * sign));
        gesturePath.lineTo(cx - (horizontal ? distance * sign : 0), cy - (horizontal ? 0 : distance * sign));
        GestureDescription gesture = new GestureDescription.Builder()
            .addStroke(new GestureDescription.StrokeDescription(gesturePath, 0, 250)).build();
        java.util.concurrent.CountDownLatch completed = new java.util.concurrent.CountDownLatch(1);
        AtomicBoolean delivered = new AtomicBoolean(false);
        boolean accepted = dispatchGesture(gesture, new GestureResultCallback() {
            @Override public void onCompleted(GestureDescription description) { delivered.set(true); completed.countDown(); }
            @Override public void onCancelled(GestureDescription description) { completed.countDown(); }
        }, new android.os.Handler(android.os.Looper.getMainLooper()));
        try {
            if (!accepted || !completed.await(1500, TimeUnit.MILLISECONDS) || !delivered.get()) {
                throw new IllegalStateException("Physical scroll delivery failed");
            }
        } catch (InterruptedException error) {
            Thread.currentThread().interrupt();
            throw new IllegalStateException("Physical scroll interrupted", error);
        }
        respondAction(socket, true, "physical scroll rejected\n");
    }


    private boolean awaitAccessibilityEventAfter(long sequence, long timeoutMs) {
        long deadlineNanos = System.nanoTime() + TimeUnit.MILLISECONDS.toNanos(timeoutMs);
        synchronized (accessibilityEventMonitor) {
            while (accessibilityEventSequence.get() <= sequence) {
                long remainingNanos = deadlineNanos - System.nanoTime();
                if (remainingNanos <= 0) return false;
                try {
                    TimeUnit.NANOSECONDS.timedWait(accessibilityEventMonitor, remainingNanos);
                } catch (InterruptedException error) {
                    Thread.currentThread().interrupt();
                    return false;
                }
            }
        }
        return true;
    }


    private static Map<String, String> queryOf(String path) {
        return parseQuery(path.contains("?") ? path.substring(path.indexOf('?') + 1) : "");
    }

    private static void respondAction(Socket socket, boolean accepted, String rejectedBody)
        throws IOException {
        respond(
            socket.getOutputStream(),
            "text/plain; charset=utf-8",
            accepted ? "ok\n" : rejectedBody,
            accepted ? 200 : 409
        );
    }

    private static void consumeHeaders(BufferedReader reader) throws IOException {
        while (true) {
            String line = reader.readLine();
            if (line == null || line.isEmpty()) return;
        }
    }

    private static Map<String, String> parseQuery(String query) {
        Map<String, String> values = new HashMap<>();
        for (String field : query.split("&")) {
            if (field.isEmpty()) continue;
            String[] parts = field.split("=", 2);
            values.put(
                URLDecoder.decode(parts[0], StandardCharsets.UTF_8),
                URLDecoder.decode(parts.length > 1 ? parts[1] : "", StandardCharsets.UTF_8)
            );
        }
        return values;
    }

    private static void respond(
        OutputStream output,
        String contentType,
        String body,
        int status
    ) throws IOException {
        byte[] bytes = body.getBytes(StandardCharsets.UTF_8);
        String reason = status == 200
            ? "OK"
            : status == 409
                ? "Conflict"
                : status == 500
                    ? "Internal Server Error"
                    : status == 503 ? "Service Unavailable" : "Not Found";
        output.write(("HTTP/1.1 " + status + " " + reason + "\r\n")
            .getBytes(StandardCharsets.US_ASCII));
        output.write(("Content-Type: " + contentType + "\r\n")
            .getBytes(StandardCharsets.US_ASCII));
        output.write(("Content-Length: " + bytes.length + "\r\n")
            .getBytes(StandardCharsets.US_ASCII));
        output.write("Connection: close\r\n\r\n".getBytes(StandardCharsets.US_ASCII));
        output.write(bytes);
        output.flush();
    }

    private static void respondFailureBestEffort(Socket socket, RuntimeException error) {
        try {
            respond(
                socket.getOutputStream(),
                "text/plain; charset=utf-8",
                "semantic request failed: " + error.getClass().getSimpleName() + "\n",
                500
            );
        } catch (IOException ignored) {}
    }

    private static void respondUnavailableBestEffort(Socket socket) {
        try {
            respond(
                socket.getOutputStream(),
                "text/plain; charset=utf-8",
                "projection provider busy\n",
                503
            );
        } catch (IOException ignored) {}
    }

    private String renderHierarchy() {
        StringBuilder output = new StringBuilder();
        output.append("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n");
        output.append("<hierarchy rotation=\"0\">\n");
        List<AccessibilityNodeInfo> roots = roots(true);
        try {
            for (int rootIndex = 0; rootIndex < roots.size(); rootIndex++) {
                appendNode(output, roots.get(rootIndex), rootIndex, Integer.toString(rootIndex));
            }
        } finally {
            recycleAll(roots);
        }
        output.append("</hierarchy>\n");
        return output.toString();
    }


    private ProviderProjection providerProjection(String tag) {
        return providerProjection(tag, true, false);
    }

    private ProviderProjection providerProjection(String tag, boolean verifyCenterReachable) {
        return providerProjection(tag, verifyCenterReachable, false);
    }

    private ProviderProjection providerProjection(
        String tag,
        boolean verifyCenterReachable,
        boolean avoidNavigation
    ) {
        ProviderSnapshot snapshot = providerSnapshot(tag);
        return providerProjection(snapshot, verifyCenterReachable, avoidNavigation);
    }

    private ProviderProjection providerProjection(
        ProviderSnapshot snapshot,
        boolean verifyCenterReachable,
        boolean avoidNavigation
    ) {
        JSONArray output = new JSONArray();
        if (!snapshot.present) return new ProviderProjection(output);
        try {
            Map<String, String> fields = projectionStateFields(snapshot.state);
            boolean hasBounds = snapshot.bounds != null && !snapshot.bounds.isEmpty();
            Rect parsedBounds = hasBounds ? parseBounds(snapshot.bounds) : null;
            JSONObject value = new JSONObject();
            value.put("resource-id", snapshot.resourceId);
            value.put("semantic-path", "projection-provider:" + snapshot.incarnation + ":" + snapshot.revision);
            value.put("text", Uri.decode(fields.getOrDefault("text", "")));
            value.put("enabled", fields.getOrDefault("enabled", "true"));
            value.put("visible", Boolean.toString(parsedBounds == null || !parsedBounds.isEmpty()));
            value.put("selected", fields.getOrDefault("selected", "false"));
            value.put("checked", fields.getOrDefault("checked", "false"));
            value.put("focused", fields.getOrDefault("focused", "false"));
            value.put("set-text-action", Boolean.toString(
                "text".equals(fields.getOrDefault("kind", ""))
            ));
            value.put(
                "input-connection-ready",
                Boolean.toString(SemanticDriverInputMethodService.focusedInputConnectionReady())
            );
            value.put("state-description", snapshot.state);
            value.put("bounds", hasBounds ? snapshot.bounds : "[0,0][1,1]");
            value.put("incarnation", snapshot.incarnation);
            value.put("scrollable", Boolean.toString("scroll".equals(fields.get("kind"))));
            for (String key : new String[]{"orientation", "position", "backward", "forward", "moving"}) {
                if (fields.containsKey(key)) value.put(key, fields.get(key));
            }
            value.put(
                "center-reachable",
                Boolean.toString(
                    parsedBounds != null && !parsedBounds.isEmpty() &&
                    "true".equals(fields.getOrDefault("window-focus", "false")) &&
                    (!verifyCenterReachable || projectedCenterReachable(parsedBounds)) &&
                    (!avoidNavigation || projectedCenterClearOfNavigation(snapshot.resourceId, parsedBounds))
                )
            );
            output.put(value);
            return new ProviderProjection(output);
        } catch (JSONException error) {
            throw new IllegalStateException("failed to encode projection provider snapshot", error);
        }
    }

    private ProviderProjection providerProjectionPrefix(String prefix) {
        ProviderSnapshotBatch batch = providerSnapshots("resource_id_prefix", prefix);
        JSONArray output = new JSONArray();
        for (ProviderSnapshot snapshot : batch.snapshots) {
            ProviderProjection projection = providerProjection(snapshot, true, false);
            for (int index = 0; index < projection.values.length(); index++) {
                try {
                    output.put(projection.values.getJSONObject(index));
                } catch (JSONException error) {
                    throw new IllegalStateException("Invalid observation batch", error);
                }
            }
        }
        return new ProviderProjection(output);
    }

    private ProviderSnapshot providerSnapshot(String tag) {
        ProviderSnapshotBatch batch = providerSnapshots("resource_id", tag);
        return batch.snapshots.isEmpty()
            ? ProviderSnapshot.absent()
            : batch.snapshots.get(0);
    }

    private ProviderSnapshotBatch providerSnapshots(String parameter, String value) {
        Uri uri = Uri.parse("content://org.aerobag.app.e2e-projections/projection")
            .buildUpon()
            .appendQueryParameter(parameter, value)
            .build();
        CancellationSignal cancellationSignal = new CancellationSignal();
        long startedNanos = System.nanoTime();
        ScheduledFuture<?> cancellation = providerQueryTimeoutExecutor.schedule(
            cancellationSignal::cancel,
            PROVIDER_QUERY_TIMEOUT_MS,
            TimeUnit.MILLISECONDS
        );
        try (Cursor cursor = getContentResolver().query(
            uri,
            null,
            null,
            null,
            null,
            cancellationSignal
        )) {
            if (cursor == null) {
                throw new IllegalStateException("Observation source returned no snapshot for " + value);
            }
            if (cursor.getExtras().getInt("schema") != 1 || cursor.getExtras().getString("incarnation") == null) {
                throw new IllegalStateException("Invalid observation snapshot envelope");
            }
            String incarnation = cursor.getExtras().getString("incarnation");
            List<ProviderSnapshot> snapshots = new ArrayList<>();
            while (cursor.moveToNext()) {
                snapshots.add(new ProviderSnapshot(
                    cursor.getInt(cursor.getColumnIndexOrThrow("present")) != 0,
                    cursor.getString(cursor.getColumnIndexOrThrow("resource_id")),
                    cursor.getString(cursor.getColumnIndexOrThrow("state")),
                    cursor.getString(cursor.getColumnIndexOrThrow("bounds")),
                    cursor.getLong(cursor.getColumnIndexOrThrow("revision")),
                    incarnation
                ));
            }
            return new ProviderSnapshotBatch(snapshots);
        } catch (OperationCanceledException error) {
            // The provider owns this semantic namespace. Report bounded IPC
            // pressure as transient instead of lying that a control is absent
            // or falling back to an accessibility-tree traversal.
            throw new ProjectionProviderBusyException(value);
        } catch (IllegalArgumentException | SecurityException error) {
            throw new IllegalStateException("projection provider unavailable for " + value, error);
        } finally {
            cancellation.cancel(false);
            long elapsedMs = TimeUnit.NANOSECONDS.toMillis(System.nanoTime() - startedNanos);
            if (elapsedMs >= SLOW_PROVIDER_QUERY_MS) {
                Log.w(LOG_TAG, "slow projection provider query value=" + value + " elapsed_ms=" + elapsedMs);
            }
        }
    }

    private boolean projectedCenterReachable(Rect bounds) {
        if (bounds.isEmpty()) return false;
        Rect displayBounds = physicalDisplayBounds();
        return displayBounds.contains(bounds.centerX(), bounds.centerY());
    }

    private boolean projectedCenterClearOfNavigation(String tag, Rect bounds) {
        if (!projectedCenterReachable(bounds)) return false;
        Rect navigationBounds = indexedBounds("parity:primary-navigation");
        if (navigationBounds == null || !navigationBounds.contains(bounds.centerX(), bounds.centerY())) {
            return true;
        }
        // The persistent navigation buttons are valid targets inside the dock.
        // Page controls whose center is under it must be scrolled clear before
        // action delivery verifies their rendered node.
        return tag.startsWith("parity:button:") && navigationBounds.contains(bounds);
    }

    @SuppressWarnings("deprecation")
    private Rect physicalDisplayBounds() {
        Point size = new Point();
        getSystemService(WindowManager.class).getDefaultDisplay().getRealSize(size);
        return new Rect(0, 0, size.x, size.y);
    }

    private Rect indexedBounds(String tag) {
        ProviderSnapshot snapshot = providerSnapshot(tag);
        return snapshot.present ? parseBounds(snapshot.bounds) : null;
    }

    private static Map<String, String> projectionStateFields(String state) {
        Map<String, String> fields = new HashMap<>();
        if (state == null) return fields;
        String[] components = state.split(":");
        for (int index = 0; index + 1 < components.length; index += 2) {
            fields.put(components[index], components[index + 1]);
        }
        return fields;
    }

    private static final class ProjectionProviderBusyException extends RuntimeException {
        ProjectionProviderBusyException(String resourceId) {
            super("projection provider timed out for " + resourceId);
        }
    }

    private static final class ProviderSnapshot {
        final boolean present;
        final String resourceId;
        final String state;
        final String bounds;
        final long revision;
        final String incarnation;

        ProviderSnapshot(
            boolean present,
            String resourceId,
            String state,
            String bounds,
            long revision,
            String incarnation
        ) {
            this.present = present;
            this.resourceId = resourceId;
            this.state = state;
            this.bounds = bounds;
            this.revision = revision;
            this.incarnation = incarnation;
        }

        static ProviderSnapshot absent() {
            return new ProviderSnapshot(false, "", "", null, 0, "");
        }
    }

    private static final class ProviderSnapshotBatch {
        final List<ProviderSnapshot> snapshots;

        ProviderSnapshotBatch(List<ProviderSnapshot> snapshots) {
            this.snapshots = snapshots;
        }
    }

    private static final class ProviderProjection {
        final JSONArray values;

        ProviderProjection(JSONArray values) {
            this.values = values;
        }
    }

    @SuppressWarnings("deprecation")

    @SuppressWarnings("deprecation")
    private boolean centerReachable(AccessibilityNodeInfo node) {
        Rect bounds = new Rect();
        node.getBoundsInScreen(bounds);
        if (!node.isVisibleToUser() || bounds.isEmpty()) return false;
        int centerX = bounds.centerX();
        int centerY = bounds.centerY();
        Rect displayBounds = getSystemService(WindowManager.class)
            .getCurrentWindowMetrics()
            .getBounds();
        if (!displayBounds.contains(centerX, centerY)) return false;
        AccessibilityNodeInfo ancestor = node.getParent();
        while (ancestor != null) {
            AccessibilityNodeInfo next = null;
            try {
                Rect ancestorBounds = new Rect();
                ancestor.getBoundsInScreen(ancestorBounds);
                if (!ancestorBounds.contains(centerX, centerY)) return false;
                next = ancestor.getParent();
            } finally {
                ancestor.recycle();
            }
            ancestor = next;
        }
        return true;
    }

    @SuppressWarnings("deprecation")

    private boolean setRenderedText(
        String tag,
        String value,
        Rect expectedBounds,
        String semanticPath
    ) {
        if (!semanticPath.startsWith("projection-provider:") || !currentProviderTargetMatches(
            tag,
            expectedBounds,
            semanticPath,
            true,
            true
        )) return false;
        return SemanticDriverInputMethodService.replaceFocusedText(value);
    }

    private boolean setRenderedProgress(
        String tag,
        float value,
        Rect expectedBounds,
        String semanticPath
    ) {
        for (int attempt = 0; attempt < 3; attempt++) {
            if (!semanticPath.startsWith("projection-provider:") ||
                !currentProviderTargetMatches(tag, expectedBounds, semanticPath, false, false)) return false;
            long sequence = accessibilityEventSequence.get();
            AccessibilityNodeInfo node = resolveRenderedNode(tag, expectedBounds, semanticPath);
            if (node != null) {
                try {
                    if (setMatchingNodeProgress(node, tag, value, expectedBounds)) return true;
                } finally {
                    node.recycle();
                }
            }
            if (attempt < 2) awaitAccessibilityEventAfter(sequence, 750);
        }
        return false;
    }

    @SuppressWarnings("deprecation")
    private AccessibilityNodeInfo resolveRenderedNode(
        String tag,
        Rect expectedBounds,
        String semanticPath
    ) {
        boolean projectedGeometry = semanticPath.startsWith("projection-provider:");
        AccessibilityNodeInfo indexed = findIndexedRenderedNode(
            tag,
            expectedBounds,
            projectedGeometry
        );
        if (indexed != null) return indexed;

        AccessibilityNodeInfo node = nodeAtPath(semanticPath);
        if (node != null) {
            node.refresh();
            if (matchesRenderedTarget(node, tag, expectedBounds)) return node;
            node.recycle();
        }

        // Compose may renumber semantics children during an unrelated
        // recomposition. Resolve that case like a real tap: descend only
        // through nodes covering the readiness point, then require the same
        // semantic tag and exact bounds before delivering the action.
        List<AccessibilityNodeInfo> roots = targetRoots(true);
        try {
            for (AccessibilityNodeInfo root : roots) {
                AccessibilityNodeInfo match = findRenderedNodeAtPoint(
                    root,
                    tag,
                    expectedBounds,
                    projectedGeometry
                );
                if (match != null) return match;
            }
            return null;
        } finally {
            recycleAll(roots);
        }
    }

    private AccessibilityNodeInfo findIndexedRenderedNode(
        String tag,
        Rect expectedBounds,
        boolean acceptProjectedGeometry
    ) {
        List<AccessibilityNodeInfo> roots = targetRoots(true);
        try {
            for (AccessibilityNodeInfo root : roots) {
                List<AccessibilityNodeInfo> indexed = root.findAccessibilityNodeInfosByViewId(tag);
                if (indexed == null) continue;
                try {
                    for (AccessibilityNodeInfo match : indexed) {
                        match.refresh();
                        if (matchesRenderedTarget(match, tag, expectedBounds) ||
                            (acceptProjectedGeometry &&
                                matchesProjectedTarget(match, tag, expectedBounds))) {
                            return AccessibilityNodeInfo.obtain(match);
                        }
                    }
                } finally {
                    recycleAll(indexed);
                }
            }
            return null;
        } finally {
            recycleAll(roots);
        }
    }

    @SuppressWarnings("deprecation")
    private static AccessibilityNodeInfo findRenderedNodeAtPoint(
        AccessibilityNodeInfo node,
        String tag,
        Rect expectedBounds,
        boolean acceptProjectedGeometry
    ) {
        node.refresh();
        Rect bounds = new Rect();
        node.getBoundsInScreen(bounds);
        boolean geometryMatches = acceptProjectedGeometry
            ? Rect.intersects(bounds, expectedBounds)
            : bounds.contains(expectedBounds.centerX(), expectedBounds.centerY());
        if (!geometryMatches) return null;
        if (matchesRenderedTarget(node, tag, expectedBounds) ||
            (acceptProjectedGeometry && tag.equals(node.getViewIdResourceName()))) {
            return AccessibilityNodeInfo.obtain(node);
        }
        for (int childIndex = 0; childIndex < node.getChildCount(); childIndex++) {
            AccessibilityNodeInfo child = childAtOrNull(node, childIndex);
            if (child == null) continue;
            try {
                AccessibilityNodeInfo match = findRenderedNodeAtPoint(
                    child,
                    tag,
                    expectedBounds,
                    acceptProjectedGeometry
                );
                if (match != null) return match;
            } finally {
                child.recycle();
            }
        }
        return null;
    }

    private static boolean matchesRenderedTarget(
        AccessibilityNodeInfo node,
        String tag,
        Rect expectedBounds
    ) {
        if (!tag.equals(node.getViewIdResourceName())) return false;
        Rect bounds = new Rect();
        node.getBoundsInScreen(bounds);
        return bounds.equals(expectedBounds);
    }

    private static boolean matchesProjectedTarget(
        AccessibilityNodeInfo node,
        String tag,
        Rect expectedBounds
    ) {
        if (!tag.equals(node.getViewIdResourceName())) return false;
        Rect bounds = new Rect();
        node.getBoundsInScreen(bounds);
        return Rect.intersects(bounds, expectedBounds);
    }

    @SuppressWarnings("deprecation")
    private AccessibilityNodeInfo nodeAtPath(String semanticPath) {
        if (!semanticPath.matches("[0-9]+(?:/[0-9]+)*")) return null;
        String[] components = semanticPath.split("/");
        int rootIndex;
        try {
            rootIndex = Integer.parseInt(components[0]);
        } catch (NumberFormatException error) {
            return null;
        }
        List<AccessibilityNodeInfo> roots = targetRoots(true);
        AccessibilityNodeInfo current = null;
        try {
            if (rootIndex < 0 || rootIndex >= roots.size()) return null;
            current = AccessibilityNodeInfo.obtain(roots.get(rootIndex));
        } finally {
            recycleAll(roots);
        }
        for (int componentIndex = 1; componentIndex < components.length; componentIndex++) {
            int childIndex;
            try {
                childIndex = Integer.parseInt(components[componentIndex]);
            } catch (NumberFormatException error) {
                current.recycle();
                return null;
            }
            AccessibilityNodeInfo child = childAtOrNull(current, childIndex);
            current.recycle();
            if (child == null) return null;
            current = child;
        }
        return current;
    }


    private List<AccessibilityNodeInfo> roots(boolean topFirst) {
        return roots(topFirst, null);
    }

    private List<AccessibilityNodeInfo> targetRoots(boolean topFirst) {
        return roots(topFirst, TARGET_PACKAGE);
    }

    private List<AccessibilityNodeInfo> roots(boolean topFirst, String requiredPackage) {
        List<AccessibilityWindowInfo> ordered = new ArrayList<>(getWindows());
        try {
            ordered.sort(Comparator.comparingInt(AccessibilityWindowInfo::getLayer));
            if (topFirst) {
                ordered.sort(Comparator.comparingInt(AccessibilityWindowInfo::getLayer).reversed());
            }
            List<AccessibilityNodeInfo> roots = new ArrayList<>();
            for (AccessibilityWindowInfo window : ordered) {
                AccessibilityNodeInfo root = window.getRoot();
                if (root != null) {
                    if (requiredPackage == null || requiredPackage.equals(
                        string(root.getPackageName())
                    )) {
                        root.refresh();
                        roots.add(root);
                    } else {
                        root.recycle();
                    }
                }
            }
            if (roots.isEmpty()) {
                AccessibilityNodeInfo activeRoot = getRootInActiveWindow();
                if (activeRoot != null) {
                    if (requiredPackage == null || requiredPackage.equals(
                        string(activeRoot.getPackageName())
                    )) {
                        activeRoot.refresh();
                        roots.add(activeRoot);
                    } else {
                        activeRoot.recycle();
                    }
                }
            }
            return roots;
        } finally {
            recycleWindows(ordered);
        }
    }

    @SuppressWarnings("deprecation")
    private static void recycleWindows(List<AccessibilityWindowInfo> windows) {
        for (AccessibilityWindowInfo window : windows) window.recycle();
    }

    @SuppressWarnings("deprecation")
    private static void recycleAll(List<AccessibilityNodeInfo> nodes) {
        for (AccessibilityNodeInfo node : nodes) node.recycle();
    }

    private static boolean supportsAction(AccessibilityNodeInfo node, int actionId) {
        for (AccessibilityNodeInfo.AccessibilityAction action : node.getActionList()) {
            if (action.getId() == actionId) return true;
        }
        return false;
    }

    private static AccessibilityNodeInfo childAtOrNull(
        AccessibilityNodeInfo node,
        int childIndex
    ) {
        try {
            if (childIndex < 0 || childIndex >= node.getChildCount()) return null;
            return node.getChild(childIndex);
        } catch (IndexOutOfBoundsException error) {
            // Compose can replace its virtual child array between getChildCount
            // and getChild. A changed tree is a stale lookup, not a driver crash.
            return null;
        }
    }

    @SuppressWarnings("deprecation")
    private static boolean setMatchingNodeProgress(
        AccessibilityNodeInfo node,
        String tag,
        float value,
        Rect expectedBounds
    ) {
        node.refresh();
        if (!tag.equals(node.getViewIdResourceName())) return false;
        Rect bounds = new Rect();
        node.getBoundsInScreen(bounds);
        if (!bounds.equals(expectedBounds) || !node.isVisibleToUser() || !node.isEnabled()) {
            return false;
        }
        Bundle arguments = new Bundle();
        arguments.putFloat(AccessibilityNodeInfo.ACTION_ARGUMENT_PROGRESS_VALUE, value);
        return node.performAction(
            AccessibilityNodeInfo.AccessibilityAction.ACTION_SET_PROGRESS.getId(),
            arguments
        );
    }

    private Rect renderedTapBounds(String tag, Rect expectedBounds, String semanticPath) {
        if (!semanticPath.startsWith("projection-provider:") || !currentProviderTargetMatches(
            tag, expectedBounds, semanticPath, false, false
        )) return null;
        return new Rect(expectedBounds);
    }

    private boolean currentProviderTargetMatches(
        String tag,
        Rect expectedBounds,
        String semanticPath,
        boolean requireFocused,
        boolean requireSetText
    ) {
        ProviderProjection projection = providerProjection(tag, true);
        if (projection.values.length() != 1) return false;
        try {
            JSONObject value = projection.values.getJSONObject(0);
            Rect currentBounds = parseBounds(value.optString("bounds", ""));
            return semanticPath.equals(value.optString("semantic-path", "")) &&
                currentBounds != null && expectedBounds.equals(currentBounds) &&
                "true".equals(value.optString("enabled", "false")) &&
                "true".equals(value.optString("visible", "false")) &&
                "true".equals(value.optString("center-reachable", "false")) &&
                (!requireFocused || "true".equals(value.optString("focused", "false"))) &&
                (!requireSetText || "true".equals(value.optString("set-text-action", "false")));
        } catch (JSONException error) {
            return false;
        }
    }

    @SuppressWarnings("deprecation")

    private static Rect parseBounds(String value) {
        String normalized = value
            .replace("][", " ")
            .replace("[", "")
            .replace("]", "")
            .replace(",", " ")
            .trim();
        String[] fields = normalized.split("\\s+");
        if (fields.length != 4) return null;
        try {
            return new Rect(
                Integer.parseInt(fields[0]),
                Integer.parseInt(fields[1]),
                Integer.parseInt(fields[2]),
                Integer.parseInt(fields[3])
            );
        } catch (NumberFormatException error) {
            return null;
        }
    }

    @SuppressWarnings("deprecation")
    private static void appendNode(
        StringBuilder output,
        AccessibilityNodeInfo node,
        int index,
        String semanticPath
    ) {
        appendNode(output, node, index, semanticPath, false);
    }

    @SuppressWarnings("deprecation")
    private static void appendNode(
        StringBuilder output,
        AccessibilityNodeInfo node,
        int index,
        String semanticPath,
        boolean refreshSubtree
    ) {
        String viewId = string(node.getViewIdResourceName());
        boolean refreshThisSubtree = refreshSubtree || viewId.startsWith("parity:");
        if (refreshThisSubtree) node.refresh();
        Rect bounds = new Rect();
        node.getBoundsInScreen(bounds);
        output.append("<node");
        attribute(output, "index", Integer.toString(index));
        attribute(output, "semantic-path", semanticPath);
        attribute(output, "text", string(node.getText()));
        attribute(output, "resource-id", string(node.getViewIdResourceName()));
        attribute(output, "class", string(node.getClassName()));
        attribute(output, "package", string(node.getPackageName()));
        attribute(output, "content-desc", string(node.getContentDescription()));
        attribute(output, "state-description", string(node.getStateDescription()));
        attribute(output, "checkable", Boolean.toString(node.isCheckable()));
        attribute(output, "checked", Boolean.toString(node.isChecked()));
        attribute(output, "clickable", Boolean.toString(node.isClickable()));
        attribute(output, "enabled", Boolean.toString(node.isEnabled()));
        attribute(output, "focusable", Boolean.toString(node.isFocusable()));
        attribute(output, "focused", Boolean.toString(node.isFocused()));
        attribute(output, "scrollable", Boolean.toString(node.isScrollable()));
        attribute(output, "long-clickable", Boolean.toString(node.isLongClickable()));
        attribute(output, "password", Boolean.toString(node.isPassword()));
        attribute(output, "selected", Boolean.toString(node.isSelected()));
        attribute(
            output,
            "bounds",
            "[" + bounds.left + "," + bounds.top + "][" + bounds.right + "," + bounds.bottom + "]"
        );
        output.append('>');
        for (int childIndex = 0; childIndex < node.getChildCount(); childIndex++) {
            AccessibilityNodeInfo child = childAtOrNull(node, childIndex);
            if (child == null) continue;
            try {
                appendNode(
                    output,
                    child,
                    childIndex,
                    semanticPath + "/" + childIndex,
                    refreshThisSubtree
                );
            } finally {
                child.recycle();
            }
        }
        output.append("</node>\n");
    }
    private static String string(CharSequence value) {
        return value == null ? "" : value.toString();
    }

    private static void attribute(StringBuilder output, String name, String value) {
        output.append(' ').append(name).append("=\"").append(xmlEscape(value)).append('"');
    }

    private static String xmlEscape(String value) {
        StringBuilder output = new StringBuilder(value.length());
        for (int index = 0; index < value.length(); index++) {
            char character = value.charAt(index);
            switch (character) {
                case '&': output.append("&amp;"); break;
                case '<': output.append("&lt;"); break;
                case '>': output.append("&gt;"); break;
                case '"': output.append("&quot;"); break;
                case '\'': output.append("&apos;"); break;
                default:
                    if (character >= ' ' || character == '\n' || character == '\t') {
                        output.append(character);
                    }
            }
        }
        return output.toString();
    }
}
