# Android Prototype

This is the Android mirror of the web `Content` prototype slice.

Current goals:
- native Kotlin/Compose shell
- same sample plan semantics as the web prototype
- same policy choices
- same inventory modes
- same content-satisfaction result

## Build Notes

This project is intended to use a modern Gradle wrapper and Android SDK.

The system `gradle` package on this machine is expected to be too old for Compose.
Use it only to bootstrap a wrapper if needed.

## Likely Requirements

- Java 17
- Android SDK
- Android platform/build-tools matching `compileSdk`
- modern Gradle wrapper
- `adb`
- emulator or physical device

## Current Status

The source scaffold is in place.
If the full Android toolchain is not yet installed, expect build/run to be blocked until:
- Java is installed
- wrapper is generated
- Android SDK is configured
