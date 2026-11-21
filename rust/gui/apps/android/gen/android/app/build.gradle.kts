import java.io.FileInputStream
import java.util.Properties
import org.jetbrains.kotlin.gradle.dsl.JvmTarget

plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
    id("rust")
}

val tauriProperties = Properties().apply {
    val propFile = file("tauri.properties")
    if (propFile.exists()) {
        propFile.inputStream().use { load(it) }
    }
}

val keystoreProperties = Properties().apply {
    val propFile = rootProject.file("keystore.properties")
    if (propFile.isFile) {
        FileInputStream(propFile).use { load(it) }
    }
}

fun signingValue(envName: String, propertyName: String): String? =
    System.getenv(envName)?.takeIf { it.isNotBlank() }
        ?: keystoreProperties.getProperty(propertyName)?.takeIf { it.isNotBlank() }

val releaseStoreFile = signingValue("ANDROID_KEYSTORE_PATH", "storeFile")
val releaseStorePassword = signingValue("ANDROID_KEYSTORE_PASSWORD", "storePassword")
val releaseKeyAlias = signingValue("ANDROID_KEY_ALIAS", "keyAlias")
val releaseKeyPassword = signingValue("ANDROID_KEY_PASSWORD", "keyPassword")
val hasReleaseSigning = !releaseStoreFile.isNullOrBlank()

if (hasReleaseSigning) {
    check(!releaseStorePassword.isNullOrBlank()) {
        "缺少 ANDROID_KEYSTORE_PASSWORD 或 keystore.properties 中的 storePassword"
    }
    check(!releaseKeyAlias.isNullOrBlank()) {
        "缺少 ANDROID_KEY_ALIAS 或 keystore.properties 中的 keyAlias"
    }
    check(!releaseKeyPassword.isNullOrBlank()) {
        "缺少 ANDROID_KEY_PASSWORD 或 keystore.properties 中的 keyPassword"
    }
}

android {
    compileSdk = 36
    namespace = "com.dreamfly.reels"
    defaultConfig {
        manifestPlaceholders["usesCleartextTraffic"] = "false"
        if (gradle.startParameter.taskNames.any { it.contains("Debug", ignoreCase = true) }) {
            applicationId = "com.dreamfly.reels.debug"
        } else {
            applicationId = "com.dreamfly.reels"
        }
        minSdk = 26
        targetSdk = 35
        versionCode = tauriProperties.getProperty("tauri.android.versionCode", "1").toInt()
        versionName = tauriProperties.getProperty("tauri.android.versionName", "1.0")
    }
    signingConfigs {
        create("release") {
            if (hasReleaseSigning) {
                storeFile = rootProject.file(releaseStoreFile!!)
                storePassword = releaseStorePassword
                keyAlias = releaseKeyAlias
                keyPassword = releaseKeyPassword
            }
        }
    }
    buildTypes {
        getByName("debug") {
            manifestPlaceholders["usesCleartextTraffic"] = "true"
            isDebuggable = true
            isJniDebuggable = true
            isMinifyEnabled = false
            packaging {
                jniLibs.keepDebugSymbols.add("*/arm64-v8a/*.so")
                jniLibs.keepDebugSymbols.add("*/armeabi-v7a/*.so")
                jniLibs.keepDebugSymbols.add("*/x86/*.so")
                jniLibs.keepDebugSymbols.add("*/x86_64/*.so")
            }
        }
        getByName("release") {
            signingConfig = if (hasReleaseSigning) {
                signingConfigs.getByName("release")
            } else {
                signingConfigs.getByName("debug")
            }
            optimization {
                enable = true
            }
            proguardFiles(
                *fileTree(".") {
                    include("**/*.pro")
                    exclude("build/**")
                }.files.toTypedArray()
            )
        }
    }
    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_1_8
        targetCompatibility = JavaVersion.VERSION_1_8
    }
    sourceSets {
        getByName("main") {
            assets.srcDirs("../../../frontend/dist")
        }
    }
    buildFeatures {
        buildConfig = true
    }
}

kotlin {
    compilerOptions {
        jvmTarget = JvmTarget.JVM_1_8
    }
}

rust {
    rootDirRel = "../../../"
}

dependencies {
    implementation("androidx.webkit:webkit:1.14.0")
    implementation("androidx.appcompat:appcompat:1.7.1")
    implementation("androidx.core:core-splashscreen:1.0.1")
    implementation("androidx.activity:activity-ktx:1.10.1")
    implementation("com.google.android.material:material:1.12.0")
    implementation("androidx.lifecycle:lifecycle-process:2.10.0")
    testImplementation("junit:junit:4.13.2")
    androidTestImplementation("androidx.test.ext:junit:1.1.4")
    androidTestImplementation("androidx.test.espresso:espresso-core:3.5.0")
}

apply(from = file("tauri.build.gradle.kts"))

val cleanStaleFrontendAssets by tasks.registering(Delete::class) {
    delete(layout.projectDirectory.dir("src/main/assets/assets"))
    delete(layout.projectDirectory.file("src/main/assets/index.html"))
}

tasks.named("preBuild").configure {
    dependsOn(cleanStaleFrontendAssets)
}

tasks.matching { it.name.contains("Lint") }.configureEach {
    dependsOn(cleanStaleFrontendAssets)
}
