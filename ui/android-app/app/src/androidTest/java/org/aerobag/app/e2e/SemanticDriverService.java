// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app.e2e;

import android.accessibilityservice.AccessibilityService;
import android.accessibilityservice.GestureDescription;
import android.graphics.Rect;
import android.graphics.Path;
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
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.TimeUnit;

/** Test-only semantic driver that remains independent of Aerobag's process lifecycle. */
public final class SemanticDriverService extends AccessibilityService {
    private static final int DRIVER_PORT = 19_191;

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
                respond(socket.getOutputStream(), "text/plain; charset=utf-8", "ok\n", 200);
                return;
            case "/dump":
                respond(
                    socket.getOutputStream(),
                    "application/xml; charset=utf-8",
                    renderHierarchy(),
                    200
                );
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
        boolean changed = !tag.isEmpty() && setRenderedText(tag, value);
        respondAction(socket, changed, "text action rejected\n");
    }

    private void handleClick(Socket socket, String path) throws IOException {
        String tag = queryOf(path).getOrDefault("tag", "");
        boolean clicked = !tag.isEmpty() && clickRenderedNode(tag);
        respondAction(socket, clicked, "click action rejected\n");
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
        long eventSequence = accessibilityEventSequence.get();
        boolean scrolled = bounds != null && action != 0 && scrollRenderedNode(bounds, action);
        if (scrolled) awaitAccessibilityEventAfter(eventSequence, 1_000);
        respondAction(socket, scrolled, "scroll action rejected\n");
    }

    private void awaitAccessibilityEventAfter(long sequence, long timeoutMs) {
        long deadlineNanos = System.nanoTime() + TimeUnit.MILLISECONDS.toNanos(timeoutMs);
        synchronized (accessibilityEventMonitor) {
            while (accessibilityEventSequence.get() <= sequence) {
                long remainingNanos = deadlineNanos - System.nanoTime();
                if (remainingNanos <= 0) return;
                try {
                    TimeUnit.NANOSECONDS.timedWait(accessibilityEventMonitor, remainingNanos);
                } catch (InterruptedException error) {
                    Thread.currentThread().interrupt();
                    return;
                }
            }
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

    private boolean setRenderedText(String tag, String value) {
        List<AccessibilityNodeInfo> roots = roots(true);
        try {
            for (AccessibilityNodeInfo root : roots) {
                if (setNodeText(root, tag, value)) return true;
            }
            return false;
        } finally {
            recycleAll(roots);
        }
    }

    private boolean clickRenderedNode(String tag) {
        List<AccessibilityNodeInfo> roots = roots(true);
        try {
            for (AccessibilityNodeInfo root : roots) {
                if (clickNode(root, tag)) return true;
            }
            return false;
        } finally {
            recycleAll(roots);
        }
    }

    private boolean scrollRenderedNode(Rect bounds, int action) {
        List<AccessibilityNodeInfo> roots = roots(true);
        try {
            boolean renderedScrollableNodeExists = false;
            for (AccessibilityNodeInfo root : roots) {
                renderedScrollableNodeExists |= hasScrollableNode(root, bounds);
                if (scrollNode(root, bounds, action)) return true;
            }
            return renderedScrollableNodeExists && dispatchScrollGesture(bounds, action);
        } finally {
            recycleAll(roots);
        }
    }

    private boolean dispatchScrollGesture(Rect bounds, int action) {
        int inset = Math.min(80, bounds.height() / 5);
        int systemGestureInset = Math.max(inset, 180);
        int safeTop = Math.min(bounds.bottom - 1, bounds.top + systemGestureInset);
        int safeBottom = Math.max(safeTop + 1, bounds.bottom - systemGestureInset);
        float midpoint = (safeTop + safeBottom) / 2.0f;
        float travel = (safeBottom - safeTop) * 0.55f;
        float upperY = midpoint - travel / 2.0f;
        float lowerY = midpoint + travel / 2.0f;
        float startY = action == AccessibilityNodeInfo.ACTION_SCROLL_FORWARD ? lowerY : upperY;
        float endY = action == AccessibilityNodeInfo.ACTION_SCROLL_FORWARD ? upperY : lowerY;
        float x = (bounds.left + bounds.right) / 2.0f;
        Path path = new Path();
        path.moveTo(x, startY);
        path.lineTo(x, endY);
        GestureDescription gesture = new GestureDescription.Builder()
            .addStroke(new GestureDescription.StrokeDescription(path, 0, 80))
            .build();
        CountDownLatch completed = new CountDownLatch(1);
        boolean accepted = dispatchGesture(
            gesture,
            new GestureResultCallback() {
                @Override
                public void onCompleted(GestureDescription description) {
                    completed.countDown();
                }

                @Override
                public void onCancelled(GestureDescription description) {
                    completed.countDown();
                }
            },
            null
        );
        if (!accepted) return false;
        try {
            return completed.await(2, TimeUnit.SECONDS);
        } catch (InterruptedException error) {
            Thread.currentThread().interrupt();
            return false;
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
                if (root != null) roots.add(root);
            }
            if (roots.isEmpty()) {
                AccessibilityNodeInfo activeRoot = getRootInActiveWindow();
                if (activeRoot != null) roots.add(activeRoot);
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
    private static boolean setNodeText(AccessibilityNodeInfo node, String tag, String value) {
        if (tag.equals(node.getViewIdResourceName())) {
            Bundle arguments = new Bundle();
            arguments.putCharSequence(
                AccessibilityNodeInfo.ACTION_ARGUMENT_SET_TEXT_CHARSEQUENCE,
                value
            );
            return node.performAction(AccessibilityNodeInfo.ACTION_SET_TEXT, arguments);
        }
        for (int childIndex = 0; childIndex < node.getChildCount(); childIndex++) {
            AccessibilityNodeInfo child = node.getChild(childIndex);
            if (child == null) continue;
            try {
                if (setNodeText(child, tag, value)) return true;
            } finally {
                child.recycle();
            }
        }
        return false;
    }

    @SuppressWarnings("deprecation")
    private static boolean clickNode(AccessibilityNodeInfo node, String tag) {
        if (tag.equals(node.getViewIdResourceName())) {
            return node.isVisibleToUser()
                && node.isEnabled()
                && node.isClickable()
                && node.performAction(AccessibilityNodeInfo.ACTION_CLICK);
        }
        for (int childIndex = 0; childIndex < node.getChildCount(); childIndex++) {
            AccessibilityNodeInfo child = node.getChild(childIndex);
            if (child == null) continue;
            try {
                if (clickNode(child, tag)) return true;
            } finally {
                child.recycle();
            }
        }
        return false;
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

    @SuppressWarnings("deprecation")
    private static boolean hasScrollableNode(AccessibilityNodeInfo node, Rect bounds) {
        Rect nodeBounds = new Rect();
        node.getBoundsInScreen(nodeBounds);
        if (bounds.equals(nodeBounds) && node.isVisibleToUser() && node.isScrollable()) return true;
        for (int childIndex = 0; childIndex < node.getChildCount(); childIndex++) {
            AccessibilityNodeInfo child = node.getChild(childIndex);
            if (child == null) continue;
            try {
                if (hasScrollableNode(child, bounds)) return true;
            } finally {
                child.recycle();
            }
        }
        return false;
    }

    private static Rect parseBounds(String value) {
        String normalized = value
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
        Rect bounds = new Rect();
        node.getBoundsInScreen(bounds);
        output.append("<node");
        attribute(output, "index", Integer.toString(index));
        attribute(output, "text", string(node.getText()));
        attribute(output, "resource-id", string(node.getViewIdResourceName()));
        attribute(output, "class", string(node.getClassName()));
        attribute(output, "package", string(node.getPackageName()));
        attribute(output, "content-desc", string(node.getContentDescription()));
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
                appendNode(output, child, childIndex);
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
