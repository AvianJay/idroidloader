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

val signingKeystore = System.getenv("KEYSTORE_FILE")
val nightlyBuild = System.getenv("IDROID_NIGHTLY_BUILD")?.toInt()

android {
    System.getenv("NDK_HOME")?.let { ndkVersion = file(it).name }
    compileSdk = 36
    namespace = "app.idroidloader.mobile"
    defaultConfig {
        manifestPlaceholders["usesCleartextTraffic"] = "false"
        applicationId = "app.idroidloader.mobile"
        minSdk = 26
        targetSdk = 36
        testInstrumentationRunner = "androidx.test.runner.AndroidJUnitRunner"
        versionCode = nightlyBuild?.let { 100_000_000 + it }
            ?: tauriProperties.getProperty("tauri.android.versionCode", "1").toInt()
        versionName = tauriProperties.getProperty("tauri.android.versionName", "1.0") +
            (nightlyBuild?.let { "-nightly.$it" } ?: "")
    }
    signingConfigs {
        if (!signingKeystore.isNullOrBlank()) {
            create("release") {
                storeFile = file(signingKeystore)
                keyAlias = System.getenv("KEYSTORE_ALIAS")?.takeIf { it.isNotBlank() }
                    ?: error("KEYSTORE_ALIAS is required for release signing")
                val password = System.getenv("KEYSTORE_PASSWORD")?.takeIf { it.isNotBlank() }
                    ?: error("KEYSTORE_PASSWORD is required for release signing")
                storePassword = password
                keyPassword = password
            }
        }
    }
    buildTypes {
        getByName("debug") {
            manifestPlaceholders["usesCleartextTraffic"] = "true"
            isDebuggable = true
            isJniDebuggable = true
            isMinifyEnabled = false
        }
        getByName("release") {
            if (!signingKeystore.isNullOrBlank()) {
                signingConfig = signingConfigs.getByName("release")
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
    implementation("androidx.activity:activity-ktx:1.10.1")
    implementation("com.google.android.material:material:1.12.0")
    implementation("androidx.lifecycle:lifecycle-process:2.10.0")
    testImplementation("junit:junit:4.13.2")
    androidTestImplementation("androidx.test.ext:junit:1.3.0")
    androidTestImplementation("androidx.test:runner:1.7.0")
}

apply(from = file("tauri.build.gradle.kts"))
