plugins {
    alias(libs.plugins.android.application)
    alias(libs.plugins.kotlin.compose)
}

// The Rust core is built from this repository's crates for each ABI the
// app carries, and its Kotlin bindings are written from the library built.
val repository: File = rootDir.parentFile
val abis: List<String> =
    (findProperty("tsumugi.abis") as String? ?: "arm64-v8a,x86_64").split(",").map(String::trim)
val triples = mapOf(
    "arm64-v8a" to "aarch64-linux-android",
    "armeabi-v7a" to "armv7-linux-androideabi",
    "x86_64" to "x86_64-linux-android",
    "x86" to "i686-linux-android",
)
val profile: String = findProperty("tsumugi.profile") as String? ?: "release"
val rustLibs = layout.buildDirectory.dir("rust/jniLibs")
val bindings = layout.buildDirectory.dir("generated/uniffi")

android {
    namespace = "io.github.aikawa9376.tsumugi.android"
    compileSdk {
        version = release(37) { minorApiLevel = 2 }
    }
    ndkVersion = "30.0.16248370"

    defaultConfig {
        applicationId = "io.github.aikawa9376.tsumugi.android"
        minSdk = 29
        targetSdk = 37
        versionCode = 1
        versionName = "0.1.0"
        // Only the ABIs the Rust core is built for, of JNA's too.
        ndk {
            abiFilters += abis
        }
    }

    buildTypes {
        release {
            isMinifyEnabled = false
        }
    }

    buildFeatures {
        compose = true
    }

    sourceSets {
        getByName("main") {
            jniLibs.directories.add(rustLibs.get().asFile.path)
            kotlin.directories.add(bindings.get().asFile.path)
        }
    }
}

val cargoBuild = tasks.register<Exec>("cargoBuild") {
    description = "Builds the Rust core for each ABI the app carries."
    workingDir = repository
    val ndk = androidComponents.sdkComponents.ndkDirectory
    doFirst { environment("ANDROID_NDK_HOME", ndk.get().asFile.path) }
    val args = mutableListOf("cargo", "ndk")
    abis.forEach { args += listOf("-t", it) }
    args += listOf("-o", rustLibs.get().asFile.path, "build", "-p", "tsumugi-mobile")
    if (profile == "release") args += "--release"
    commandLine(args)
    // Cargo knows what changed; it is asked every build.
    outputs.upToDateWhen { false }
}

val uniffiBindings = tasks.register<Exec>("uniffiBindings") {
    description = "Writes the Kotlin bindings of the Rust core."
    dependsOn(cargoBuild)
    workingDir = repository
    val triple = triples.getValue(abis.first())
    val library = repository.resolve("target/$triple/${if (profile == "release") "release" else "debug"}/libtsumugi_mobile.so")
    commandLine(
        "cargo", "run", "--quiet", "-p", "tsumugi-mobile", "--features", "bindgen",
        "--bin", "uniffi-bindgen", "--", "generate",
        "--library", library.path,
        "--language", "kotlin",
        "--no-format",
        "--out-dir", bindings.get().asFile.path,
    )
    outputs.upToDateWhen { false }
}

tasks.named("preBuild") {
    dependsOn(uniffiBindings)
}

dependencies {
    implementation(platform(libs.compose.bom))
    implementation(libs.compose.ui)
    implementation(libs.compose.material3)
    implementation(libs.compose.ui.tooling.preview)
    implementation(libs.activity.compose)
    implementation(libs.lifecycle.viewmodel.compose)
    implementation(libs.lifecycle.runtime.compose)
    implementation(libs.coroutines.android)
    implementation("${libs.jna.get()}@aar")

    testImplementation(libs.junit)
}
