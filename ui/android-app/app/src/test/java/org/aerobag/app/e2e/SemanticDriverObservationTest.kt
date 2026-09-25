// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

package org.aerobag.app.e2e

import org.aerobag.app.E2eProjectionProvider
import org.aerobag.app.E2eProjectionRegistry
import android.accessibilityservice.AccessibilityService
import android.accessibilityservice.GestureDescription
import android.graphics.Rect
import android.os.Handler
import android.view.accessibility.AccessibilityWindowInfo
import org.json.JSONArray
import org.junit.After
import org.junit.Assert.*
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Robolectric
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.Implementation
import org.robolectric.annotation.Implements
import org.robolectric.shadows.ShadowContentResolver
import org.robolectric.util.ReflectionHelpers
import org.robolectric.util.ReflectionHelpers.ClassParameter
import java.util.concurrent.Executors

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [34], shadows = [SemanticDriverObservationTest.ImeProbe::class, SemanticDriverObservationTest.GestureProbe::class])
class SemanticDriverObservationTest {
    private val owner = Any()
    private val tags = mutableListOf<String>()
    private val timer = Executors.newSingleThreadScheduledExecutor()
    private lateinit var driver: SemanticDriverService

    @Before fun setUp() {
        ImeProbe.reads = 0
        ImeProbe.ready = true
        GestureProbe.gestures.clear()
        driver = Robolectric.buildService(SemanticDriverService::class.java).create().get()
        ReflectionHelpers.setField(driver, "providerQueryTimeoutExecutor", timer)
        val provider = Robolectric.buildContentProvider(E2eProjectionProvider::class.java).create().get()
        ShadowContentResolver.registerProviderInternal("org.aerobag.app.e2e-projections", provider)
    }

    @After fun tearDown() {
        tags.forEach { E2eProjectionRegistry.remove(it, owner) }
        timer.shutdownNow()
    }

    private fun publish(tag: String, state: String) {
        tags.add(tag)
        E2eProjectionRegistry.publish(tag, state, owner, "[20,20][60,60]")
    }

    private fun read(tag: String, prefix: Boolean = false): JSONArray {
        val projection = if (prefix) {
            ReflectionHelpers.callInstanceMethod<Any>(driver, "providerProjectionPrefix",
                ClassParameter.from(String::class.java, tag))
        } else {
            ReflectionHelpers.callInstanceMethod<Any>(driver, "providerProjection",
                ClassParameter.from(String::class.java, tag),
                ClassParameter.from(Boolean::class.javaPrimitiveType, false))
        }
        return ReflectionHelpers.getField(projection, "values")
    }

    @Test fun wholeIndexReadNeverContactsTheAppThroughItsInputConnection() {
        repeat(100) { publish("parity:test-map-$it", "enabled:true:window-focus:true") }
        // Even if search retains focus, map hit-testing only needs published geometry/state.
        publish("parity:test-search", "kind:text:enabled:true:focused:true:window-focus:true")
        assertEquals(101, read("parity:test-", prefix = true).length())
        assertEquals("a batch must be independent of main-thread IME responses", 0, ImeProbe.reads)
    }

    @Test fun exactNonTextAndInactiveEditorsNeverProbeTheInputConnection() {
        for ((index, state) in listOf(
            "enabled:true:window-focus:true",
            "kind:text:enabled:true:focused:false:window-focus:true",
            "kind:text:enabled:false:focused:true:window-focus:true",
            "kind:text:enabled:true:focused:true:window-focus:false",
        ).withIndex()) {
            val tag = "parity:test-$index"
            publish(tag, state)
            assertFalse(read(tag).getJSONObject(0).getBoolean("input-connection-ready"))
        }
        assertEquals(0, ImeProbe.reads)
    }

    @Test fun exactFocusedEditorStillRequiresAWorkingInputConnection() {
        publish("parity:test-search", "kind:text:enabled:true:focused:true:window-focus:true")
        ImeProbe.ready = false
        assertFalse(read("parity:test-search").getJSONObject(0).getBoolean("input-connection-ready"))
        ImeProbe.ready = true
        assertTrue(read("parity:test-search").getJSONObject(0).getBoolean("input-connection-ready"))
        assertEquals(2, ImeProbe.reads)
    }

    @Test fun absentControlsDoNotProbeTheInputConnection() {
        assertEquals(0, read("parity:test-absent").length())
        assertEquals(0, ImeProbe.reads)
    }

    private fun progress(tag: String, value: Float, evidence: String): Boolean =
        ReflectionHelpers.callInstanceMethod(driver, "setRenderedProgress",
            ClassParameter.from(String::class.java, tag),
            ClassParameter.from(Float::class.javaPrimitiveType, value),
            ClassParameter.from(Rect::class.java, Rect(20, 20, 60, 60)),
            ClassParameter.from(String::class.java, evidence))

    @Test fun progressUsesOnePhysicalGestureWithoutReadingTheAccessibilityTree() {
        val tag = "parity:test-slider"
        publish(tag, "kind:horizontal-progress:min:0.25:max:11.0:enabled:true:window-focus:true")
        val evidence = read(tag).getJSONObject(0).getString("semantic-path")
        assertTrue(progress(tag, 5.625f, evidence))
        assertEquals(1, GestureProbe.gestures.size)
        val bounds = android.graphics.RectF()
        GestureProbe.gestures.single().getStroke(0).path.computeBounds(bounds, true)
        assertEquals(40f, bounds.left, 0.01f)
        assertEquals(40f, bounds.top, 0.01f)
        assertEquals(0, ImeProbe.reads)
    }

    @Test fun progressRejectsDisabledChangedAndUnmountedControlsWithoutInput() {
        val tag = "parity:test-slider"
        val state = "kind:horizontal-progress:min:0:max:10:enabled:true:window-focus:true"
        publish(tag, state)
        val evidence = read(tag).getJSONObject(0).getString("semantic-path")
        assertFalse(progress(tag, -1f, evidence))
        assertFalse(progress(tag, Float.NaN, evidence))
        publish(tag, state.replace("enabled:true", "enabled:false"))
        assertFalse(progress(tag, 5f, evidence))
        E2eProjectionRegistry.remove(tag, owner)
        assertFalse(progress(tag, 5f, evidence))
        publish(tag, state)
        assertFalse(progress(tag, 5f, evidence))
        assertTrue(GestureProbe.gestures.isEmpty())
    }

    @Implements(AccessibilityService::class)
    class GestureProbe {
        companion object { val gestures = mutableListOf<GestureDescription>() }
        @Implementation fun getWindows(): List<AccessibilityWindowInfo> =
            error("Progress input must not wait for an accessibility-tree round trip")
        @Implementation fun dispatchGesture(gesture: GestureDescription,
            callback: AccessibilityService.GestureResultCallback?, handler: Handler?): Boolean {
            gestures.add(gesture)
            callback?.onCompleted(gesture)
            return true
        }
    }

    @Implements(SemanticDriverInputMethodService::class)
    class ImeProbe {
        companion object {
            var reads = 0
            var ready = true
            @JvmStatic @Implementation fun focusedInputConnectionReady(): Boolean {
                reads++
                return ready
            }
        }
    }
}
