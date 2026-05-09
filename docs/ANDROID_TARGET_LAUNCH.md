# Android Target Launch

The Android APK bakes its dev-server and offline package-source URLs into assets at build time.
Do not reuse an APK built for one target on another target.

## Emulator

Use the emulator wrapper when the device is an Android emulator:

```sh
ui/android-app/scripts/install_emulator_dev.sh
```

Defaults:

- `ANDROID_SERIAL=emulator-5560`
- `ANDROID_DEV_SERVER_BASE_URL=http://10.0.2.2:8083`
- `ANDROID_PACKAGE_SOURCE_BASE_URL=http://10.0.2.2:8083/packages/`

`10.0.2.2` is Android emulator magic for the host machine. It is wrong for physical devices.

## Red Tablet

Use the tablet wrapper when the device is the red physical tablet:

```sh
ui/android-app/scripts/install_red_tablet_dev.sh
```

Defaults:

- `ANDROID_SERIAL=10.110.10.232:5555`
- `ANDROID_DEV_SERVER_BASE_URL=http://10.110.44.18:8083`
- `ANDROID_PACKAGE_SOURCE_BASE_URL=http://10.110.44.18:8083/packages/`

The tablet is on the WLAN and must use the host machine's WLAN address. It cannot reach `10.0.2.2`.

## Override Points

Both wrappers accept environment overrides:

```sh
ANDROID_SERIAL=<adb-serial> \
ANDROID_DEV_SERVER_BASE_URL=http://<host>:8083 \
ANDROID_PACKAGE_SOURCE_BASE_URL=http://<host>:8083/packages/ \
ui/android-app/scripts/install_red_tablet_dev.sh
```

## Crash Pattern To Recognize

If tablet logs show:

```text
failed to connect to /10.0.2.2 (port 8083)
```

then the installed APK was built with emulator URLs and must be rebuilt with the tablet wrapper.

