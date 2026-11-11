import java.io.File

plugins {
    id("com.android.application")
}

fun findGpuiAndroidJava(): File {
    val explicit = System.getenv("GPUI_ANDROID_JAVA_DIR")
    if (!explicit.isNullOrBlank()) {
        return file(explicit)
    }
    val checkouts = File(System.getProperty("user.home"), ".cargo/git/checkouts")
    return checkouts.walkTopDown()
        .firstOrNull { it.isDirectory && it.path.endsWith("gpui-android/android/src/main/java") }
        ?: file("missing-gpui-android-java")
}

android {
    namespace = "com.dreamfly.douyinParser"
    compileSdk = 35

    defaultConfig {
        applicationId = "com.dreamfly.douyinParser"
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

        manifestPlaceholders["nativeLibraryName"] = "douyin_parser_android"
    }

    buildTypes {
        debug {
            isDebuggable = true
            isJniDebuggable = true
        }
        release {
            isMinifyEnabled = false
            signingConfig = signingConfigs.getByName("debug")
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
                "*/armeabi-v7a/libdouyin_parser_android.so",
                "*/arm64-v8a/libdouyin_parser_android.so",
            )
        }
    }
}

dependencies {
    testImplementation("junit:junit:4.13.2")
}
