// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app.e2e;

import android.accessibilityservice.AccessibilityService;
import android.database.Cursor;
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
import java.util.ArrayDeque;
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
    private static final String DRIVER_PROTOCOL = "aerobag-semantic-driver/23";
    private static final String TOUCH_RECEIPT_RESOURCE_ID =
        "org.aerobag.app:id/e2e_touch_receipt";
    private static final int EXACT_PROJECTION_NODE_LIMIT = 8_192;
    private static final long EXACT_PROJECTION_TIME_LIMIT_NANOS = TimeUnit.MILLISECONDS.toNanos(750);
    private static final long PROVIDER_QUERY_TIMEOUT_MS = 500;
    private static final long SLOW_PROVIDER_QUERY_MS = 100;
    private final AtomicBoolean running = new AtomicBoolean(false);
    private final AtomicBoolean semanticRequestActive = new AtomicBoolean(false);
    private final Object semanticRequestMonitor = new Object();
    private final AtomicLong accessibilityEventSequence = new AtomicLong();
    private final Object accessibilityEventMonitor = new Object();
    private final Map<String, String> exactNodePaths = new ConcurrentHashMap<>();
    private final Map<String, Rect> exactNodeBounds = new ConcurrentHashMap<>();
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
        if (isSemanticEndpoint(endpoint)) {
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
                case "/ime-ready":
                    respond(
                        socket.getOutputStream(),
                        "text/plain; charset=utf-8",
                        SemanticDriverInputMethodService.focusedTextReady() ? "ready\n" : "not-ready\n",
                        200
                    );
                    return;
                case "/set-progress":
                    handleSetProgress(socket, path);
                    return;
                case "/tap-target":
                    handleTapTarget(socket, path);
                    return;
                case "/await-touch":
                    handleAwaitTouch(socket, path);
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
            case "/dump", "/query", "/exact-projection", "/ime-ready", "/set-text", "/set-progress", "/tap-target", "/scroll" ->
                true;
            default -> false;
        };
    }

    private void handleSetText(Socket socket, String path) throws IOException {
        Map<String, String> query = queryOf(path);
        String tag = query.getOrDefault("tag", "");
        String value = query.getOrDefault("value", "");
        boolean changed = !tag.isEmpty() && replaceIndexedFocusedText(tag, value);
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
        boolean prefix = "true".equals(query.getOrDefault("prefix", "false"));
        boolean first = "true".equals(query.getOrDefault("first", "false"));
        boolean includeDescendantText = !"false".equals(
            query.getOrDefault("descendant_text", "true")
        );
        respond(
            socket.getOutputStream(),
            "application/json; charset=utf-8",
            renderNodeQuery(tag, prefix, first, includeDescendantText).toString() + "\n",
            200
        );
    }

    private void handleExactProjection(Socket socket, String path) throws IOException {
        Map<String, String> query = queryOf(path);
        String tag = query.getOrDefault("tag", "");
        boolean includeDescendantText = "true".equals(
            query.getOrDefault("descendant_text", "false")
        );
        boolean indexedOnly = "true".equals(query.getOrDefault("indexed_only", "false"));
        boolean boundedOnly = "true".equals(query.getOrDefault("bounded_only", "false"));
        boolean providerOnly = "true".equals(query.getOrDefault("provider_only", "false"));
        boolean renderedOnly = "true".equals(query.getOrDefault("rendered_only", "false"));
        boolean verifyReachable = "true".equals(
            query.getOrDefault("verify_reachable", "false")
        );
        boolean avoidNavigation = "true".equals(
            query.getOrDefault("avoid_navigation", "false")
        );
        respond(
            socket.getOutputStream(),
            "application/json; charset=utf-8",
            renderExactProjection(
                tag,
                includeDescendantText,
                indexedOnly,
                boundedOnly,
                providerOnly,
                renderedOnly,
                verifyReachable,
                avoidNavigation
            ).toString() + "\n",
            200
        );
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

    private void handleTapTarget(Socket socket, String path) throws IOException {
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
        JSONObject target = new JSONObject();
        try {
            String touchTag = semanticPath.startsWith("projection-provider:") ? tag : "";
            target.put("bounds", renderedBounds.toShortString());
            target.put("touch_tag", touchTag);
            target.put("touch_sequence", currentTouchReceipt(touchTag).sequence);
        } catch (JSONException error) {
            throw new IllegalStateException("failed to encode physical tap target", error);
        }
        respond(
            socket.getOutputStream(),
            "application/json; charset=utf-8",
            target.toString() + "\n",
            200
        );
    }

    private void handleAwaitTouch(Socket socket, String path) throws IOException {
        Map<String, String> query = queryOf(path);
        long after;
        long timeoutMs;
        try {
            after = Long.parseLong(query.getOrDefault("after", "0"));
            timeoutMs = Long.parseLong(query.getOrDefault("timeout_ms", "500"));
        } catch (NumberFormatException error) {
            after = 0;
            timeoutMs = 500;
        }
        timeoutMs = Math.max(1, Math.min(750, timeoutMs));
        Rect bounds = parseBounds(query.getOrDefault("bounds", ""));
        String touchTag = query.getOrDefault("tag", "");
        boolean received = bounds != null && awaitTouchAfter(after, bounds, touchTag, timeoutMs);
        respond(
            socket.getOutputStream(),
            "text/plain; charset=utf-8",
            received ? "received\n" : "unreceived\n",
            200
        );
    }

    private void handleScroll(Socket socket, String path) throws IOException {
        Map<String, String> query = queryOf(path);
        Rect bounds = parseBounds(query.getOrDefault("bounds", ""));
        String orientation = query.getOrDefault("orientation", "");
        String direction = query.getOrDefault("direction", "");
        int action = "forward".equals(direction)
            ? AccessibilityNodeInfo.ACTION_SCROLL_FORWARD
            : "backward".equals(direction)
                ? AccessibilityNodeInfo.ACTION_SCROLL_BACKWARD
                : 0;
        long eventSequence = accessibilityEventSequence.get();
        boolean scrolled = action != 0 && (
            bounds != null
                ? scrollRenderedNode(bounds, action)
                : scrollFirstRenderedSurface(orientation, action)
        );
        if (scrolled) awaitAccessibilityQuietAfter(eventSequence, 150, 750);
        respondAction(socket, scrolled, "scroll action rejected\n");
    }

    private void awaitAccessibilityQuietAfter(long sequence, long quietMs, long timeoutMs) {
        long deadlineNanos = System.nanoTime() + TimeUnit.MILLISECONDS.toNanos(timeoutMs);
        long observedSequence = sequence;
        while (System.nanoTime() < deadlineNanos) {
            long remainingMs = Math.max(
                1,
                TimeUnit.NANOSECONDS.toMillis(deadlineNanos - System.nanoTime())
            );
            if (!awaitAccessibilityEventAfter(observedSequence, Math.min(quietMs, remainingMs))) {
                return;
            }
            observedSequence = accessibilityEventSequence.get();
        }
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

    private boolean awaitTouchAfter(
        long sequence,
        Rect expectedBounds,
        String touchTag,
        long timeoutMs
    ) {
        long deadlineNanos = System.nanoTime() + TimeUnit.MILLISECONDS.toNanos(timeoutMs);
        do {
            TouchReceipt receipt = currentTouchReceipt(touchTag);
            boolean exactTaggedReceipt = !touchTag.isEmpty();
            if (receipt.sequence > sequence && receipt.handled &&
                (exactTaggedReceipt || expectedBounds.contains(receipt.rawX, receipt.rawY))) {
                return true;
            }
            try {
                Thread.sleep(10);
            } catch (InterruptedException error) {
                Thread.currentThread().interrupt();
                return false;
            }
        } while (System.nanoTime() < deadlineNanos);
        return false;
    }

    private TouchReceipt currentTouchReceipt(String touchTag) {
        String resourceId = touchTag.isEmpty()
            ? TOUCH_RECEIPT_RESOURCE_ID
            : TOUCH_RECEIPT_RESOURCE_ID + ":" + Uri.encode(touchTag);
        ProviderProjection projection = providerProjection(resourceId);
        if (!projection.handled || projection.values.length() != 1) return TouchReceipt.Empty;
        try {
            JSONObject value = projection.values.getJSONObject(0);
            Map<String, String> fields = projectionStateFields(
                value.optString("state-description", "")
            );
            return new TouchReceipt(
                Long.parseLong(fields.getOrDefault("sequence", "0")),
                Integer.parseInt(fields.getOrDefault("x", "-1")),
                Integer.parseInt(fields.getOrDefault("y", "-1")),
                Boolean.parseBoolean(fields.getOrDefault("handled", "false"))
            );
        } catch (JSONException | NumberFormatException error) {
            return TouchReceipt.Empty;
        }
    }

    private static final class TouchReceipt {
        static final TouchReceipt Empty = new TouchReceipt(0, -1, -1, false);

        final long sequence;
        final int rawX;
        final int rawY;
        final boolean handled;

        TouchReceipt(long sequence, int rawX, int rawY, boolean handled) {
            this.sequence = sequence;
            this.rawX = rawX;
            this.rawY = rawY;
            this.handled = handled;
        }
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

    private JSONArray renderNodeQuery(
        String tag,
        boolean prefix,
        boolean first,
        boolean includeDescendantText
    ) {
        JSONArray output = new JSONArray();
        if (tag.isEmpty()) return output;
        if (!prefix && appendIndexedNodeQuery(tag, output, includeDescendantText)) return output;
        if (!prefix && appendCachedNodeQuery(tag, output, includeDescendantText)) return output;
        List<AccessibilityNodeInfo> roots = targetRoots(true);
        try {
            for (int rootIndex = 0; rootIndex < roots.size(); rootIndex++) {
                boolean matched = collectMatchingNodes(
                    roots.get(rootIndex),
                    tag,
                    prefix,
                    first,
                    output,
                    null,
                    Integer.toString(rootIndex),
                    includeDescendantText
                );
                if (matched && (!prefix || first)) {
                    break;
                }
            }
        } catch (JSONException error) {
            throw new IllegalStateException("failed to encode semantic query result", error);
        } finally {
            recycleAll(roots);
        }
        return output;
    }

    private boolean appendIndexedNodeQuery(
        String tag,
        JSONArray output,
        boolean includeDescendantText
    ) {
        List<AccessibilityNodeInfo> roots = targetRoots(true);
        try {
            for (AccessibilityNodeInfo root : roots) {
                List<AccessibilityNodeInfo> indexed = root.findAccessibilityNodeInfosByViewId(tag);
                if (indexed == null) continue;
                try {
                    for (AccessibilityNodeInfo match : indexed) {
                        match.refresh();
                        if (!tag.equals(match.getViewIdResourceName())) continue;
                        Rect bounds = new Rect();
                        match.getBoundsInScreen(bounds);
                        appendNodeQueryValue(
                            output,
                            match,
                            centerReachable(match),
                            "indexed",
                            includeDescendantText
                        );
                        exactNodePaths.put(tag, "indexed");
                        exactNodeBounds.put(tag, new Rect(bounds));
                        return true;
                    }
                } finally {
                    recycleAll(indexed);
                }
            }
            return false;
        } catch (JSONException error) {
            throw new IllegalStateException("failed to encode indexed semantic query", error);
        } finally {
            recycleAll(roots);
        }
    }

    private boolean appendCachedNodeQuery(
        String tag,
        JSONArray output,
        boolean includeDescendantText
    ) {
        String semanticPath = exactNodePaths.get(tag);
        Rect expectedBounds = exactNodeBounds.get(tag);
        if (semanticPath == null || expectedBounds == null) return false;
        AccessibilityNodeInfo node = nodeAtPath(semanticPath);
        if (node != null) {
            try {
                node.refresh();
                Rect bounds = new Rect();
                node.getBoundsInScreen(bounds);
                if (tag.equals(node.getViewIdResourceName()) && bounds.equals(expectedBounds)) {
                    appendNodeQueryValue(
                        output,
                        node,
                        centerReachable(node),
                        semanticPath,
                        includeDescendantText
                    );
                    return true;
                }
            } catch (JSONException error) {
                throw new IllegalStateException("failed to encode cached semantic query", error);
            } finally {
                node.recycle();
            }
        }
        if (appendCachedNodeQueryAtPoint(
            tag,
            expectedBounds,
            output,
            includeDescendantText
        )) return true;
        exactNodePaths.remove(tag, semanticPath);
        exactNodeBounds.remove(tag, expectedBounds);
        return false;
    }

    private boolean appendCachedNodeQueryAtPoint(
        String tag,
        Rect expectedBounds,
        JSONArray output,
        boolean includeDescendantText
    ) {
        List<AccessibilityNodeInfo> roots = targetRoots(true);
        try {
            for (int rootIndex = 0; rootIndex < roots.size(); rootIndex++) {
                if (appendCachedNodeQueryAtPoint(
                    roots.get(rootIndex),
                    tag,
                    expectedBounds,
                    Integer.toString(rootIndex),
                    output,
                    includeDescendantText
                )) {
                    return true;
                }
            }
            return false;
        } catch (JSONException error) {
            throw new IllegalStateException("failed to repair cached semantic query", error);
        } finally {
            recycleAll(roots);
        }
    }

    @SuppressWarnings("deprecation")
    private boolean appendCachedNodeQueryAtPoint(
        AccessibilityNodeInfo node,
        String tag,
        Rect expectedBounds,
        String semanticPath,
        JSONArray output,
        boolean includeDescendantText
    ) throws JSONException {
        node.refresh();
        Rect bounds = new Rect();
        node.getBoundsInScreen(bounds);
        if (!bounds.contains(expectedBounds.centerX(), expectedBounds.centerY())) return false;
        if (tag.equals(node.getViewIdResourceName()) && bounds.equals(expectedBounds)) {
            appendNodeQueryValue(
                output,
                node,
                centerReachable(node),
                semanticPath,
                includeDescendantText
            );
            exactNodePaths.put(tag, semanticPath);
            exactNodeBounds.put(tag, new Rect(bounds));
            return true;
        }
        for (int childIndex = 0; childIndex < node.getChildCount(); childIndex++) {
            AccessibilityNodeInfo child = childAtOrNull(node, childIndex);
            if (child == null) continue;
            try {
                if (appendCachedNodeQueryAtPoint(
                    child,
                    tag,
                    expectedBounds,
                    semanticPath + "/" + childIndex,
                    output,
                    includeDescendantText
                )) {
                    return true;
                }
            } finally {
                child.recycle();
            }
        }
        return false;
    }

    private JSONArray renderExactProjection(
        String tag,
        boolean includeDescendantText,
        boolean indexedOnly,
        boolean boundedOnly,
        boolean providerOnly,
        boolean renderedOnly,
        boolean verifyReachable,
        boolean avoidNavigation
    ) {
        // Compose publishes indexed control geometry explicitly. Reading that
        // channel must not block behind an accessibility-tree traversal; the
        // subsequent physical touch receipt proves that actions reached the
        // rendered control. Unknown controls still use accessibility below.
        ProviderProjection providerProjection = renderedOnly
            ? ProviderProjection.unhandled()
            : providerProjection(tag, verifyReachable, avoidNavigation);
        if (providerProjection.handled) return providerProjection.values;
        JSONArray output = new JSONArray();
        if (tag.isEmpty() || providerOnly) return output;
        List<AccessibilityNodeInfo> roots = targetRoots(true);
        try {
            for (AccessibilityNodeInfo root : roots) {
                List<AccessibilityNodeInfo> indexed = root.findAccessibilityNodeInfosByViewId(tag);
                if (indexed == null) continue;
                try {
                    for (AccessibilityNodeInfo match : indexed) {
                        appendExactProjectionValue(
                            output,
                            match,
                            "indexed",
                            includeDescendantText
                        );
                    }
                } finally {
                    recycleAll(indexed);
                }
            }
            if (output.length() > 0 || indexedOnly) return output;
        } catch (JSONException error) {
            throw new IllegalStateException("failed to encode indexed semantic projection", error);
        } finally {
            recycleAll(roots);
        }
        String cachedPath = exactNodePaths.get(tag);
        if (cachedPath != null) {
            AccessibilityNodeInfo cached = nodeAtPath(cachedPath);
            if (cached != null) {
                try {
                    cached.refresh();
                    if (tag.equals(cached.getViewIdResourceName())) {
                        appendExactProjectionValue(
                            output,
                            cached,
                            cachedPath,
                            includeDescendantText
                        );
                        return output;
                    }
                } catch (JSONException error) {
                    throw new IllegalStateException("failed to encode exact semantic projection", error);
                } finally {
                    cached.recycle();
                }
            }
            Rect cachedBounds = exactNodeBounds.get(tag);
            if (cachedBounds != null && appendExactProjectionAtPoint(
                tag,
                cachedBounds,
                output,
                includeDescendantText
            )) {
                return output;
            }
            exactNodePaths.remove(tag, cachedPath);
            exactNodeBounds.remove(tag);
        }
        if (boundedOnly) return output;
        roots = targetRoots(true);
        try {
            boolean found = appendFirstExactProjectionBreadthFirst(
                roots,
                tag,
                output,
                includeDescendantText
            );
            if (!found) {
                long deadlineNanos = System.nanoTime() + (EXACT_PROJECTION_TIME_LIMIT_NANOS / 2);
                int[] visited = {0};
                for (int rootIndex = 0; rootIndex < roots.size(); rootIndex++) {
                    if (appendFirstExactProjectionDepthFirst(
                        roots.get(rootIndex),
                        tag,
                        Integer.toString(rootIndex),
                        output,
                        includeDescendantText,
                        deadlineNanos,
                        visited
                    )) {
                        break;
                    }
                }
            }
        } catch (JSONException error) {
            throw new IllegalStateException("failed to encode exact semantic projection", error);
        } finally {
            recycleAll(roots);
        }
        return output;
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
        if (!snapshot.handled) return ProviderProjection.unhandled();
        JSONArray output = new JSONArray();
        if (!snapshot.present) return new ProviderProjection(true, output);
        try {
            Map<String, String> fields = projectionStateFields(snapshot.state);
            boolean hasBounds = snapshot.bounds != null && !snapshot.bounds.isEmpty();
            Rect parsedBounds = hasBounds ? parseBounds(snapshot.bounds) : null;
            JSONObject value = new JSONObject();
            value.put("resource-id", snapshot.resourceId);
            value.put("semantic-path", "projection-provider:" + snapshot.revision);
            value.put("text", Uri.decode(fields.getOrDefault("text", "")));
            value.put("enabled", fields.getOrDefault("enabled", "true"));
            value.put("visible", fields.getOrDefault("visible", "true"));
            value.put("selected", fields.getOrDefault("selected", "false"));
            value.put("checked", fields.getOrDefault("checked", "false"));
            value.put("focused", fields.getOrDefault("focused", "false"));
            value.put("state-description", snapshot.state);
            value.put("bounds", hasBounds ? snapshot.bounds : "[0,0][1,1]");
            value.put(
                "center-reachable",
                Boolean.toString(
                    parsedBounds != null &&
                    "true".equals(fields.getOrDefault("window-focus", "false")) &&
                    (!verifyCenterReachable || projectedCenterReachable(parsedBounds)) &&
                    (!avoidNavigation || projectedCenterClearOfNavigation(tag, parsedBounds))
                )
            );
            output.put(value);
            return new ProviderProjection(true, output);
        } catch (JSONException error) {
            return ProviderProjection.unhandled();
        }
    }

    private ProviderSnapshot providerSnapshot(String tag) {
        Uri uri = Uri.parse("content://org.aerobag.app.e2e-projections/projection")
            .buildUpon()
            .appendQueryParameter("resource_id", tag)
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
            if (cursor == null || !cursor.moveToFirst()) {
                return ProviderSnapshot.unhandled();
            }
            return new ProviderSnapshot(
                true,
                cursor.getInt(cursor.getColumnIndexOrThrow("present")) != 0,
                cursor.getString(cursor.getColumnIndexOrThrow("resource_id")),
                cursor.getString(cursor.getColumnIndexOrThrow("state")),
                cursor.getString(cursor.getColumnIndexOrThrow("bounds")),
                cursor.getLong(cursor.getColumnIndexOrThrow("revision"))
            );
        } catch (OperationCanceledException error) {
            Log.w(LOG_TAG, "projection provider timed out for " + tag);
            // The provider owns this semantic namespace. Do not turn a bounded
            // IPC miss into an expensive accessibility-tree fallback.
            return ProviderSnapshot.handledAbsent();
        } catch (IllegalArgumentException | SecurityException error) {
            return ProviderSnapshot.unhandled();
        } finally {
            cancellation.cancel(false);
            long elapsedMs = TimeUnit.NANOSECONDS.toMillis(System.nanoTime() - startedNanos);
            if (elapsedMs >= SLOW_PROVIDER_QUERY_MS) {
                Log.w(LOG_TAG, "slow projection provider query tag=" + tag + " elapsed_ms=" + elapsedMs);
            }
        }
    }

    private boolean projectedCenterReachable(Rect bounds) {
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
        return snapshot.handled && snapshot.present ? parseBounds(snapshot.bounds) : null;
    }

    private boolean replaceIndexedFocusedText(
        String tag,
        String value
    ) {
        ProviderProjection projection = providerProjection(tag);
        if (!projection.handled || projection.values.length() != 1) return false;
        try {
            JSONObject current = projection.values.getJSONObject(0);
            if (!"true".equals(current.optString("enabled", "false")) ||
                !"true".equals(current.optString("focused", "false")) ||
                !"true".equals(current.optString("center-reachable", "false"))) {
                return false;
            }
            return SemanticDriverInputMethodService.replaceFocusedText(value);
        } catch (JSONException error) {
            return false;
        }
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

    private static final class ProviderSnapshot {
        final boolean handled;
        final boolean present;
        final String resourceId;
        final String state;
        final String bounds;
        final long revision;

        ProviderSnapshot(
            boolean handled,
            boolean present,
            String resourceId,
            String state,
            String bounds,
            long revision
        ) {
            this.handled = handled;
            this.present = present;
            this.resourceId = resourceId;
            this.state = state;
            this.bounds = bounds;
            this.revision = revision;
        }

        static ProviderSnapshot unhandled() {
            return new ProviderSnapshot(false, false, "", "", null, 0);
        }

        static ProviderSnapshot handledAbsent() {
            return new ProviderSnapshot(true, false, "", "", null, 0);
        }
    }

    private static final class ProviderProjection {
        final boolean handled;
        final JSONArray values;

        ProviderProjection(boolean handled, JSONArray values) {
            this.handled = handled;
            this.values = values;
        }

        static ProviderProjection unhandled() {
            return new ProviderProjection(false, new JSONArray());
        }
    }

    @SuppressWarnings("deprecation")
    private boolean appendFirstExactProjectionBreadthFirst(
        List<AccessibilityNodeInfo> roots,
        String tag,
        JSONArray output,
        boolean includeDescendantText
    ) throws JSONException {
        ArrayDeque<PathNode> pending = new ArrayDeque<>();
        for (int rootIndex = 0; rootIndex < roots.size(); rootIndex++) {
            pending.addLast(new PathNode(
                AccessibilityNodeInfo.obtain(roots.get(rootIndex)),
                Integer.toString(rootIndex)
            ));
        }
        long deadlineNanos = System.nanoTime() + (EXACT_PROJECTION_TIME_LIMIT_NANOS / 2);
        int visited = 0;
        try {
            while (!pending.isEmpty() && visited < EXACT_PROJECTION_NODE_LIMIT / 2 &&
                System.nanoTime() < deadlineNanos) {
                PathNode current = pending.removeFirst();
                try {
                    AccessibilityNodeInfo node = current.node;
                    visited += 1;
                    node.refresh();
                    if (tag.equals(node.getViewIdResourceName())) {
                        appendExactProjectionValue(output, node, current.semanticPath, includeDescendantText);
                        exactNodePaths.put(tag, current.semanticPath);
                        Rect bounds = new Rect();
                        node.getBoundsInScreen(bounds);
                        exactNodeBounds.put(tag, bounds);
                        return true;
                    }
                    for (int childIndex = 0; childIndex < node.getChildCount(); childIndex++) {
                        AccessibilityNodeInfo child = childAtOrNull(node, childIndex);
                        if (child == null) continue;
                        pending.addLast(new PathNode(
                            child,
                            current.semanticPath + "/" + childIndex
                        ));
                    }
                } finally {
                    current.node.recycle();
                }
            }
            return false;
        } finally {
            recyclePathNodes(pending);
        }
    }

    @SuppressWarnings("deprecation")
    private boolean appendFirstExactProjectionDepthFirst(
        AccessibilityNodeInfo node,
        String tag,
        String semanticPath,
        JSONArray output,
        boolean includeDescendantText,
        long deadlineNanos,
        int[] visited
    ) throws JSONException {
        if (visited[0] >= EXACT_PROJECTION_NODE_LIMIT / 2 ||
            System.nanoTime() >= deadlineNanos) return false;
        visited[0] += 1;
        node.refresh();
        if (tag.equals(node.getViewIdResourceName())) {
            appendExactProjectionValue(output, node, semanticPath, includeDescendantText);
            exactNodePaths.put(tag, semanticPath);
            Rect bounds = new Rect();
            node.getBoundsInScreen(bounds);
            exactNodeBounds.put(tag, bounds);
            return true;
        }
        for (int childIndex = 0; childIndex < node.getChildCount(); childIndex++) {
            AccessibilityNodeInfo child = childAtOrNull(node, childIndex);
            if (child == null) continue;
            try {
                if (appendFirstExactProjectionDepthFirst(
                    child,
                    tag,
                    semanticPath + "/" + childIndex,
                    output,
                    includeDescendantText,
                    deadlineNanos,
                    visited
                )) {
                    return true;
                }
            } finally {
                child.recycle();
            }
        }
        return false;
    }

    private static void recyclePathNodes(ArrayDeque<PathNode> nodes) {
        while (!nodes.isEmpty()) nodes.removeFirst().node.recycle();
    }

    private static final class PathNode {
        final AccessibilityNodeInfo node;
        final String semanticPath;

        PathNode(AccessibilityNodeInfo node, String semanticPath) {
            this.node = node;
            this.semanticPath = semanticPath;
        }
    }

    private boolean appendExactProjectionAtPoint(
        String tag,
        Rect expectedBounds,
        JSONArray output,
        boolean includeDescendantText
    ) {
        List<AccessibilityNodeInfo> roots = targetRoots(true);
        try {
            for (int rootIndex = 0; rootIndex < roots.size(); rootIndex++) {
                if (appendExactProjectionAtPoint(
                    roots.get(rootIndex),
                    tag,
                    expectedBounds,
                    Integer.toString(rootIndex),
                    output,
                    includeDescendantText
                )) {
                    return true;
                }
            }
            return false;
        } catch (JSONException error) {
            throw new IllegalStateException("failed to encode exact semantic projection", error);
        } finally {
            recycleAll(roots);
        }
    }

    @SuppressWarnings("deprecation")
    private boolean appendExactProjectionAtPoint(
        AccessibilityNodeInfo node,
        String tag,
        Rect expectedBounds,
        String semanticPath,
        JSONArray output,
        boolean includeDescendantText
    ) throws JSONException {
        node.refresh();
        Rect bounds = new Rect();
        node.getBoundsInScreen(bounds);
        if (!bounds.contains(expectedBounds.centerX(), expectedBounds.centerY())) return false;
        if (tag.equals(node.getViewIdResourceName())) {
            appendExactProjectionValue(output, node, semanticPath, includeDescendantText);
            exactNodePaths.put(tag, semanticPath);
            exactNodeBounds.put(tag, new Rect(bounds));
            return true;
        }
        for (int childIndex = 0; childIndex < node.getChildCount(); childIndex++) {
            AccessibilityNodeInfo child = childAtOrNull(node, childIndex);
            if (child == null) continue;
            try {
                if (appendExactProjectionAtPoint(
                    child,
                    tag,
                    expectedBounds,
                    semanticPath + "/" + childIndex,
                    output,
                    includeDescendantText
                )) {
                    return true;
                }
            } finally {
                child.recycle();
            }
        }
        return false;
    }

    @SuppressWarnings("deprecation")
    private void appendExactProjectionValue(
        JSONArray output,
        AccessibilityNodeInfo node,
        String semanticPath,
        boolean includeDescendantText
    ) throws JSONException {
        node.refresh();
        Rect bounds = new Rect();
        node.getBoundsInScreen(bounds);
        JSONObject value = new JSONObject();
        value.put("resource-id", node.getViewIdResourceName());
        value.put("semantic-path", semanticPath);
        value.put("text", includeDescendantText ? nodeLabel(node) : directNodeLabel(node));
        value.put("enabled", Boolean.toString(node.isEnabled()));
        value.put("visible", Boolean.toString(node.isVisibleToUser()));
        value.put("selected", Boolean.toString(node.isSelected()));
        value.put("checked", Boolean.toString(node.isChecked()));
        value.put("focused", Boolean.toString(node.isFocused()));
        value.put("state-description", stringValue(node.getStateDescription()));
        value.put("bounds", bounds.toShortString());
        value.put("center-reachable", Boolean.toString(centerReachable(node)));
        output.put(value);
    }

    private static String directNodeLabel(AccessibilityNodeInfo node) {
        String text = stringValue(node.getText());
        String description = stringValue(node.getContentDescription());
        return (text + " " + description).trim().replaceAll("\\s+", " ");
    }

    @SuppressWarnings("deprecation")
    private boolean collectMatchingNodes(
        AccessibilityNodeInfo node,
        String tag,
        boolean prefix,
        boolean first,
        JSONArray output,
        Rect ancestorClip,
        String semanticPath,
        boolean includeDescendantText
    ) throws JSONException {
        // Compose can retain an accessibility node whose only changing field is
        // its test tag. Refresh every queried node so transition predicates see
        // the current semantic projection rather than a cached identifier.
        node.refresh();
        Rect bounds = new Rect();
        node.getBoundsInScreen(bounds);
        boolean centerReachable = node.isVisibleToUser() && !bounds.isEmpty() &&
            (ancestorClip == null || ancestorClip.contains(bounds.centerX(), bounds.centerY()));
        String nodeTag = node.getViewIdResourceName();
        boolean matched = false;
        if (nodeTag != null && (prefix ? nodeTag.startsWith(tag) : nodeTag.equals(tag))) {
            appendNodeQueryValue(
                output,
                node,
                centerReachable,
                semanticPath,
                includeDescendantText
            );
            if (!prefix) {
                exactNodePaths.put(tag, semanticPath);
                exactNodeBounds.put(tag, new Rect(bounds));
            }
            matched = true;
            if (!prefix || first) return true;
        }
        Rect childClip = new Rect(bounds);
        if (ancestorClip != null && !childClip.intersect(ancestorClip)) childClip.setEmpty();
        for (int childIndex = 0; childIndex < node.getChildCount(); childIndex++) {
            AccessibilityNodeInfo child = childAtOrNull(node, childIndex);
            if (child == null) continue;
            try {
                boolean childMatched = collectMatchingNodes(
                    child,
                    tag,
                    prefix,
                    first,
                    output,
                    childClip,
                    semanticPath + "/" + childIndex,
                    includeDescendantText
                );
                matched = matched || childMatched;
                if (childMatched && (!prefix || first)) return true;
            } finally {
                child.recycle();
            }
        }
        return matched;
    }

    private static void appendNodeQueryValue(
        JSONArray output,
        AccessibilityNodeInfo node,
        boolean centerReachable,
        String semanticPath,
        boolean includeDescendantText
    ) throws JSONException {
        node.refresh();
        Rect bounds = new Rect();
        node.getBoundsInScreen(bounds);
        JSONObject value = new JSONObject();
        value.put("resource-id", node.getViewIdResourceName());
        value.put("semantic-path", semanticPath);
        value.put("text", includeDescendantText ? nodeLabel(node) : directNodeLabel(node));
        value.put("enabled", Boolean.toString(node.isEnabled()));
        value.put("clickable", Boolean.toString(node.isClickable()));
        value.put("visible", Boolean.toString(node.isVisibleToUser()));
        value.put("center-reachable", Boolean.toString(centerReachable));
        value.put("selected", Boolean.toString(node.isSelected()));
        value.put("checked", Boolean.toString(node.isChecked()));
        value.put("checkable", Boolean.toString(node.isCheckable()));
        value.put("focused", Boolean.toString(node.isFocused()));
        value.put("scrollable", Boolean.toString(node.isScrollable()));
        value.put("state-description", stringValue(node.getStateDescription()));
        value.put("bounds", bounds.toShortString());
        output.put(value);
    }

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
    private static String nodeLabel(AccessibilityNodeInfo node) {
        StringBuilder label = new StringBuilder();
        appendLabel(label, node);
        return label.toString().trim().replaceAll("\\s+", " ");
    }

    @SuppressWarnings("deprecation")
    private static void appendLabel(StringBuilder output, AccessibilityNodeInfo node) {
        String text = stringValue(node.getText());
        String description = stringValue(node.getContentDescription());
        if (!text.isEmpty()) output.append(text).append(' ');
        if (!description.isEmpty()) output.append(description).append(' ');
        for (int childIndex = 0; childIndex < node.getChildCount(); childIndex++) {
            AccessibilityNodeInfo child = childAtOrNull(node, childIndex);
            if (child == null) continue;
            try {
                child.refresh();
                appendLabel(output, child);
            } finally {
                child.recycle();
            }
        }
    }

    private static String stringValue(CharSequence value) {
        return value == null ? "" : value.toString();
    }

    private boolean setRenderedProgress(
        String tag,
        float value,
        Rect expectedBounds,
        String semanticPath
    ) {
        for (int attempt = 0; attempt < 3; attempt++) {
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
        AccessibilityNodeInfo indexed = findIndexedRenderedNode(tag, expectedBounds);
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
                    semanticPath.startsWith("projection-provider:")
                );
                if (match != null) return match;
            }
            return null;
        } finally {
            recycleAll(roots);
        }
    }

    private AccessibilityNodeInfo findIndexedRenderedNode(String tag, Rect expectedBounds) {
        List<AccessibilityNodeInfo> roots = targetRoots(true);
        try {
            for (AccessibilityNodeInfo root : roots) {
                List<AccessibilityNodeInfo> indexed = root.findAccessibilityNodeInfosByViewId(tag);
                if (indexed == null) continue;
                try {
                    for (AccessibilityNodeInfo match : indexed) {
                        match.refresh();
                        if (matchesRenderedTarget(match, tag, expectedBounds)) {
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
        if (!bounds.contains(expectedBounds.centerX(), expectedBounds.centerY())) return null;
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

    private boolean scrollRenderedNode(Rect bounds, int action) {
        List<AccessibilityNodeInfo> roots = targetRoots(true);
        try {
            for (AccessibilityNodeInfo root : roots) {
                if (scrollNode(root, bounds, action)) return true;
            }
            return false;
        } finally {
            recycleAll(roots);
        }
    }

    @SuppressWarnings("deprecation")
    private boolean scrollFirstRenderedSurface(String orientation, int action) {
        if (!"vertical".equals(orientation) && !"horizontal".equals(orientation)) return false;
        List<AccessibilityNodeInfo> roots = targetRoots(true);
        ArrayDeque<AccessibilityNodeInfo> pending = new ArrayDeque<>();
        try {
            for (AccessibilityNodeInfo root : roots) {
                pending.addLast(AccessibilityNodeInfo.obtain(root));
            }
            long deadlineNanos = System.nanoTime() + EXACT_PROJECTION_TIME_LIMIT_NANOS;
            int visited = 0;
            while (!pending.isEmpty() && visited < EXACT_PROJECTION_NODE_LIMIT &&
                System.nanoTime() < deadlineNanos) {
                AccessibilityNodeInfo node = pending.removeFirst();
                try {
                    visited += 1;
                    node.refresh();
                    Rect bounds = new Rect();
                    node.getBoundsInScreen(bounds);
                    boolean matchesOrientation = "vertical".equals(orientation)
                        ? bounds.height() >= bounds.width()
                        : bounds.width() > bounds.height();
                    if (node.isScrollable() && matchesOrientation && node.performAction(action)) {
                        return true;
                    }
                    for (int childIndex = 0; childIndex < node.getChildCount(); childIndex++) {
                        AccessibilityNodeInfo child = childAtOrNull(node, childIndex);
                        if (child != null) pending.addLast(child);
                    }
                } finally {
                    node.recycle();
                }
            }
            return false;
        } finally {
            recycleAll(new ArrayList<>(pending));
            recycleAll(roots);
        }
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
                        stringValue(root.getPackageName())
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
                        stringValue(activeRoot.getPackageName())
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
        if (semanticPath.startsWith("projection-provider:")) {
            // Current app-owned geometry rejects stale actions. The tagged
            // physical touch receipt proves that the rendered control, rather
            // than merely these coordinates, received the one emitted tap.
            ProviderProjection projection = providerProjection(tag, true);
            if (!projection.handled || projection.values.length() != 1) return null;
            try {
                JSONObject value = projection.values.getJSONObject(0);
                Rect currentBounds = parseBounds(value.optString("bounds", ""));
                if (!semanticPath.equals(value.optString("semantic-path", "")) ||
                    currentBounds == null || !expectedBounds.equals(currentBounds) ||
                    !"true".equals(value.optString("enabled", "false")) ||
                    !"true".equals(value.optString("visible", "false")) ||
                    !"true".equals(value.optString("center-reachable", "false"))) {
                    return null;
                }
                return currentBounds;
            } catch (JSONException error) {
                return null;
            }
        }
        AccessibilityNodeInfo node = resolveRenderedNode(tag, expectedBounds, semanticPath);
        if (node == null) return null;
        Rect renderedBounds = new Rect();
        try {
            node.refresh();
            node.getBoundsInScreen(renderedBounds);
            if (!node.isVisibleToUser() || !node.isEnabled() || renderedBounds.isEmpty() ||
                !centerReachable(node)) {
                return null;
            }
        } finally {
            node.recycle();
        }
        return renderedBounds;
    }

    @SuppressWarnings("deprecation")
    private static boolean scrollNode(AccessibilityNodeInfo node, Rect bounds, int action) {
        Rect nodeBounds = new Rect();
        node.getBoundsInScreen(nodeBounds);
        if (bounds.equals(nodeBounds) && node.isVisibleToUser() && node.isScrollable()) {
            return node.performAction(action);
        }
        for (int childIndex = 0; childIndex < node.getChildCount(); childIndex++) {
            AccessibilityNodeInfo child = childAtOrNull(node, childIndex);
            if (child == null) continue;
            try {
                if (scrollNode(child, bounds, action)) return true;
            } finally {
                child.recycle();
            }
        }
        return false;
    }

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
