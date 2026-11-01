import java.util.Properties

plugins {
    alias(libs.plugins.android.application)
    alias(libs.plugins.kotlin.compose)
    alias(libs.plugins.kotlin.serialization)
    alias(libs.plugins.ksp)
    alias(libs.plugins.room)
}

// The key that signs the app's releases lives on the MacBook it's built on, never in the repo:
// `dispatch.relay.signing` in ~/.gradle/gradle.properties names a properties file holding
// storeFile, storePassword, keyAlias and keyPassword. Without it, a release builds unsigned.
val signing = providers.gradleProperty("dispatch.relay.signing").orNull?.let { path ->
    Properties().apply { file(path).inputStream().use { load(it) } }
}

android {
    namespace = "com.dspdispatch.relay"
    compileSdk = 37

    defaultConfig {
        applicationId = "com.dspdispatch.relay"
        minSdk = 26
        targetSdk = 37
        versionCode = 1
        versionName = "0.1.0"
    }

    // One app for each server: Dev's to test, Production's for DSPs, each beside the other on a
    // phone. Production's pairs only with its DSPs, https://<code>.dspdispatch.com; Dev's only with
    // DSPs elsewhere (`Server`).
    defaultConfig.buildConfigField("String", "PRODUCTION_DOMAIN", "\"dspdispatch.com\"")
    flavorDimensions += "server"
    productFlavors {
        create("dev") {
            dimension = "server"
            applicationIdSuffix = ".dev"
            buildConfigField("boolean", "PRODUCTION", "false")
        }
        create("production") {
            dimension = "server"
            buildConfigField("boolean", "PRODUCTION", "true")
        }
    }

    signingConfigs {
        if (signing != null) {
            create("release") {
                storeFile = file(signing.getProperty("storeFile"))
                storePassword = signing.getProperty("storePassword")
                keyAlias = signing.getProperty("keyAlias")
                keyPassword = signing.getProperty("keyPassword")
            }
        }
    }

    buildTypes {
        debug {
            // A debug build installs beside the signed one of its server, never in its place.
            applicationIdSuffix = ".debug"
        }
        release {
            signingConfig = signingConfigs.findByName("release")
        }
    }

    lint {
        warningsAsErrors = true
    }

    buildFeatures {
        buildConfig = true
        compose = true
    }
}

// The texts queue's schema, each version kept, so a change to it comes with its migration.
room3 {
    schemaDirectory("$projectDir/schemas")
}

dependencies {
    implementation(platform(libs.compose.bom))
    implementation(libs.activity.compose)
    implementation(libs.camera.camera2)
    implementation(libs.camera.lifecycle)
    implementation(libs.camera.view)
    implementation(libs.compose.material3)
    implementation(libs.compose.ui)
    implementation(libs.compose.ui.tooling.preview)
    implementation(libs.core.ktx)
    implementation(libs.coroutines.android)
    implementation(libs.exifinterface)
    implementation(libs.lifecycle.runtime.compose)
    implementation(libs.lifecycle.service)
    implementation(libs.okhttp)
    implementation(libs.room.runtime)
    implementation(libs.serialization.json)
    implementation(libs.work.runtime)
    implementation(libs.zxing.core)
    debugImplementation(libs.compose.ui.tooling)
    ksp(libs.room.compiler)
    testImplementation(libs.coroutines.test)
    testImplementation(libs.junit)
    testImplementation(libs.okhttp.mockwebserver)
    testImplementation(libs.sqlite.bundled.jvm)
}
