import java.io.File
import java.io.FileInputStream
import java.util.Properties

plugins {
    id("com.android.application")
}

val keystoreProperties = Properties().apply {
    val file = rootProject.file("keystore.properties")
    if (file.isFile) {
        FileInputStream(file).use { load(it) }
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

fun findGpuiAndroidJava(): File {
    val explicit = System.getenv("GPUI_ANDROID_JAVA_DIR")
    if (!explicit.isNullOrBlank()) {
        return file(explicit)
    }
    val vendored = file("../../../../vendor/gpui-android/android/src/main/java")
    if (vendored.isDirectory) {
        return vendored
    }
    val checkouts = File(System.getProperty("user.home"), ".cargo/git/checkouts")
    return checkouts.walkTopDown()
        .firstOrNull { it.isDirectory && it.path.endsWith("gpui-android/android/src/main/java") }
        ?: file("missing-gpui-android-java")
}

android {
    namespace = "com.dreamfly.reels"
    compileSdk = 35

    defaultConfig {
        applicationId = "com.dreamfly.reels"
        minSdk = 26
        targetSdk = 35
        versionCode = (project.findProperty("douyinVersionCode") as String? ?: "1").toInt()
        versionName = project.findProperty("douyinVersionName") as String? ?: "1.0.2"

        val requestedAbis = (project.findProperty("douyinAbiFilters") as String?)
            ?.split(',', ' ', '\t')
            ?.map(String::trim)
            ?.filter(String::isNotEmpty)
            ?: listOf("armeabi-v7a", "arm64-v8a")
        ndk {
            abiFilters += requestedAbis
        }

        manifestPlaceholders["nativeLibraryName"] = "reels_android"
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
        debug {
            isDebuggable = true
            isJniDebuggable = true
        }
        release {
            isMinifyEnabled = false
            signingConfig = if (hasReleaseSigning) {
                signingConfigs.getByName("release")
            } else {
                signingConfigs.getByName("debug")
            }
        }
    }

    sourceSets {
        getByName("main") {
            jniLibs.srcDirs("src/main/jniLibs")
            java.srcDir(findGpuiAndroidJava())
        }
    }

    packaging {
        jniLibs {
            keepDebugSymbols += listOf(
                "*/armeabi-v7a/libreels_android.so",
                "*/arm64-v8a/libreels_android.so",
            )
        }
    }
}

dependencies {
    implementation("androidx.core:core-splashscreen:1.0.1")
    implementation("androidx.activity:activity:1.10.1")
    testImplementation("junit:junit:4.13.2")
}
