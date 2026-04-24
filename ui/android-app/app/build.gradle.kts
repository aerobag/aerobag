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
fun latestCurrentArtifacts(root: File): File? =
    root.resolve("published-packaged")
        .listFiles()
        ?.filter { it.isFile && it.name.startsWith("current_artifacts_") && it.name.endsWith(".json") }
        ?.maxByOrNull { it.name }

fun allCurrentArtifacts(root: File): List<File> =
    root.resolve("published-packaged")
        .listFiles()
        ?.filter { it.isFile && it.name == "current_artifacts.json" || (it.isFile && it.name.startsWith("current_artifacts_") && it.name.endsWith(".json")) }
        ?.sortedBy { it.name }
        ?: emptyList()

val resolvedArtifactRoot = artifactRoot
val currentArtifactsFile = latestCurrentArtifacts(resolvedArtifactRoot)
    ?: throw GradleException("missing current_artifacts_*.json under ${resolvedArtifactRoot.resolve("published-packaged").absolutePath}")
val currentArtifactsPayload by lazy { JsonSlurper().parse(currentArtifactsFile) as Map<*, *> }
val allCurrentArtifactsFiles by lazy { allCurrentArtifacts(resolvedArtifactRoot) }
val bundleFilename = ((currentArtifactsPayload["bundles"] as? List<*>)?.firstOrNull {
    (it as? Map<*, *>)?.get("bundle_type") == "cycle"
} as? Map<*, *>)?.get("filename") as? String
    ?: throw GradleException("missing cycle bundle filename in ${currentArtifactsFile.absolutePath}")
val productBuildFile = resolvedArtifactRoot.resolve("published-packaged").resolve(bundleFilename)
val productBuildPayload by lazy { JsonSlurper().parse(productBuildFile) as Map<*, *> }
val bundlePackagesById by lazy {
    val packages = productBuildPayload["packages"] as? List<*> ?: emptyList<Any?>()
    packages
        .filterIsInstance<Map<*, *>>()
        .mapNotNull { entry ->
            val id = entry["id"] as? String ?: return@mapNotNull null
            id to entry
        }
        .toMap()
}
val discoveryCycleBundleFiles by lazy {
    allCurrentArtifactsFiles
        .flatMap { manifestFile ->
            val payload = JsonSlurper().parse(manifestFile) as Map<*, *>
            (payload["bundles"] as? List<*> ?: emptyList<Any?>())
                .filterIsInstance<Map<*, *>>()
                .filter { it["bundle_type"] == "cycle" }
                .mapNotNull { it["filename"] as? String }
        }
        .distinct()
        .sorted()
        .map(::resolvePublishedFilename)
}
val discoveryPackageFiles by lazy {
    discoveryCycleBundleFiles
        .flatMap { bundleFile ->
            val payload = JsonSlurper().parse(bundleFile) as Map<*, *>
            (payload["packages"] as? List<*> ?: emptyList<Any?>())
                .filterIsInstance<Map<*, *>>()
                .mapNotNull { it["filename"] as? String }
        }
        .distinct()
        .sorted()
        .map(::resolvePublishedFilename)
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

val uiThemeFile = file("../../shared-fixtures/ui-theme.json")
val devBootstrapFile = file("../../shared/dev-bootstrap.json")

fun resolveBundlePackageFile(packageId: String): File {
    val packageEntry = bundlePackagesById[packageId]
        ?: throw GradleException("missing bundle package $packageId in ${productBuildFile.absolutePath}")
    val filename = packageEntry["filename"] as? String
        ?: throw GradleException("missing filename for bundle package $packageId in ${productBuildFile.absolutePath}")
    return resolvePublishedFilename(filename)
}

fun resolveBundlePackagesForFamilies(vararg familyIds: String): List<Pair<File, String>> {
    val wanted = familyIds.toSet()
    val packages = productBuildPayload["packages"] as? List<*> ?: emptyList<Any?>()
    return packages
        .filterIsInstance<Map<*, *>>()
        .filter { (it["family_id"] as? String) in wanted }
        .map { entry ->
            val packageId = entry["id"] as? String
                ?: throw GradleException("missing package id in ${productBuildFile.absolutePath}")
            resolveBundlePackageFile(packageId) to "$packageId.zip"
        }
}

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
        linkOrCopy(currentArtifactsFile, fixturesDir.resolve("current-artifacts.json"))
        linkOrCopy(productBuildFile, fixturesDir.resolve("cycle-bundle.json"))
        val packageManagementDiscoveryDir = fixturesDir.resolve("package-management/discovery")
        val packageManagementBundlesDir = fixturesDir.resolve("package-management/bundles")
        packageManagementDiscoveryDir.mkdirs()
        packageManagementBundlesDir.mkdirs()
        allCurrentArtifactsFiles.forEach { manifestFile ->
            linkOrCopy(manifestFile, packageManagementDiscoveryDir.resolve(manifestFile.name))
        }
        discoveryCycleBundleFiles.forEach { bundleFile ->
            linkOrCopy(bundleFile, packageManagementBundlesDir.resolve(bundleFile.name))
        }
        linkOrCopy(uiThemeFile, fixturesDir.resolve("ui-theme.json"))
        linkOrCopy(devBootstrapFile, fixturesDir.resolve("dev-bootstrap.json"))
        fixturesDir.resolve("android-dev-server-base-url.txt").writeText(androidDevServerBaseUrl)
    }
}

