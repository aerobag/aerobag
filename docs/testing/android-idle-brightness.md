# Android idle brightness

Aerobag's idle brightness target is currently 0.05 (5% of the Android brightness
range). At the core-selected idle deadline, Android reads the primary brightness
from `Settings.System.SCREEN_BRIGHTNESS`. It applies the window override only if
the target is lower. A missing/invalid reading leaves system brightness in control.
The same check applies in manual and automatic modes.

An existing idle override remains in place across unrelated session updates.
User input, leaving the foreground, disabling dimming, or allowing the screen to
sleep clears the override. Do not restore/reapply it for every session snapshot:
that can flicker and confuse our dimmed level with the original primary level.

## Regression coverage

Run `./ui/android-app/scripts/test.sh`. `DisplayInactivityPolicyTest` covers a
brighter display, a display already below the target, equality, and invalid
readings. The original unconditional override failed the below-target case.

On a physical device, use its existing dim timeout and compare these commands
before and after that deadline (replace `$ANDROID_SERIAL` with the ADB serial):

```sh
adb -s "$ANDROID_SERIAL" shell settings get system screen_brightness_mode
adb -s "$ANDROID_SERIAL" shell settings get system screen_brightness
adb -s "$ANDROID_SERIAL" shell dumpsys display
adb -s "$ANDROID_SERIAL" shell dumpsys activity org.aerobag.app/.MainActivity
```

In `dumpsys display`, inspect `Display Brightness` and `mBrightnessReason`.
The activity dump includes the pending dim callback and its remaining delay.
An unknown key records activity without changing the flight plan:

```sh
adb -s "$ANDROID_SERIAL" shell input keyevent KEYCODE_UNKNOWN
```

Check these cases:

- Bright automatic mode: after idle, the actual backlight falls to the target;
  input restores automatic brightness.
- Manual brightness below target: idle leaves the backlight and manual control
  unchanged. Restore the original brightness setting and mode after testing.
- Covered light sensor/dark room, automatic mode: wait for the backlight to fall
  below target, then leave Aerobag idle; it must not brighten the display.
- While dimmed, session updates must not make the display alternate between its
  primary brightness and the idle target.

## Device Evidence And Limits

Samsung SM-X520, Android tablet over ADB, 2026-09-08:

- Automatic brightness: setting 42 corresponded to actual brightness 0.16470589.
- After the two-minute Aerobag timeout: actual brightness was 0.05 and the reason
  was `override(org.aerobag.app/org.aerobag.app.MainActivity)`.
- Input restored `automatic` control at approximately 0.16.
- Manual brightness set to 5/255: actual brightness was 0.019607846 before idle
  and remained 0.019607846 with reason `manual` after two minutes and 18 seconds.
  The activity dump confirmed that the dim callback had run. The original
  brightness setting and automatic mode were restored afterwards.
- Face down on the desk: the light sensor settled at 6 lux, automatic brightness
  was 0.058823533, and `SCREEN_BRIGHTNESS` agreed at 15/255. After two minutes
  idle, actual brightness fell to 0.05 with reason `override`; the activity dump
  confirmed that the dim callback had run. There was no increase, but this setup
  did not bring automatic brightness below the 5% idle target.
- Inside a closed drawer: the light sensor read 0 lux, automatic brightness was
  0.019607846, and `SCREEN_BRIGHTNESS` agreed at 5/255. Readings before and more
  than two minutes after injected activity stayed at that level with reason
  `automatic`, not `override`. This verifies the below-target automatic case on
  this tablet: Aerobag did not raise the backlight to its 5% idle target.

Android documents that automatic mode *may* update `SCREEN_BRIGHTNESS`; this is
not a portable guarantee that every OEM reports instantaneous auto-brightness.
Validate additional tablet families using actual display readings, particularly
in darkness. See [Settings.System](https://developer.android.com/reference/android/provider/Settings.System#SCREEN_BRIGHTNESS_MODE).

This is a check at idle entry. A window override replaces automatic control while
active; it is not a continuously recomputed fraction of ambient brightness.

`SCREEN_DIM_WAKE_LOCK` was investigated and removed. It permits Android's native
dim policy after the system inactivity timeout rather than forcing dimming at
Aerobag's deadline (the test tablet had a 30-minute system timeout). A black scrim
would not provide the required LCD backlight power saving.
