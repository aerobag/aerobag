// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app.e2e;

import android.inputmethodservice.InputMethodService;
import android.view.inputmethod.EditorInfo;
import android.view.inputmethod.ExtractedText;
import android.view.inputmethod.ExtractedTextRequest;
import android.view.inputmethod.InputConnection;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.atomic.AtomicBoolean;

/** Test-only IME that commits text through the same input connection as a hardware keyboard. */
public final class SemanticDriverInputMethodService extends InputMethodService {
    private static volatile SemanticDriverInputMethodService active;
    private volatile boolean focusedTextConnectionReady;

    @Override
    public void onCreate() {
        super.onCreate();
        active = this;
    }

    @Override
    public void onDestroy() {
        if (active == this) active = null;
        super.onDestroy();
    }

    @Override
    public void onStartInput(EditorInfo attribute, boolean restarting) {
        super.onStartInput(attribute, restarting);
        refreshFocusedTextConnection(attribute);
    }

    @Override
    public void onStartInputView(EditorInfo attribute, boolean restarting) {
        super.onStartInputView(attribute, restarting);
        refreshFocusedTextConnection(attribute);
    }

    @Override
    public void onFinishInput() {
        focusedTextConnectionReady = false;
        super.onFinishInput();
    }

    @Override
    public void onUnbindInput() {
        focusedTextConnectionReady = false;
        super.onUnbindInput();
    }

    static boolean replaceFocusedText(String value) {
        SemanticDriverInputMethodService service = active;
        if (service == null) return false;
        CountDownLatch completed = new CountDownLatch(1);
        AtomicBoolean committed = new AtomicBoolean(false);
        service.getMainExecutor().execute(() -> {
            InputConnection connection = service.getCurrentInputConnection();
            ExtractedText extracted = extractedText(connection);
            if (connection != null && extracted != null) {
                connection.beginBatchEdit();
                try {
                    int start = Math.max(0, extracted.startOffset);
                    int end = start + (extracted.text == null ? 0 : extracted.text.length());
                    boolean selected = connection.setSelection(start, end);
                    committed.set(selected && connection.commitText(value, 1));
                } finally {
                    connection.endBatchEdit();
                }
            }
            completed.countDown();
        });
        try {
            return completed.await(1, TimeUnit.SECONDS) && committed.get();
        } catch (InterruptedException error) {
            Thread.currentThread().interrupt();
            return false;
        }
    }

    static boolean focusedTextReady() {
        SemanticDriverInputMethodService service = active;
        return service != null && service.focusedTextConnectionReady;
    }

    private void refreshFocusedTextConnection(EditorInfo editor) {
        focusedTextConnectionReady =
            editor != null &&
                "org.aerobag.app".equals(editor.packageName) &&
                extractedText(getCurrentInputConnection()) != null;
    }

    private static ExtractedText extractedText(InputConnection connection) {
        if (connection == null) return null;
        ExtractedTextRequest request = new ExtractedTextRequest();
        request.hintMaxChars = 1_048_576;
        request.hintMaxLines = 16_384;
        return connection.getExtractedText(request, 0);
    }
}