val stageDevPackageManagementSourcePackages by tasks.registering {
    outputs.dir(generatedPrototypeSeedChartPackagesDir.map { it.dir("package-management-source") })
    outputs.upToDateWhen { false }
    doLast {
        val outputDir = generatedPrototypeSeedChartPackagesDir.get().dir("package-management-source").asFile
        delete(outputDir)
        outputDir.mkdirs()
        discoveryPackageFiles.forEach { source ->
            if (!source.isFile) {
                throw GradleException("missing staged package management source package ${source.absolutePath}")
            }
            Files.copy(
                source.toPath(),
                outputDir.resolve(source.name).toPath(),
                StandardCopyOption.REPLACE_EXISTING,
            )
        }
    }
}

val seedDevPackageManagementSourcePackages by tasks.registering {
    dependsOn("installDebug")
    dependsOn(stageDevPackageManagementSourcePackages)
    doLast {
        val packageDir = generatedPrototypeSeedChartPackagesDir.get().dir("package-management-source").asFile
        if (!packageDir.isDirectory) {
            throw GradleException("missing package management source directory ${packageDir.absolutePath}")
        }
        val targetDir = "/storage/emulated/0/Android/data/net.jonh.aerobag.prototype/files/package-management-source"
        exec {
            commandLine("adb", "shell", "rm", "-rf", targetDir)
        }
        exec {
            commandLine("adb", "shell", "mkdir", "-p", targetDir)
        }
        packageDir.listFiles()
            ?.filter { it.isFile && it.extension == "zip" }
            ?.sortedBy { it.name }
            ?.forEach { packageFile ->
                exec {
                    commandLine("adb", "push", packageFile.absolutePath, "$targetDir/${packageFile.name}")
                }
            }
    }
}

