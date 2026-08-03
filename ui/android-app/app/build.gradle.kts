// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import org.gradle.api.GradleException
import org.gradle.api.tasks.Exec
import java.nio.file.Files
import java.nio.file.StandardCopyOption
import java.time.Instant
import java.time.ZoneOffset
import java.time.format.DateTimeFormatter

plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
    id("org.jetbrains.kotlin.plugin.compose")
    id("org.jetbrains.kotlin.plugin.serialization")
}

val ndkVersion = "26.3.11579264"
val androidSdkRoot = System.getenv("ANDROID_SDK_ROOT")
    ?: System.getenv("ANDROID_HOME")
    ?: "/usr/lib/android-sdk"
val ndkRoot = System.getenv("ANDROID_NDK_ROOT")
    ?: "$androidSdkRoot/ndk/$ndkVersion"
val ndkToolchainBin = "$ndkRoot/toolchains/llvm/prebuilt/linux-x86_64/bin"
val rustArchiver = "$ndkRoot/toolchains/llvm/prebuilt/linux-x86_64/bin/llvm-ar"
val cargoHome = System.getenv("CARGO_HOME")
    ?: File(System.getProperty("user.home"), ".cargo").absolutePath
val rustupHome = System.getenv("RUSTUP_HOME")
    ?: File(System.getProperty("user.home"), ".rustup").absolutePath
val cargoBinary = System.getenv("CARGO")
    ?: "cargo"
val rustcBinary = System.getenv("RUSTC")
    ?: "rustc"

fun readBooleanBuildConfig(key: String, defaultValue: Boolean): Boolean {
    val rawValue = System.getenv(key)
        ?: readInstanceConfigValue(key)
        ?: return defaultValue
    return when (rawValue.lowercase()) {
        "1", "true", "yes", "on" -> true
        "0", "false", "no", "off" -> false
        else -> throw IllegalArgumentException("$key must be a boolean, got '$rawValue'")
    }
}

fun readStringListBuildConfig(key: String): List<String>? {
    val rawValue = System.getenv(key)
        ?: readInstanceConfigValue(key)
        ?: return null
    return rawValue
        .split(",")
        .map { it.trim() }
        .filter { it.isNotEmpty() }
}

data class RustAndroidTarget(
    val rustTriple: String,
    val abi: String,
    val linkerPrefix: String,
    val envPrefix: String,
)
val rustAndroidTargets = listOf(
    RustAndroidTarget(
        rustTriple = "x86_64-linux-android",
        abi = "x86_64",
        linkerPrefix = "x86_64-linux-android",
        envPrefix = "X86_64_LINUX_ANDROID",
    ),
    RustAndroidTarget(
        rustTriple = "aarch64-linux-android",
        abi = "arm64-v8a",
        linkerPrefix = "aarch64-linux-android",
        envPrefix = "AARCH64_LINUX_ANDROID",
    ),
)
val targetRootFile = file("../../target-root.txt")
val uiTargetRoot = File(
    System.getenv("AEROBAG_UI_TARGET_ROOT")
        ?: rootDir.parentFile.parentFile.resolve(targetRootFile.readText().trim()).absolutePath,
)
val rustTargetDir = uiTargetRoot.resolve("shared/rust-target")
val rustProjectDir = file("../../core-rust")
layout.buildDirectory.set(uiTargetRoot.resolve("android/build/app"))
val rustJniLibsDir = layout.buildDirectory.dir("generated/rustJniLibs")
val generatedPrototypeAssetsDir = project.objects.directoryProperty().convention(layout.dir(project.provider { uiTargetRoot.resolve("android/assets") }))
val repoRoot = file("../../..")
val instanceConfigFile = repoRoot.parentFile.resolve("INSTANCE_CONFIG")
fun readInstanceConfigValue(key: String): String? {
    if (!instanceConfigFile.isFile) return null
    return instanceConfigFile.readLines()
        .map { it.trim() }
        .filter { it.isNotEmpty() && !it.startsWith("#") }
        .firstNotNullOfOrNull { line ->
            val separator = line.indexOf('=')
            if (separator <= 0) return@firstNotNullOfOrNull null
            val parsedKey = line.substring(0, separator).trim()
            if (parsedKey != key) return@firstNotNullOfOrNull null
            line.substring(separator + 1).trim().trim('"', '\'')
        }
}

