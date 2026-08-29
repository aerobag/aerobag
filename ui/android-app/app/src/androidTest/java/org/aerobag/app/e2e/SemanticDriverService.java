// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app.e2e;

import android.accessibilityservice.AccessibilityService;
import android.graphics.Rect;
import android.os.Bundle;
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
import java.util.concurrent.atomic.AtomicBoolean;
import java.util.concurrent.atomic.AtomicLong;
import java.util.concurrent.TimeUnit;
import org.json.JSONArray;
import org.json.JSONException;
import org.json.JSONObject;

/** Test-only semantic driver that remains independent of Aerobag's process lifecycle. */
public final class SemanticDriverService extends AccessibilityService {
    private static final int DRIVER_PORT = 19_191;
    private static final String DRIVER_PROTOCOL = "aerobag-semantic-driver/2";
    private final AtomicBoolean running = new AtomicBoolean(false);
    private final AtomicLong accessibilityEventSequence = new AtomicLong();
    private final Object accessibilityEventMonitor = new Object();
    private ServerSocket server;
    private Thread serverThread;

    @Override
    protected void onServiceConnected() {
        if (!running.compareAndSet(false, true)) return;
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
                try (Socket client = socket.accept()) {
                    handleRequest(client);
                }
            }
        } catch (IOException error) {
            if (running.get()) throw new RuntimeException(error);
        } finally {
            server = null;
            running.set(false);
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
        switch (endpoint) {
            case "/health":
                respond(
                    socket.getOutputStream(),
                    "text/plain; charset=utf-8",
                    DRIVER_PROTOCOL + "\n",
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
            case "/await-event":
                handleAwaitEvent(socket, path);
                return;
            case "/set-text":
                handleSetText(socket, path);
                return;
            case "/click":
                handleClick(socket, path);
                return;
            case "/scroll":
                handleScroll(socket, path);
                return;
            default:
                respond(socket.getOutputStream(), "text/plain; charset=utf-8", "not found\n", 404);
        }
    }

    private void handleSetText(Socket socket, String path) throws IOException {
        Map<String, String> query = queryOf(path);
        String tag = query.getOrDefault("tag", "");
        String value = query.getOrDefault("value", "");
        Rect expectedBounds = parseBounds(query.getOrDefault("bounds", ""));
        boolean changed = !tag.isEmpty() && expectedBounds != null &&
            setRenderedText(tag, value, expectedBounds);
        respondAction(socket, changed, "text action rejected\n");
    }

    private void handleQuery(Socket socket, String path) throws IOException {
        Map<String, String> query = queryOf(path);
        String tag = query.getOrDefault("tag", "");
        boolean prefix = "true".equals(query.getOrDefault("prefix", "false"));
        respond(
            socket.getOutputStream(),
            "application/json; charset=utf-8",
            renderNodeQuery(tag, prefix).toString() + "\n",
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

    private void handleClick(Socket socket, String path) throws IOException {
        Map<String, String> query = queryOf(path);
        String tag = query.getOrDefault("tag", "");
        Rect expectedBounds = parseBounds(query.getOrDefault("bounds", ""));
        boolean clicked = !tag.isEmpty() && expectedBounds != null &&
            clickRenderedNode(tag, expectedBounds);
        respondAction(socket, clicked, "click action rejected for " + tag + "\n");
    }

    private void handleScroll(Socket socket, String path) throws IOException {
        Map<String, String> query = queryOf(path);
        Rect bounds = parseBounds(query.getOrDefault("bounds", ""));
        String direction = query.getOrDefault("direction", "");
        int action = "forward".equals(direction)
            ? AccessibilityNodeInfo.ACTION_SCROLL_FORWARD
            : "backward".equals(direction)
                ? AccessibilityNodeInfo.ACTION_SCROLL_BACKWARD
                : 0;
        boolean scrolled = bounds != null && action != 0 && scrollRenderedNode(bounds, action);
        respondAction(socket, scrolled, "scroll action rejected\n");
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
        String reason = status == 200 ? "OK" : status == 409 ? "Conflict" : "Not Found";
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

    private String renderHierarchy() {
        StringBuilder output = new StringBuilder();
        output.append("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n");
        output.append("<hierarchy rotation=\"0\">\n");
        List<AccessibilityNodeInfo> roots = roots(false);
        try {
            for (AccessibilityNodeInfo root : roots) appendNode(output, root, 0);
        } finally {
            recycleAll(roots);
        }
        output.append("</hierarchy>\n");
        return output.toString();
    }

    private JSONArray renderNodeQuery(String tag, boolean prefix) {
        JSONArray output = new JSONArray();
        if (tag.isEmpty()) return output;
        List<AccessibilityNodeInfo> roots = roots(true);
        try {
            for (AccessibilityNodeInfo root : roots) {
                if (prefix) {
                    collectMatchingNodes(root, tag, true, output, null);
                    continue;
                }
                List<AccessibilityNodeInfo> matches = root.findAccessibilityNodeInfosByViewId(tag);
                try {
                    for (AccessibilityNodeInfo match : matches) {
                        appendNodeQueryValue(output, match, centerReachable(match));
                    }
                } finally {
                    recycleAll(matches);
                }
            }
        } catch (JSONException error) {
            throw new IllegalStateException("failed to encode semantic query result", error);
        } finally {
            recycleAll(roots);
        }
        return output;
    }

    @SuppressWarnings("deprecation")
    private static void collectMatchingNodes(
        AccessibilityNodeInfo node,
        String tag,
        boolean prefix,
        JSONArray output,
        Rect ancestorClip
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
        if (nodeTag != null && (prefix ? nodeTag.startsWith(tag) : nodeTag.equals(tag))) {
            appendNodeQueryValue(output, node, centerReachable);
        }
        Rect childClip = new Rect(bounds);
        if (ancestorClip != null && !childClip.intersect(ancestorClip)) childClip.setEmpty();
        for (int childIndex = 0; childIndex < node.getChildCount(); childIndex++) {
            AccessibilityNodeInfo child = node.getChild(childIndex);
            if (child == null) continue;
            try {
                collectMatchingNodes(child, tag, prefix, output, childClip);
            } finally {
                child.recycle();
            }
        }
    }

    private static void appendNodeQueryValue(
        JSONArray output,
        AccessibilityNodeInfo node,
        boolean centerReachable
    ) throws JSONException {
        node.refresh();
        Rect bounds = new Rect();
        node.getBoundsInScreen(bounds);
        JSONObject value = new JSONObject();
        value.put("resource-id", node.getViewIdResourceName());
        value.put("text", nodeLabel(node));
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
    private static boolean centerReachable(AccessibilityNodeInfo node) {
        Rect bounds = new Rect();
        node.getBoundsInScreen(bounds);
        if (!node.isVisibleToUser() || bounds.isEmpty()) return false;
        int centerX = bounds.centerX();
        int centerY = bounds.centerY();
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
            AccessibilityNodeInfo child = node.getChild(childIndex);
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

    private boolean setRenderedText(String tag, String value, Rect expectedBounds) {
        List<AccessibilityNodeInfo> roots = roots(true);
        try {
            for (AccessibilityNodeInfo root : roots) {
                List<AccessibilityNodeInfo> matches = root.findAccessibilityNodeInfosByViewId(tag);
                try {
                    for (AccessibilityNodeInfo match : matches) {
                        if (setMatchingNodeText(match, value, expectedBounds)) return true;
                    }
                } finally {
                    recycleAll(matches);
                }
            }
            return false;
        } finally {
            recycleAll(roots);
        }
    }

    private boolean clickRenderedNode(String tag, Rect expectedBounds) {
        List<AccessibilityNodeInfo> roots = roots(true);
        try {
            for (AccessibilityNodeInfo root : roots) {
                List<AccessibilityNodeInfo> matches = root.findAccessibilityNodeInfosByViewId(tag);
                try {
                    for (AccessibilityNodeInfo match : matches) {
                        if (clickMatchingNode(match, expectedBounds)) return true;
                    }
                } finally {
                    recycleAll(matches);
                }
            }
            return false;
        } finally {
            recycleAll(roots);
        }
    }

    private boolean scrollRenderedNode(Rect bounds, int action) {
        List<AccessibilityNodeInfo> roots = roots(true);
        try {
            for (AccessibilityNodeInfo root : roots) {
                if (scrollNode(root, bounds, action)) return true;
            }
            return false;
        } finally {
            recycleAll(roots);
        }
    }

    private List<AccessibilityNodeInfo> roots(boolean topFirst) {
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
                    root.refresh();
                    roots.add(root);
                }
            }
            if (roots.isEmpty()) {
                AccessibilityNodeInfo activeRoot = getRootInActiveWindow();
                if (activeRoot != null) {
                    activeRoot.refresh();
                    roots.add(activeRoot);
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

    @SuppressWarnings("deprecation")
    private static boolean setMatchingNodeText(
        AccessibilityNodeInfo node,
        String value,
        Rect expectedBounds
    ) {
        node.refresh();
        Rect bounds = new Rect();
        node.getBoundsInScreen(bounds);
        if (!bounds.equals(expectedBounds)) return false;
        if (!node.isFocused()) {
            if (!node.performAction(AccessibilityNodeInfo.ACTION_FOCUS)) return false;
            if (!node.refresh()) return false;
            node.getBoundsInScreen(bounds);
            if (!bounds.equals(expectedBounds) || !node.isFocused()) return false;
        }
        Bundle arguments = new Bundle();
        arguments.putCharSequence(
            AccessibilityNodeInfo.ACTION_ARGUMENT_SET_TEXT_CHARSEQUENCE,
            value
        );
        return node.performAction(AccessibilityNodeInfo.ACTION_SET_TEXT, arguments);
    }

    @SuppressWarnings("deprecation")
    private boolean clickMatchingNode(AccessibilityNodeInfo node, Rect expectedBounds) {
        node.refresh();
        if (!node.isVisibleToUser() || !node.isEnabled() || !node.isClickable()) return false;
        Rect bounds = new Rect();
        node.getBoundsInScreen(bounds);
        if (!bounds.equals(expectedBounds) || !centerReachable(node)) return false;
        return node.performAction(AccessibilityNodeInfo.ACTION_CLICK);
    }

    @SuppressWarnings("deprecation")
    private static boolean scrollNode(AccessibilityNodeInfo node, Rect bounds, int action) {
        Rect nodeBounds = new Rect();
        node.getBoundsInScreen(nodeBounds);
        if (bounds.equals(nodeBounds) && node.isVisibleToUser() && node.isScrollable()) {
            return node.performAction(action);
        }
        for (int childIndex = 0; childIndex < node.getChildCount(); childIndex++) {
            AccessibilityNodeInfo child = node.getChild(childIndex);
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
    private static void appendNode(StringBuilder output, AccessibilityNodeInfo node, int index) {
        appendNode(output, node, index, false);
    }

    @SuppressWarnings("deprecation")
    private static void appendNode(
        StringBuilder output,
        AccessibilityNodeInfo node,
        int index,
        boolean refreshSubtree
    ) {
        String viewId = string(node.getViewIdResourceName());
        boolean refreshThisSubtree = refreshSubtree || viewId.startsWith("parity:");
        if (refreshThisSubtree) node.refresh();
        Rect bounds = new Rect();
        node.getBoundsInScreen(bounds);
        output.append("<node");
        attribute(output, "index", Integer.toString(index));
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
            AccessibilityNodeInfo child = node.getChild(childIndex);
            if (child == null) continue;
            try {
                appendNode(output, child, childIndex, refreshThisSubtree);
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