val stageDevChartPackages by tasks.registering {
    outputs.dir(generatedPrototypeSeedPackagesDir.map { it.dir("chart-packages") })
    outputs.upToDateWhen { false }
    doLast {
        val packages = resolveBundlePackagesForFamilies("sec", "tac", "enr-l", "enr-h", "shaded-relief")
        val outputDir = generatedPrototypeSeedPackagesDir.get().dir("chart-packages").asFile
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

val seedDevChartPackages by tasks.registering {
    dependsOn("installDebug")
    dependsOn(stageDevChartPackages)
    doLast {
        val packageDir = generatedPrototypeSeedPackagesDir.get().dir("chart-packages").asFile
        if (!packageDir.isDirectory) {
            throw GradleException("missing staged package directory ${packageDir.absolutePath}")
        }
        val tempDir = "/data/local/tmp/aerobag-chart-packages"
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

val stageDevPlatePackages by tasks.registering {
    outputs.dir(generatedPrototypeSeedChartPackagesDir.map { it.dir("plate-packages") })
    outputs.upToDateWhen { false }
    doLast {
        val packages = resolveBundlePackagesForFamilies("tpp", "csup")
            .filter { (_, targetName) ->
                targetName.startsWith("NW_") || targetName.startsWith("SW_")
            }
        val outputDir = generatedPrototypeSeedChartPackagesDir.get().dir("plate-packages").asFile
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

val seedDevPlatePackages by tasks.registering {
    dependsOn("installDebug")
    dependsOn(stageDevPlatePackages)
    doLast {
        val packageDir = generatedPrototypeSeedChartPackagesDir.get().dir("plate-packages").asFile
        if (!packageDir.isDirectory) {
            throw GradleException("missing staged chart package directory ${packageDir.absolutePath}")
        }
        val tempDir = "/data/local/tmp/aerobag-plate-packages"
        exec {
            commandLine("adb", "shell", "rm", "-rf", tempDir)
        }
        exec {
            commandLine("adb", "shell", "mkdir", "-p", tempDir)
        }
        exec {
            commandLine("adb", "shell", "run-as", "net.jonh.aerobag.prototype", "rm", "-rf", "files/plate-packages")
        }
        exec {
            commandLine("adb", "shell", "run-as", "net.jonh.aerobag.prototype", "mkdir", "-p", "files/plate-packages")
        }
        packageDir.listFiles()
            ?.filter { it.isFile && it.extension == "zip" }
            ?.sortedBy { it.name }
            ?.forEach { packageFile ->
                exec {
                    commandLine("adb", "push", packageFile.absolutePath, "$tempDir/${packageFile.name}")
                }
                exec {
                    commandLine("adb", "shell", "run-as", "net.jonh.aerobag.prototype", "cp", "$tempDir/${packageFile.name}", "files/plate-packages/${packageFile.name}")
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

val stageDevDataPackages by tasks.registering {
    outputs.dir(generatedPrototypeSeedChartPackagesDir.map { it.dir("data-packages") })
    outputs.upToDateWhen { false }
    doLast {
        val packages = resolveBundlePackagesForFamilies("nav-db", "vectors")
        val outputDir = generatedPrototypeSeedChartPackagesDir.get().dir("data-packages").asFile
        delete(outputDir)
        outputDir.mkdirs()
        packages.forEach { (source, targetName) ->
            if (!source.isFile) {
                throw GradleException("missing staged data package ${source.absolutePath}")
            }
            Files.copy(
                source.toPath(),
                outputDir.resolve(targetName).toPath(),
                StandardCopyOption.REPLACE_EXISTING,
            )
        }
    }
}

val seedDevDataPackages by tasks.registering {
    dependsOn("installDebug")
    dependsOn(stageDevDataPackages)
    doLast {
        val packageDir = generatedPrototypeSeedChartPackagesDir.get().dir("data-packages").asFile
        if (!packageDir.isDirectory) {
            throw GradleException("missing staged data package directory ${packageDir.absolutePath}")
        }
        val tempDir = "/data/local/tmp/aerobag-data-packages"
        exec {
            commandLine("adb", "shell", "rm", "-rf", tempDir)
        }
        exec {
            commandLine("adb", "shell", "mkdir", "-p", tempDir)
        }
        exec {
            commandLine("adb", "shell", "run-as", "net.jonh.aerobag.prototype", "rm", "-rf", "files/data-packages")
        }
        exec {
            commandLine("adb", "shell", "run-as", "net.jonh.aerobag.prototype", "mkdir", "-p", "files/data-packages")
        }
        packageDir.listFiles()
            ?.filter { it.isFile && it.extension == "zip" }
            ?.sortedBy { it.name }
            ?.forEach { packageFile ->
                exec {
                    commandLine("adb", "push", packageFile.absolutePath, "$tempDir/${packageFile.name}")
                }
                exec {
                    commandLine("adb", "shell", "run-as", "net.jonh.aerobag.prototype", "cp", "$tempDir/${packageFile.name}", "files/data-packages/${packageFile.name}")
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
