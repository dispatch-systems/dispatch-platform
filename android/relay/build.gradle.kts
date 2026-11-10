plugins {
    alias(libs.plugins.android.application)
    alias(libs.plugins.kotlin.compose)
    alias(libs.plugins.kotlin.serialization)
    alias(libs.plugins.ksp)
    alias(libs.plugins.room)
}

// The version comes from the build, never the source: CI names each one (`relay.versionCode`, the
// commit count of the built commit on main, and `relay.versionName`). A build without them is
// clearly no release: versionCode 1, named 0.0.0-local.
val relayVersionCode = providers.gradleProperty("relay.versionCode").orNull?.toInt() ?: 1
val relayVersionName = providers.gradleProperty("relay.versionName").orNull ?: "0.0.0-local"
require(relayVersionCode in 1..2_100_000_000) { "relay.versionCode is 1 to 2,100,000,000" }

// Release builds are signed in CI alone, by the key of the server's GitHub environment, never in
// the repo: RELAY_KEYSTORE_FILE names a PKCS12 keystore, and RELAY_KEYSTORE_PASSWORD,
// RELAY_KEY_ALIAS and RELAY_KEY_PASSWORD open it. Without it, a release builds unsigned.
val signing = providers.environmentVariable("RELAY_KEYSTORE_FILE").orNull?.let { path ->
    fun required(name: String) = providers.environmentVariable(name).orNull
        ?: throw GradleException("$name is required with RELAY_KEYSTORE_FILE")
    mapOf(
        "storeFile" to path,
        "storePassword" to required("RELAY_KEYSTORE_PASSWORD"),
        "keyAlias" to required("RELAY_KEY_ALIAS"),
        "keyPassword" to required("RELAY_KEY_PASSWORD"),
    )
}

android {
    namespace = "com.dspdispatch.relay"
    compileSdk = 37

    defaultConfig {
        applicationId = "com.dspdispatch.relay"
        minSdk = 26
        targetSdk = 37
        versionCode = relayVersionCode
        versionName = relayVersionName
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
                storeFile = file(signing.getValue("storeFile"))
                storeType = "pkcs12"
                storePassword = signing.getValue("storePassword")
                keyAlias = signing.getValue("keyAlias")
                keyPassword = signing.getValue("keyPassword")
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
        // A newer Gradle or Android plugin is Dependabot's weekly PR to make. As an error it
        // failed whatever change next touched an app, on the day of each release.
        informational += "AndroidGradlePluginVersion"
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
