import org.gradle.api.tasks.Copy
import org.gradle.api.tasks.Exec

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
val rustToolchainBin = "/root/.rustup/toolchains/stable-x86_64-unknown-linux-gnu/bin"
val cargoBinary = "$rustToolchainBin/cargo"
val rustcBinary = "$rustToolchainBin/rustc"
val rustProjectDir = file("../../core-rust")
val rustJniLibsDir = layout.buildDirectory.dir("generated/rustJniLibs")
val rustOutputAbiDir = layout.buildDirectory.dir("generated/rustJniLibs/x86_64")
val generatedPrototypeAssetsDir = layout.buildDirectory.dir("generated/prototypeAssets")
val sectionalPackageRunDir = file("../../../runs/20260406T032350Z-validation/native/charts-sec/work/charts-sec")
val tacPackageRunDir = file("../../../runs/20260406T032350Z-validation/native/charts-tac/work/charts-tac")
val uiFixtureGenerator = file("../../scripts/generate_content_fixture.py")

val buildRustX86_64Android by tasks.registering(Exec::class) {
    workingDir = rustProjectDir
    environment("CARGO_HOME", "/root/.cargo")
    environment("RUSTUP_HOME", "/root/.rustup")
    environment("RUSTC", rustcBinary)
    environment("CARGO_TARGET_X86_64_LINUX_ANDROID_LINKER", rustLinker)
    commandLine(cargoBinary, "build", "-p", "app-ffi", "--target", rustTarget)
}

val copyRustX86_64Library by tasks.registering(Copy::class) {
    dependsOn(buildRustX86_64Android)
    from(File(rustProjectDir, "target/$rustTarget/debug/libapp_ffi.so"))
    into(rustOutputAbiDir)
    rename { "libapp_ffi.so" }
}

val generatePrototypeFixture by tasks.registering(Exec::class) {
    workingDir = rootDir.parentFile.parentFile
    commandLine("python3", uiFixtureGenerator.absolutePath)
}

val stagePrototypeSectionalPackages by tasks.registering(Copy::class) {
    dependsOn(generatePrototypeFixture)
    from(File(sectionalPackageRunDir, "NW_SEC.zip"))
    from(File(sectionalPackageRunDir, "SW_SEC.zip"))
    from(File(tacPackageRunDir, "NW_TAC.zip"))
    from(File(tacPackageRunDir, "SW_TAC.zip"))
    into(generatedPrototypeAssetsDir.map { it.dir("sectional-packages") })
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
    sourceSets.getByName("main").jniLibs.srcDir(rustJniLibsDir)
    sourceSets.getByName("main").assets.srcDir(generatedPrototypeAssetsDir)
    packaging {
        resources {
            excludes += "/META-INF/{AL2.0,LGPL2.1}"
        }
    }
}

tasks.named("preBuild") {
    dependsOn(copyRustX86_64Library)
    dependsOn(generatePrototypeFixture)
    dependsOn(stagePrototypeSectionalPackages)
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
