import org.gradle.api.tasks.Exec
import java.nio.file.Files
import java.nio.file.StandardCopyOption

plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
    id("org.jetbrains.kotlin.plugin.compose")
    id("org.jetbrains.kotlin.plugin.serialization")
}

val rustTarget = "x86_64-linux-android"
val ndkVersion = "26.3.11579264"
val ndkRoot = "/usr/lib/android-sdk/ndk/$ndkVersion"
val rustLinker = "$ndkRoot/toolchains/llvm/prebuilt/linux-x86_64/bin/x86_64-linux-android21-clang"
val rustArchiver = "$ndkRoot/toolchains/llvm/prebuilt/linux-x86_64/bin/llvm-ar"
val rustToolchainBin = "/root/.rustup/toolchains/stable-x86_64-unknown-linux-gnu/bin"
val cargoBinary = "$rustToolchainBin/cargo"
val rustcBinary = "$rustToolchainBin/rustc"
val targetRootFile = file("../../target-root.txt")
val uiTargetRoot = File(
    System.getenv("AEROBAG_UI_TARGET_ROOT")
        ?: rootDir.parentFile.parentFile.resolve(targetRootFile.readText().trim()).absolutePath,
)
val rustTargetDir = uiTargetRoot.resolve("shared/rust-target")
val rustProjectDir = file("../../core-rust")
layout.buildDirectory.set(uiTargetRoot.resolve("android/build/app"))
val rustJniLibsDir = layout.buildDirectory.dir("generated/rustJniLibs")
val rustOutputAbiDir = layout.buildDirectory.dir("generated/rustJniLibs/x86_64")
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
val androidDevServerBaseUrl = "http://10.0.2.2:$webPort"
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

fun linkOrCopy(source: File, target: File) {
    target.parentFile.mkdirs()
    Files.deleteIfExists(target.toPath())
    try {
        Files.createLink(target.toPath(), source.toPath())
    } catch (_: Exception) {
        Files.copy(source.toPath(), target.toPath(), StandardCopyOption.REPLACE_EXISTING)
    }
}

val buildRustX86_64Android by tasks.registering(Exec::class) {
    workingDir = rustProjectDir
    environment("CARGO_HOME", "/root/.cargo")
    environment("RUSTUP_HOME", "/root/.rustup")
    environment("RUSTC", rustcBinary)
    environment("CARGO_TARGET_DIR", rustTargetDir.absolutePath)
    environment("CARGO_TARGET_X86_64_LINUX_ANDROID_LINKER", rustLinker)
    environment("CC_x86_64_linux_android", rustLinker)
    environment("CC_x86_64-linux-android", rustLinker)
    environment("CXX_x86_64_linux_android", rustLinker.replace("clang", "clang++"))
    environment("AR_x86_64_linux_android", rustArchiver)
    environment("ANDROID_NDK_ROOT", ndkRoot)
    environment("NDK_HOME", ndkRoot)
    commandLine(cargoBinary, "build", "-p", "app-ffi", "--target", rustTarget)
}

val copyRustX86_64Library by tasks.registering(Copy::class) {
    dependsOn(buildRustX86_64Android)
    from(File(rustTargetDir, "$rustTarget/debug/libapp_ffi.so"))
    into(rustOutputAbiDir)
    rename { "libapp_ffi.so" }
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
    }
}

android {
    namespace = "net.jonh.aerobag.prototype"
    compileSdk = 34

    defaultConfig {
        applicationId = "net.jonh.aerobag.prototype"
        minSdk = 26
        targetSdk = 34
        versionCode = 1
        versionName = "0.1.0"

        testInstrumentationRunner = "androidx.test.runner.AndroidJUnitRunner"
        vectorDrawables {
            useSupportLibrary = true
        }
    }

    buildTypes {
        release {
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
        compose = true
    }
    androidResources {
        noCompress += listOf("webp", "png", "db")
    }
    sourceSets.getByName("main").jniLibs.setSrcDirs(listOf(rustJniLibsDir))
    sourceSets.getByName("main").assets.setSrcDirs(listOf(generatedPrototypeAssetsDir))
    packaging {
        resources {
            excludes += "/META-INF/{AL2.0,LGPL2.1}"
        }
    }
}

tasks.named("preBuild") {
    dependsOn(copyRustX86_64Library)
    dependsOn(stageCanonicalAndroidAssets)
}

dependencies {
    implementation("androidx.core:core-ktx:1.13.1")
    implementation("androidx.activity:activity-compose:1.9.2")
    implementation("androidx.lifecycle:lifecycle-runtime-ktx:2.8.6")
    implementation("androidx.compose.ui:ui:1.7.3")
    implementation("androidx.compose.ui:ui-tooling-preview:1.7.3")
    implementation("androidx.compose.material3:material3:1.3.0")
    implementation("com.google.android.material:material:1.12.0")
    implementation("org.jetbrains.kotlinx:kotlinx-serialization-json:1.7.3")

    testImplementation("junit:junit:4.13.2")

    debugImplementation("androidx.compose.ui:ui-tooling:1.7.3")
    debugImplementation("androidx.compose.ui:ui-test-manifest:1.7.3")
}
