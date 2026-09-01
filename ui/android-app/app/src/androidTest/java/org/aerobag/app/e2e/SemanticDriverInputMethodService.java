// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app.e2e;

import android.inputmethodservice.InputMethodService;
import android.view.inputmethod.ExtractedText;
import android.view.inputmethod.ExtractedTextRequest;
import android.view.inputmethod.InputConnection;

/** Test-only IME used to deliver text through the same focused editor as a keyboard. */
public final class SemanticDriverInputMethodService extends InputMethodService {
    private static volatile SemanticDriverInputMethodService activeService;

    @Override
    public void onCreate() {
        super.onCreate();
        activeService = this;
    }

    @Override
    public void onDestroy() {
        if (activeService == this) activeService = null;
        super.onDestroy();
    }

    static boolean focusedInputConnectionReady() {
        SemanticDriverInputMethodService service = activeService;
        if (service == null) return false;
        InputConnection connection = service.getCurrentInputConnection();
        if (connection == null) return false;
        ExtractedText current = connection.getExtractedText(new ExtractedTextRequest(), 0);
        return current != null && current.text != null;
    }

    static boolean replaceFocusedText(String value) {
        SemanticDriverInputMethodService service = activeService;
        if (service == null) return false;
        InputConnection connection = service.getCurrentInputConnection();
        if (connection == null) return false;
        ExtractedText current = connection.getExtractedText(new ExtractedTextRequest(), 0);
        if (current == null || current.text == null) return false;

        connection.beginBatchEdit();
        try {
            connection.finishComposingText();
            int start = current.startOffset;
            if (!connection.setSelection(start, start + current.text.length())) return false;
            return connection.commitText(value, 1);
        } finally {
            connection.endBatchEdit();
        }
    }
}
