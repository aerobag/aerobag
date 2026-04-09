import org.gradle.api.tasks.Exec
import groovy.json.JsonSlurper
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
val rustToolchainBin = "/root/.rustup/toolchains/stable-x86_64-unknown-linux-gnu/bin"
val cargoBinary = "$rustToolchainBin/cargo"
val rustcBinary = "$rustToolchainBin/rustc"
val rustProjectDir = file("../../core-rust")
val rustJniLibsDir = layout.buildDirectory.dir("generated/rustJniLibs")
val rustOutputAbiDir = layout.buildDirectory.dir("generated/rustJniLibs/x86_64")
val generatedPrototypeAssetsDir = layout.buildDirectory.dir("generated/prototypeAssets")
val generatedPrototypeSeedPackagesDir = layout.buildDirectory.dir("generated/prototypeSeedPackages")
val generatedPrototypeSeedChartPackagesDir = layout.buildDirectory.dir("generated/prototypeSeedChartPackages")
val uiFixtureGenerator = file("../../scripts/generate_content_fixture.py")
val resourceIndexFile = file("../../shared-fixtures/content-prototype/resource-index.json")
val artifactRoot = File(
    System.getenv("AEROBAG_ARTIFACT_ROOT")
        ?: rootDir.parentFile.parentFile.resolveSibling("aerobag-artifacts").absolutePath,
)

fun resolveArtifactPath(rawPath: String): File {
    val source = file(rawPath)
    if (source.isFile) {
        return source
    }
    val marker = "${File.separator}product-builds${File.separator}"
    val raw = rawPath.replace('\\', File.separatorChar)
    val markerIndex = raw.indexOf(marker)
    if (markerIndex >= 0) {
        val relative = raw.substring(markerIndex + marker.length)
        val rebased = artifactRoot.resolve("product-builds").resolve(relative)
        if (rebased.isFile) {
            return rebased
        }
    }
    return source
}

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
    doFirst {
        delete(generatedPrototypeAssetsDir.get().dir("sectional-packages").asFile)
    }
    commandLine("python3", uiFixtureGenerator.absolutePath)
}

val stagePrototypeSectionalPackages by tasks.registering {
    dependsOn(generatePrototypeFixture)
    outputs.dir(generatedPrototypeSeedPackagesDir.map { it.dir("sectional-packages") })
    outputs.upToDateWhen { false }
    doLast {
        val payload = JsonSlurper().parse(resourceIndexFile) as Map<*, *>
        val packages = (payload["packages"] as List<*>)
            .filterIsInstance<Map<*, *>>()
            .filter {
                val familyId = it["family_id"] as? String
                familyId in setOf("sectional", "tac", "ifr_low", "ifr_high")
            }
            .map { resolveArtifactPath(it["artifact_path"] as String) }
        val outputDir = generatedPrototypeSeedPackagesDir.get().dir("sectional-packages").asFile
        delete(outputDir)
        outputDir.mkdirs()
        packages.forEach { source ->
            if (!source.isFile) {
                throw GradleException("missing staged package ${source.absolutePath}")
            }
            Files.copy(
                source.toPath(),
                outputDir.resolve(source.name).toPath(),
                StandardCopyOption.REPLACE_EXISTING,
            )
        }
    }
}

val seedPrototypeSectionalPackages by tasks.registering {
    dependsOn("installDebug")
    dependsOn(stagePrototypeSectionalPackages)
    doLast {
        val packageDir = generatedPrototypeSeedPackagesDir.get().dir("sectional-packages").asFile
        if (!packageDir.isDirectory) {
            throw GradleException("missing staged package directory ${packageDir.absolutePath}")
        }
        val tempDir = "/data/local/tmp/aerobag-packages"
        exec {
            commandLine("adb", "shell", "mkdir", "-p", tempDir)
        }
        exec {
            commandLine("adb", "shell", "run-as", "net.jonh.aerobag.prototype", "mkdir", "-p", "files/sectional-packages")
        }
        packageDir.listFiles()
            ?.filter { it.isFile && it.extension == "zip" }
            ?.sortedBy { it.name }
            ?.forEach { packageFile ->
                exec {
                    commandLine("adb", "push", packageFile.absolutePath, "$tempDir/${packageFile.name}")
                }
                exec {
                    commandLine("adb", "shell", "run-as", "net.jonh.aerobag.prototype", "cp", "$tempDir/${packageFile.name}", "files/sectional-packages/${packageFile.name}")
                }
                exec {
                    commandLine("adb", "shell", "rm", "-f", "$tempDir/${packageFile.name}")
                }
            }
        exec {
            commandLine("adb", "shell", "rm", "-rf", tempDir)
        }
    }
}

val stagePrototypeChartPackages by tasks.registering {
    dependsOn(generatePrototypeFixture)
    outputs.dir(generatedPrototypeSeedChartPackagesDir.map { it.dir("chart-packages") })
    outputs.upToDateWhen { false }
    doLast {
        val payload = JsonSlurper().parse(resourceIndexFile) as Map<*, *>
        val packages = (payload["packages"] as List<*>)
            .filterIsInstance<Map<*, *>>()
            .filter { (it["id"] as? String) in setOf("NW_TPP", "NW_CSUP") }
            .map { resolveArtifactPath(it["artifact_path"] as String) }
        val outputDir = generatedPrototypeSeedChartPackagesDir.get().dir("chart-packages").asFile
        delete(outputDir)
        outputDir.mkdirs()
        packages.forEach { source ->
            if (!source.isFile) {
                throw GradleException("missing staged chart package ${source.absolutePath}")
            }
            Files.copy(
                source.toPath(),
                outputDir.resolve(source.name).toPath(),
                StandardCopyOption.REPLACE_EXISTING,
            )
        }
    }
}

val seedPrototypeChartPackages by tasks.registering {
    dependsOn("installDebug")
    dependsOn(stagePrototypeChartPackages)
    doLast {
        val packageDir = generatedPrototypeSeedChartPackagesDir.get().dir("chart-packages").asFile
        if (!packageDir.isDirectory) {
            throw GradleException("missing staged chart package directory ${packageDir.absolutePath}")
        }
        val tempDir = "/data/local/tmp/aerobag-chart-packages"
        exec {
            commandLine("adb", "shell", "rm", "-rf", tempDir)
        }
        exec {
            commandLine("adb", "shell", "mkdir", "-p", tempDir)
        }
        exec {
            commandLine("adb", "shell", "run-as", "net.jonh.aerobag.prototype", "rm", "-rf", "files/chart-packages")
        }
        exec {
            commandLine("adb", "shell", "run-as", "net.jonh.aerobag.prototype", "mkdir", "-p", "files/chart-packages")
        }
        packageDir.listFiles()
            ?.filter { it.isFile && it.extension == "zip" }
            ?.sortedBy { it.name }
            ?.forEach { packageFile ->
                exec {
                    commandLine("adb", "push", packageFile.absolutePath, "$tempDir/${packageFile.name}")
                }
                exec {
                    commandLine("adb", "shell", "run-as", "net.jonh.aerobag.prototype", "cp", "$tempDir/${packageFile.name}", "files/chart-packages/${packageFile.name}")
                }
                exec {
                    commandLine("adb", "shell", "rm", "-f", "$tempDir/${packageFile.name}")
                }
            }
        exec {
            commandLine("adb", "shell", "rm", "-rf", tempDir)
        }
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
