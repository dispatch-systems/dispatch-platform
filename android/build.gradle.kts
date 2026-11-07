import com.android.build.api.artifact.SingleArtifact
import com.android.build.api.variant.ApplicationAndroidComponentsExtension
import com.android.build.api.variant.BuiltArtifactsLoader
import groovy.json.JsonOutput
import org.gradle.work.DisableCachingByDefault

plugins {
    alias(libs.plugins.android.application) apply false
    alias(libs.plugins.kotlin.compose) apply false
    alias(libs.plugins.kotlin.serialization) apply false
    alias(libs.plugins.ksp) apply false
    alias(libs.plugins.room) apply false
}

@DisableCachingByDefault(because = "Reports local application metadata")
abstract class DispatchApps : DefaultTask() {
    @get:Input abstract val variants: ListProperty<String>
    @get:OutputFile abstract val report: RegularFileProperty

    @TaskAction
    fun write() {
        report.get().asFile.apply {
            parentFile.mkdirs()
            writeText(variants.get().joinToString(",", "[", "]"))
        }
    }
}

@DisableCachingByDefault(because = "Reports local APK paths")
abstract class DispatchApk : DefaultTask() {
    @get:InputDirectory
    @get:PathSensitive(PathSensitivity.RELATIVE)
    abstract val apkDirectory: DirectoryProperty
    @get:Internal abstract val loader: Property<BuiltArtifactsLoader>
    @get:OutputFile abstract val report: RegularFileProperty

    @TaskAction
    fun write() {
        val built = loader.get().load(apkDirectory.get()) ?: error("Cannot read APK metadata")
        report.get().asFile.apply {
            parentFile.mkdirs()
            writeText(JsonOutput.toJson(mapOf(
                "applicationId" to built.applicationId,
                "variant" to built.variantName,
                "apks" to built.elements.map { it.outputFile },
            )))
        }
    }
}

val dispatchdevApps = tasks.register<DispatchApps>("dispatchdevApps") {
    variants.convention(emptyList())
    report.set(layout.buildDirectory.file("dispatchdev/apps.json"))
}

subprojects {
    plugins.withId("com.android.application") {
        extensions.getByType<ApplicationAndroidComponentsExtension>().onVariants { variant ->
            if (variant.debuggable) {
                val taskName = variant.computeTaskName("dispatchdev", "Apk")
                val reportPath = layout.buildDirectory.file("dispatchdev/${variant.name}.json")
                tasks.register<DispatchApk>(taskName) {
                    apkDirectory.set(variant.artifacts.get(SingleArtifact.APK))
                    loader.set(variant.artifacts.getBuiltArtifactsLoader())
                    report.set(reportPath)
                }
                val entry = JsonOutput.toJson(mapOf(
                    "module" to path,
                    "variant" to variant.name,
                    "flavors" to variant.productFlavors.map { it.second },
                    "task" to "$path:$taskName",
                    "report" to reportPath.get().asFile.absolutePath,
                ))
                dispatchdevApps.configure { variants.add(entry) }
            }
        }
    }
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