val webPort = System.getenv("WEB_PORT")
    ?: readInstanceConfigValue("WEB_PORT")
    ?: "8080"
val androidDevServerBaseUrl = System.getenv("ANDROID_DEV_SERVER_BASE_URL")
    ?: readInstanceConfigValue("ANDROID_DEV_SERVER_BASE_URL")
    ?: "http://10.0.2.2:$webPort"
val androidPackageSourceBaseUrl = System.getenv("ANDROID_PACKAGE_SOURCE_BASE_URL")
    ?: readInstanceConfigValue("ANDROID_PACKAGE_SOURCE_BASE_URL")
    ?: "$androidDevServerBaseUrl/packages/"
val androidLiveFeedSourceBaseUrl = System.getenv("ANDROID_LIVE_FEED_SOURCE_BASE_URL")?.takeIf { it.isNotBlank() }
    ?: readInstanceConfigValue("ANDROID_LIVE_FEED_SOURCE_BASE_URL")
    ?: ""
val androidCloudServerBaseUrl = System.getenv("ANDROID_CLOUD_SERVER_BASE_URL")?.takeIf { it.isNotBlank() }
    ?: readInstanceConfigValue("ANDROID_CLOUD_SERVER_BASE_URL")
    ?: ""
fun readIntegerBuildConfig(key: String, defaultValue: Int): Int {
    val rawValue = System.getenv(key)
        ?: readInstanceConfigValue(key)
        ?: return defaultValue
    return rawValue.toIntOrNull()
        ?: throw IllegalArgumentException("$key must be an integer, got '$rawValue'")
}

fun readRequiredBuildConfig(key: String): String =
    System.getenv(key)
        ?: readInstanceConfigValue(key)
        ?: throw GradleException("$key must be set")

fun buildConfigStringLiteral(value: String): String =
    "\"" + value.replace("\\", "\\\\").replace("\"", "\\\"") + "\""

fun gitOutput(vararg args: String): String? {
    return runCatching {
        val process = ProcessBuilder(listOf("git", "-C", repoRoot.absolutePath, *args))
            .redirectErrorStream(false)
            .start()
        val output = process.inputStream.bufferedReader().readText().trim()
        if (process.waitFor() == 0 && output.isNotEmpty()) output else null
    }.getOrNull()
}

val androidBuildInstant = Instant.now()
val androidBuildStampUtc = System.getenv("AEROBAG_BUILD_STAMP_UTC")
    ?: DateTimeFormatter.ofPattern("yyyyMMddHHmm")
        .withZone(ZoneOffset.UTC)
        .format(androidBuildInstant)
val androidBuiltAtUtc = System.getenv("AEROBAG_BUILT_AT_UTC")
    ?: DateTimeFormatter.ISO_INSTANT.format(androidBuildInstant)
val androidGitCommit = System.getenv("AEROBAG_GIT_COMMIT")
    ?: gitOutput("rev-parse", "HEAD")
    ?: "unknown"
val androidShortCommit = System.getenv("AEROBAG_SHORT_COMMIT")
    ?: gitOutput("rev-parse", "--short=8", "HEAD")
    ?: if (androidGitCommit == "unknown") "unknown" else androidGitCommit.take(8)
val androidBuildDirty = System.getenv("AEROBAG_BUILD_DIRTY")
    ?.let { it.lowercase() in setOf("1", "true", "yes", "on") }
    ?: ((gitOutput("status", "--porcelain") ?: "").isNotEmpty())
val androidBuildId = androidShortCommit + if (androidBuildDirty) ".dirty" else ""
val defaultAndroidVersionName = "0.1.$androidBuildStampUtc+$androidBuildId"
val androidVersionCode = readIntegerBuildConfig(
    "ANDROID_VERSION_CODE",
    (androidBuildInstant.epochSecond / 60).toInt(),
)
val androidVersionName = System.getenv("ANDROID_VERSION_NAME")
    ?: readInstanceConfigValue("ANDROID_VERSION_NAME")
    ?: System.getenv("AEROBAG_VERSION_NAME")
    ?: defaultAndroidVersionName
