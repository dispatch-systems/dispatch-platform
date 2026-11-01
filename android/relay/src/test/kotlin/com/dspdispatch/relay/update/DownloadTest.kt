package com.dspdispatch.relay.update

import com.dspdispatch.relay.dispatch.AppDetails
import java.io.ByteArrayInputStream
import java.io.File
import java.security.MessageDigest
import org.junit.After
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Test

class DownloadTest {
    private val apk = File.createTempFile("update", ".apk")
    private val bytes = ByteArray(200_000) { (it % 251).toByte() }

    @After
    fun delete() {
        apk.delete()
    }

    @Test
    fun savesTheApkTheDetailsDescribe() {
        val progress = mutableListOf<Long>()
        save(ByteArrayInputStream(bytes), apk, details(bytes)) { progress += it }
        assertArrayEquals(bytes, apk.readBytes())
        assertEquals(bytes.size.toLong(), progress.last())
        assertTrue(progress.zipWithNext().all { (a, b) -> a < b })
    }

    @Test
    fun stopsOnceLargerThanTheDetailsSay() {
        var read = 0L
        val endless = generateSequence { bytes }.flatMap { it.asSequence() }.iterator()
        val input = object : java.io.InputStream() {
            override fun read(): Int = endless.next().toInt().also { read++ } and BYTE
        }
        assertThrows(Mismatch::class.java) { save(input, apk, details(bytes)) {} }
        assertTrue(read <= bytes.size + BUFFER)
    }

    @Test
    fun refusesAShorterDownload() {
        assertThrows(Mismatch::class.java) { save(ByteArrayInputStream(bytes.copyOf(1_000)), apk, details(bytes)) {} }
    }

    @Test
    fun refusesAnotherApkOfTheSameSize() {
        val other = bytes.copyOf().also { it[0] = (it[0] + 1).toByte() }
        assertThrows(Mismatch::class.java) { save(ByteArrayInputStream(other), apk, details(bytes)) {} }
    }

    private fun details(of: ByteArray) = AppDetails("1.0.5", 5, of.size.toLong(), sha256(of))

    private companion object {
        const val BYTE = 0xff
        const val BUFFER = 64 * 1024

        fun sha256(bytes: ByteArray) = MessageDigest.getInstance("SHA-256").digest(bytes).joinToString("") { "%02x".format(it) }
    }
}
