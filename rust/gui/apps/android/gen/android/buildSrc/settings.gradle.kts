pluginManagement {
    repositories {
        // Gradle 9.6.1 的 kotlin-dsl 依赖在 Maven Central 有，插件门户未必同步。
        google()
        mavenCentral()
        gradlePluginPortal()
    }
}

rootProject.name = "buildSrc"
