import org.gradle.api.tasks.Exec
import java.nio.file.Files
import java.nio.file.StandardCopyOption

plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
    id("org.jetbrains.kotlin.plugin.compose")
    id("org.jetbrains.kotlin.plugin.serialization")
}

val ndkVersion = "26.3.11579264"
val ndkRoot = "/usr/lib/android-sdk/ndk/$ndkVersion"
val ndkToolchainBin = "$ndkRoot/toolchains/llvm/prebuilt/linux-x86_64/bin"
val rustArchiver = "$ndkRoot/toolchains/llvm/prebuilt/linux-x86_64/bin/llvm-ar"
val rustToolchainBin = "/root/.rustup/toolchains/stable-x86_64-unknown-linux-gnu/bin"
val cargoBinary = "$rustToolchainBin/cargo"
val rustcBinary = "$rustToolchainBin/rustc"
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
val androidPackageSourceBaseUrl = System.getenv("ANDROID_PACKAGE_SOURCE_BASE_URL")
    ?: readInstanceConfigValue("ANDROID_PACKAGE_SOURCE_BASE_URL")
    ?: "http://10.0.2.2:$webPort/packages/"
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

fun linkOrCopy(source: File, target: File) {
    target.parentFile.mkdirs()
    Files.deleteIfExists(target.toPath())
    try {
        Files.createLink(target.toPath(), source.toPath())
    } catch (_: Exception) {
        Files.copy(source.toPath(), target.toPath(), StandardCopyOption.REPLACE_EXISTING)
    }
}

val copyRustLibraries = rustAndroidTargets.map { target ->
    val linker = "$ndkToolchainBin/${target.linkerPrefix}21-clang"
    val buildTask = tasks.register<Exec>("buildRust${target.abi.replace("-", "").replace("_", "")}Android") {
        workingDir = rustProjectDir
        environment("CARGO_HOME", "/root/.cargo")
        environment("RUSTUP_HOME", "/root/.rustup")
        environment("RUSTC", rustcBinary)
        environment("CARGO_TARGET_DIR", rustTargetDir.absolutePath)
        environment("CARGO_TARGET_${target.envPrefix}_LINKER", linker)
        environment("CC_${target.rustTriple.replace("-", "_")}", linker)
        environment("CC_${target.rustTriple}", linker)
        environment("CXX_${target.rustTriple.replace("-", "_")}", linker.replace("clang", "clang++"))
        environment("AR_${target.rustTriple.replace("-", "_")}", rustArchiver)
        environment("ANDROID_NDK_ROOT", ndkRoot)
        environment("NDK_HOME", ndkRoot)
        commandLine(cargoBinary, "build", "-p", "app-ffi", "--target", target.rustTriple)
    }
    tasks.register<Copy>("copyRust${target.abi.replace("-", "").replace("_", "")}Library") {
        dependsOn(buildTask)
        from(File(rustTargetDir, "${target.rustTriple}/debug/libapp_ffi.so"))
        into(rustJniLibsDir.map { it.dir(target.abi) })
        rename { "libapp_ffi.so" }
    }
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
        fixturesDir.resolve("android-package-source-base-url.txt").writeText(androidPackageSourceBaseUrl)
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
        generatedSymbolSourceDir.get().asFile.resolve("net/jonh/aerobag/prototype/generated").absolutePath,
        "--web-out",
        repoRoot.resolve("ui/web-app/src/generated/navSymbols.ts").absolutePath,
    )
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
    sourceSets.getByName("main").java.srcDir(generatedSymbolSourceDir)
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
}

dependencies {
    implementation("androidx.core:core-ktx:1.13.1")
    implementation("androidx.activity:activity-compose:1.9.2")
    implementation("androidx.lifecycle:lifecycle-runtime-ktx:2.8.6")
    implementation("androidx.compose.ui:ui:1.7.3")
    implementation("androidx.compose.ui:ui-tooling-preview:1.7.3")
    implementation("androidx.compose.material3:material3:1.3.0")
    implementation("com.google.android.material:material:1.12.0")
    implementation("com.google.android.gms:play-services-location:21.3.0")
    implementation("org.jetbrains.kotlinx:kotlinx-serialization-json:1.7.3")

    testImplementation("junit:junit:4.13.2")

    debugImplementation("androidx.compose.ui:ui-tooling:1.7.3")
    debugImplementation("androidx.compose.ui:ui-test-manifest:1.7.3")
}
