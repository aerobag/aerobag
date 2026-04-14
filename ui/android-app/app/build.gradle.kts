import org.gradle.api.tasks.Exec
import groovy.json.JsonSlurper
import java.nio.file.Files
import java.nio.file.StandardCopyOption
import java.util.zip.ZipFile

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
val generatedPrototypeSeedPackagesDir = layout.buildDirectory.dir("generated/prototypeSeedPackages")
val generatedPrototypeSeedChartPackagesDir = layout.buildDirectory.dir("generated/prototypeSeedChartPackages")
val repoRoot = rootDir.parentFile.parentFile
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
fun latestCurrentArtifacts(root: File): File? =
    root.resolve("published-packaged")
        .listFiles()
        ?.filter { it.isFile && it.name.startsWith("current_artifacts_") && it.name.endsWith(".json") }
        ?.maxByOrNull { it.name }

val resolvedArtifactRoot = artifactRoot
val currentArtifactsFile = latestCurrentArtifacts(resolvedArtifactRoot)
    ?: throw GradleException("missing current_artifacts_*.json under ${resolvedArtifactRoot.resolve("published-packaged").absolutePath}")
val currentArtifactsPayload by lazy { JsonSlurper().parse(currentArtifactsFile) as Map<*, *> }
val bundleFilename = ((currentArtifactsPayload["bundles"] as? List<*>)?.lastOrNull() as? Map<*, *>)?.get("filename") as? String
    ?: throw GradleException("missing bundles[-1].filename in ${currentArtifactsFile.absolutePath}")
val productBuildFile = resolvedArtifactRoot.resolve("published-packaged").resolve(bundleFilename)
val productBuildPayload by lazy { JsonSlurper().parse(productBuildFile) as Map<*, *> }
val productCycle = productBuildPayload["cycle"] as? String
    ?: throw GradleException("missing cycle in ${productBuildFile.absolutePath}")
val bundlePackageFilenamesById by lazy {
    val packages = productBuildPayload["packages"] as? List<*> ?: emptyList<Any?>()
    packages
        .filterIsInstance<Map<*, *>>()
        .mapNotNull { entry ->
            val id = entry["id"] as? String ?: return@mapNotNull null
            val filename = entry["filename"] as? String ?: return@mapNotNull null
            id to filename
        }
        .toMap()
}

fun resolvePublishedFilename(rawPath: String): File {
    val relative = File(rawPath)
    if (relative.isAbsolute) {
        throw GradleException("expected published filename, got absolute path $rawPath")
    }
    if (relative.toPath().nameCount != 1) {
        throw GradleException("expected flat published filename, got $rawPath")
    }
    return resolvedArtifactRoot.resolve("published-packaged").resolve(rawPath)
}

fun resolveProductBuildOutput(nodeName: String, outputName: String): File {
    val topLevel = productBuildPayload[nodeName] as? Map<*, *>
    if (topLevel != null) {
        val rawPath = topLevel["relative_path"] as? String
        if (!rawPath.isNullOrBlank()) {
            val resolved = resolvePublishedFilename(rawPath)
            if (resolved.isFile) {
                return resolved
            }
            throw GradleException("missing product build output $nodeName at ${resolved.absolutePath}")
        }
    }
    val nodes = productBuildPayload["nodes"] as? List<*> ?: emptyList<Any?>()
    for (node in nodes) {
        val nodeMap = node as? Map<*, *> ?: continue
        if (nodeMap["name"] != nodeName) continue
        val outputs = nodeMap["outputs"] as? Map<*, *> ?: break
        val rawPath = outputs[outputName] as? String
        if (!rawPath.isNullOrBlank()) {
            val resolved = resolvePublishedFilename(rawPath)
            if (resolved.isFile) {
                return resolved
            }
            throw GradleException("missing product build output ${nodeName}.${outputName} at ${resolved.absolutePath}")
        }
        break
    }
    throw GradleException("missing product build output ${nodeName}.${outputName} in ${productBuildFile.absolutePath}")
}

