package com.dspdispatch.relay

import java.io.File
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

// The guards android/build.gradle.kts puts on every test run, so a test that runs away can't fill
// the disk.
class TestGuardsTest {
    @Test
    fun scratchFilesStayInTheBuildFolder() {
        val scratch = File.createTempFile("guard", null).apply { deleteOnExit() }
        assertTrue(scratch.path, scratch.canonicalPath.contains("/build/tmp/test-scratch/"))
    }

    @Test
    fun noFileGrowsPastTheCap() {
        val cap = ProcessBuilder("bash", "-c", "ulimit -f").start().inputStream.bufferedReader().readText().trim()
        assertEquals("1048576", cap)
    }
}
