plugins {
    alias(libs.plugins.android.application) apply false
    alias(libs.plugins.kotlin.compose) apply false
    alias(libs.plugins.kotlin.serialization) apply false
    alias(libs.plugins.ksp) apply false
    alias(libs.plugins.room) apply false
}

// Each app's tests write no file over 1 GB (`gradle/capped-jdk`), so a test that writes without end
// fails on its own write; run 15 minutes at most; and take their temp folder in the app's build
// folder, emptied before each run, not the machine's.
subprojects {
    tasks.withType<Test>().configureEach {
        val capped = rootProject.file("gradle/capped-jdk/bin/java")
        val scratch = layout.buildDirectory.dir("tmp/test-scratch/$name").get().asFile
        executable = capped.absolutePath
        // Gradle doesn't track the executable's contents: a change to the cap reruns the tests.
        inputs.file(capped).withPropertyName("cappedJava").withPathSensitivity(PathSensitivity.NONE)
        systemProperty("java.io.tmpdir", scratch.absolutePath)
        timeout = java.time.Duration.ofMinutes(15)
        doFirst {
            scratch.deleteRecursively()
            scratch.mkdirs()
        }
    }
}