val resourceIndexFile = resolveProductBuildOutput("resource_index", "resource_index")
val dataZipFile = resolveProductBuildOutput("data", "zip")
val vectorsZipFile = resolveProductBuildOutput("vectors", "zip")
val uiThemeFile = file("../../shared-fixtures/ui-theme.json")
val devBootstrapFile = file("../../shared/dev-bootstrap.json")

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
        val navDbDir = assetRoot.resolve("nav-db")
        fixturesDir.mkdirs()
        navDbDir.mkdirs()
        Files.copy(resourceIndexFile.toPath(), fixturesDir.resolve("resource-index.json").toPath(), StandardCopyOption.REPLACE_EXISTING)
        Files.copy(vectorsZipFile.toPath(), fixturesDir.resolve("vectors.zip").toPath(), StandardCopyOption.REPLACE_EXISTING)
        Files.copy(uiThemeFile.toPath(), fixturesDir.resolve("ui-theme.json").toPath(), StandardCopyOption.REPLACE_EXISTING)
        Files.copy(devBootstrapFile.toPath(), fixturesDir.resolve("dev-bootstrap.json").toPath(), StandardCopyOption.REPLACE_EXISTING)
        ZipFile(dataZipFile).use { zip ->
            val entry = zip.getEntry("main.db")
                ?: throw GradleException("missing main.db in packaged data zip ${dataZipFile.absolutePath}")
            zip.getInputStream(entry).use { input ->
                Files.copy(input, navDbDir.resolve("main.db").toPath(), StandardCopyOption.REPLACE_EXISTING)
            }
        }
    }
}

val stagePrototypeSectionalPackages by tasks.registering {
    outputs.dir(generatedPrototypeSeedPackagesDir.map { it.dir("sectional-packages") })
    outputs.upToDateWhen { false }
    doLast {
        val payload = JsonSlurper().parse(resourceIndexFile) as Map<*, *>
        val packages = (payload["packages"] as List<*>)
            .filterIsInstance<Map<*, *>>()
            .filter {
                val familyId = it["family_id"] as? String
                familyId in setOf("sec", "tac", "enr-l", "enr-h")
            }
            .map {
                val packageId = it["id"] as? String
                    ?: throw GradleException("missing package id in ${resourceIndexFile.absolutePath}")
                val filename = bundlePackageFilenamesById[packageId]
                    ?: throw GradleException("missing bundle filename for $packageId in ${productBuildFile.absolutePath}")
                resolvePublishedFilename(filename) to "$packageId.zip"
            }
        val outputDir = generatedPrototypeSeedPackagesDir.get().dir("sectional-packages").asFile
        delete(outputDir)
        outputDir.mkdirs()
        packages.forEach { (source, targetName) ->
            if (!source.isFile) {
                throw GradleException("missing staged package ${source.absolutePath}")
            }
            Files.copy(
                source.toPath(),
                outputDir.resolve(targetName).toPath(),
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
            commandLine("adb", "shell", "run-as", "net.jonh.aerobag.prototype", "rm", "-rf", "files/sectional-packages")
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
    outputs.dir(generatedPrototypeSeedChartPackagesDir.map { it.dir("chart-packages") })
    outputs.upToDateWhen { false }
    doLast {
        val payload = JsonSlurper().parse(resourceIndexFile) as Map<*, *>
        val packages = (payload["packages"] as List<*>)
            .filterIsInstance<Map<*, *>>()
            .filter {
                val packageId = it["id"] as? String ?: return@filter false
                packageId.startsWith("NW_TPP") || packageId.startsWith("NW_CSUP")
            }
            .map {
                val packageId = it["id"] as? String
                    ?: throw GradleException("missing package id in ${resourceIndexFile.absolutePath}")
                val filename = bundlePackageFilenamesById[packageId]
                    ?: throw GradleException("missing bundle filename for $packageId in ${productBuildFile.absolutePath}")
                resolvePublishedFilename(filename) to "$packageId.zip"
            }
        val outputDir = generatedPrototypeSeedChartPackagesDir.get().dir("chart-packages").asFile
        delete(outputDir)
        outputDir.mkdirs()
        packages.forEach { (source, targetName) ->
            if (!source.isFile) {
                throw GradleException("missing staged chart package ${source.absolutePath}")
            }
            Files.copy(
                source.toPath(),
                outputDir.resolve(targetName).toPath(),
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