val androidSigningKeystore = File(readRequiredBuildConfig("AEROBAG_ANDROID_KEYSTORE"))
val androidSigningKeystorePassword = readRequiredBuildConfig("AEROBAG_ANDROID_KEYSTORE_PASSWORD")
val androidSigningKeyAlias = readRequiredBuildConfig("AEROBAG_ANDROID_KEY_ALIAS")
val androidSigningKeyPassword = readRequiredBuildConfig("AEROBAG_ANDROID_KEY_PASSWORD")
val androidBuildRustRelease = readBooleanBuildConfig("ANDROID_BUILD_RUST_RELEASE", false)
val androidBuildNativeLibraries = readBooleanBuildConfig("ANDROID_BUILD_NATIVE_LIBRARIES", true)
val androidRustProfileArgs = if (androidBuildRustRelease) listOf("--release") else emptyList()
val androidRustProfileDir = if (androidBuildRustRelease) "release" else "debug"
val androidRust16KbPageSizeRustFlags = listOf(
    "-Clink-arg=-Wl,-z,max-page-size=16384",
)
val androidTargetAbiFilters = readStringListBuildConfig("ANDROID_TARGET_ABIS")
val enabledRustAndroidTargets = androidTargetAbiFilters
    ?.let { requestedAbis ->
        val knownAbis = rustAndroidTargets.map { it.abi }.toSet()
        val unknownAbis = requestedAbis.filter { it !in knownAbis }
        if (unknownAbis.isNotEmpty()) {
            throw IllegalArgumentException("ANDROID_TARGET_ABIS contains unsupported Rust ABI(s): ${unknownAbis.joinToString(", ")}")
        }
        rustAndroidTargets.filter { it.abi in requestedAbis }
    }
    ?: rustAndroidTargets
if (enabledRustAndroidTargets.isEmpty()) {
    throw IllegalArgumentException("ANDROID_TARGET_ABIS did not select any Rust Android targets")
}
fun rustFlagsForAndroidTarget(target: RustAndroidTarget): String {
    val envKey = "CARGO_TARGET_${target.envPrefix}_RUSTFLAGS"
    val existingFlags = System.getenv(envKey)?.trim()?.takeIf { it.isNotEmpty() }
    return (listOfNotNull(existingFlags) + androidRust16KbPageSizeRustFlags).joinToString(" ")
}
val artifactReadPathConfigFile = repoRoot.resolve(".aerobag-artifact-read-path")
val configuredArtifactRoot = artifactReadPathConfigFile.readText().trim()
val defaultArtifactRoot =
    if (File(configuredArtifactRoot).isAbsolute) {
        File(configuredArtifactRoot)
    } else {
        repoRoot.resolve(configuredArtifactRoot)
    }
val artifactRoot = File(
    System.getenv("AEROBAG_ARTIFACT_READ_PATH")
        ?: defaultArtifactRoot.absolutePath,
)

val uiThemeFile = file("../../shared-fixtures/ui-theme.json")
val devBootstrapFile = file("../../shared/dev-bootstrap.json")
val generatedSymbolSourceDir = layout.buildDirectory.dir("generated/aerobagSymbols/kotlin")
val generatedWireSourceDir = layout.buildDirectory.dir("generated/aerobagWire/kotlin")

fun linkOrCopy(source: File, target: File) {
    target.parentFile.mkdirs()
    Files.deleteIfExists(target.toPath())
    try {
        Files.createLink(target.toPath(), source.toPath())
    } catch (_: Exception) {
        Files.copy(source.toPath(), target.toPath(), StandardCopyOption.REPLACE_EXISTING)
    }
}

