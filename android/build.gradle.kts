plugins {
    alias(libs.plugins.android.application) apply false
    alias(libs.plugins.kotlin.compose) apply false
    alias(libs.plugins.kotlin.serialization) apply false
    alias(libs.plugins.ksp) apply false
    alias(libs.plugins.room) apply false
}

// A test that runs away fails instead of taking the machine with it: each app's tests write no file
// over 1 GB (`gradle/capped-jdk`), run 15 minutes at most, and keep their scratch files in the app's
// build folder, emptied before each run, never in the machine's own temp folder.
subprojects {
    tasks.withType<Test>().configureEach {
        val scratch = layout.buildDirectory.dir("tmp/test-scratch/$name").get().asFile
        executable = rootProject.file("gradle/capped-jdk/bin/java").absolutePath
        systemProperty("java.io.tmpdir", scratch.absolutePath)
        timeout = java.time.Duration.ofMinutes(15)
        doFirst {
            scratch.deleteRecursively()
            scratch.mkdirs()
        }
    }
}