val copyRustLibraries = if (androidBuildNativeLibraries) {
    enabledRustAndroidTargets.map { target ->
        val linker = "$ndkToolchainBin/${target.linkerPrefix}21-clang"
        val buildTask = tasks.register<Exec>("buildRust${target.abi.replace("-", "").replace("_", "")}Android") {
            workingDir = rustProjectDir
            environment("CARGO_HOME", cargoHome)
            environment("RUSTUP_HOME", rustupHome)
            environment("RUSTC", rustcBinary)
            environment("CARGO_TARGET_DIR", rustTargetDir.absolutePath)
            environment("CARGO_TARGET_${target.envPrefix}_LINKER", linker)
            environment("CC_${target.rustTriple.replace("-", "_")}", linker)
            environment("CC_${target.rustTriple}", linker)
            environment("CXX_${target.rustTriple.replace("-", "_")}", linker.replace("clang", "clang++"))
            environment("AR_${target.rustTriple.replace("-", "_")}", rustArchiver)
            environment("CARGO_TARGET_${target.envPrefix}_RUSTFLAGS", rustFlagsForAndroidTarget(target))
            environment("ANDROID_NDK_ROOT", ndkRoot)
            environment("NDK_HOME", ndkRoot)
            commandLine(listOf(cargoBinary, "build", "-p", "app-ffi", "--target", target.rustTriple) + androidRustProfileArgs)
        }
        tasks.register<Copy>("copyRust${target.abi.replace("-", "").replace("_", "")}Library") {
            dependsOn(buildTask)
            from(File(rustTargetDir, "${target.rustTriple}/$androidRustProfileDir/libapp_ffi.so"))
            into(rustJniLibsDir.map { it.dir(target.abi) })
            rename { "libapp_ffi.so" }
        }
    }
} else {
    emptyList()
}

val stageCanonicalAndroidAssets by tasks.registering {
    outputs.dir(generatedPrototypeAssetsDir)
    outputs.upToDateWhen { false }
    doFirst {
        delete(generatedPrototypeAssetsDir.get().asFile)
    }
    doLast {
        val assetRoot = generatedPrototypeAssetsDir.get().asFile
        val fixturesDir = assetRoot.resolve("fixtures")
        fixturesDir.mkdirs()
        linkOrCopy(uiThemeFile, fixturesDir.resolve("ui-theme.json"))
        linkOrCopy(devBootstrapFile, fixturesDir.resolve("dev-bootstrap.json"))
        fixturesDir.resolve("android-dev-server-base-url.txt").writeText(androidDevServerBaseUrl)
        fixturesDir.resolve("android-package-source-base-url.txt").writeText(androidPackageSourceBaseUrl)
        fixturesDir.resolve("android-live-feed-source-base-url.txt").writeText(androidLiveFeedSourceBaseUrl)
        fixturesDir.resolve("android-cloud-server-base-url.txt").writeText(androidCloudServerBaseUrl)
    }
}

val generateSharedNavSymbols by tasks.registering(Exec::class) {
    workingDir = repoRoot
    inputs.file(repoRoot.resolve("ui/shared-symbols/nav-symbols.json"))
    inputs.file(repoRoot.resolve("tools/generate-nav-symbols.mjs"))
    outputs.dir(generatedSymbolSourceDir)
    commandLine(
        "node",
        repoRoot.resolve("tools/generate-nav-symbols.mjs").absolutePath,
        "--android-out",
        generatedSymbolSourceDir.get().asFile.resolve("org/aerobag/app/generated").absolutePath,
        "--web-out",
        repoRoot.resolve("ui/web-app/src/generated/navSymbols.ts").absolutePath,
    )
}

val generateSharedWireTypes by tasks.registering(Exec::class) {
    workingDir = repoRoot
    inputs.file(repoRoot.resolve("ui/core-rust/schemas/nexrad-overlay-wire.schema.json"))
    inputs.file(repoRoot.resolve("ui/core-rust/schemas/cloud-wire.schema.json"))
    inputs.file(repoRoot.resolve("tools/generate-ui-wire-types.mjs"))
    inputs.file(repoRoot.resolve("ui/core-rust/schemas/home-page-wire.schema.json"))
    outputs.dir(generatedWireSourceDir)
    outputs.file(repoRoot.resolve("ui/web-app/src/generated/nexradOverlayWire.ts"))
    outputs.file(repoRoot.resolve("ui/web-app/src/generated/cloudWire.ts"))
    commandLine(
        "node",
        repoRoot.resolve("tools/generate-ui-wire-types.mjs").absolutePath,
        "--android-out",
        generatedWireSourceDir.get().asFile.resolve("org/aerobag/app/generated").absolutePath,
        "--web-out",
        repoRoot.resolve("ui/web-app/src/generated/nexradOverlayWire.ts").absolutePath,
    )
}

android {
    namespace = "org.aerobag.app"
    compileSdk = 34

    defaultConfig {
        applicationId = "org.aerobag.app"
        minSdk = 26
        targetSdk = 34
        versionCode = androidVersionCode
        versionName = androidVersionName
        buildConfigField("String", "AEROBAG_BUILT_AT_UTC", buildConfigStringLiteral(androidBuiltAtUtc))
        buildConfigField("String", "AEROBAG_GIT_COMMIT", buildConfigStringLiteral(androidGitCommit))
        buildConfigField("boolean", "AEROBAG_BUILD_DIRTY", androidBuildDirty.toString())

        testInstrumentationRunner = "androidx.test.runner.AndroidJUnitRunner"
        vectorDrawables {
            useSupportLibrary = true
        }
        if (androidTargetAbiFilters != null) {
            ndk {
                abiFilters += androidTargetAbiFilters
            }
        }
    }

    signingConfigs {
        create("aerobag") {
            storeFile = androidSigningKeystore
            storePassword = androidSigningKeystorePassword
            keyAlias = androidSigningKeyAlias
            keyPassword = androidSigningKeyPassword
        }
    }

    buildTypes {
        debug {
            signingConfig = signingConfigs.getByName("aerobag")
        }
        release {
            signingConfig = signingConfigs.getByName("aerobag")
            isMinifyEnabled = false
            proguardFiles(
                getDefaultProguardFile("proguard-android-optimize.txt"),
                "proguard-rules.pro",
            )
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
    kotlinOptions {
        jvmTarget = "17"
    }
    buildFeatures {
        buildConfig = true
        compose = true
    }
    androidResources {
        noCompress += listOf("webp", "png", "db")
    }
    sourceSets.getByName("main").jniLibs.setSrcDirs(listOf(rustJniLibsDir))
    sourceSets.getByName("main").assets.setSrcDirs(listOf(generatedPrototypeAssetsDir))
    sourceSets.getByName("main").java.srcDir(generatedSymbolSourceDir)
    sourceSets.getByName("main").java.srcDir(generatedWireSourceDir)
    packaging {
        resources {
            excludes += "/META-INF/{AL2.0,LGPL2.1}"
        }
    }
}

tasks.named("preBuild") {
    dependsOn(copyRustLibraries)
    dependsOn(stageCanonicalAndroidAssets)
    dependsOn(generateSharedNavSymbols)
    dependsOn(generateSharedWireTypes)
}

dependencies {
    implementation("androidx.core:core-ktx:1.13.1")
    implementation("androidx.activity:activity-compose:1.9.2")
    implementation("androidx.lifecycle:lifecycle-runtime-ktx:2.8.6")
    implementation("androidx.work:work-runtime-ktx:2.9.1")
    implementation("androidx.compose.ui:ui:1.7.3")
    implementation("androidx.compose.ui:ui-tooling-preview:1.7.3")
    implementation("androidx.compose.material3:material3:1.3.0")
    implementation("com.google.android.material:material:1.12.0")
    implementation("com.google.android.gms:play-services-auth:21.6.0")
    implementation("com.google.android.gms:play-services-code-scanner:16.1.0")
    implementation("com.google.android.gms:play-services-location:21.3.0")
    implementation("com.squareup.okhttp3:okhttp:4.12.0")
    implementation("org.jetbrains.kotlinx:kotlinx-serialization-json:1.7.3")

    testImplementation("junit:junit:4.13.2")

    debugImplementation("androidx.compose.ui:ui-tooling:1.7.3")
    debugImplementation("androidx.compose.ui:ui-test-manifest:1.7.3")
}
