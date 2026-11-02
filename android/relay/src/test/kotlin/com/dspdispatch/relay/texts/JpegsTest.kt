package com.dspdispatch.relay.texts

import java.io.File
import java.nio.file.Files
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class JpegsTest {
    private val folder = Files.createTempDirectory("jpegs").toFile()
    private val pictures = FakePictures(jpeg = { byteArrayOf(-1, -40, -1) })
    private val jpegs = Jpegs(File(folder, "jpegs"), pictures)

    @After
    fun clean() {
        folder.deleteRecursively()
    }

    @Test
    fun takesHeifPicturesByTheirTypeOrByTheirNameWhenTheTypeSaysOnlyBytes() {
        assertTrue(heif("IMG_0001.HEIC", "image/heic"))
        assertTrue(heif("part-12", "image/heif"))
        assertTrue(heif("IMG_0001.heic", "image/heic-sequence"))
        assertTrue(heif("IMG_0001.heif", "image/heif-sequence"))
        assertTrue(heif("IMG_0001.HEIC", "application/octet-stream"))
        assertTrue(heif("IMG_0001.heif", "application/octet-stream"))

        assertFalse(heif("door.jpg", "image/jpeg"))
        assertFalse(heif("door.avif", "image/avif"))
        assertFalse(heif("IMG_0001.HEIC", "image/jpeg"))
        assertFalse(heif("notes.pdf", "application/octet-stream"))
        assertFalse(heif("IMG_0001.heic.pdf", "application/octet-stream"))
    }

    @Test
    fun namesAJpegForThePictureItsMadeOf() {
        assertEquals("IMG_0001.jpg", jpegName("IMG_0001.HEIC"))
        assertEquals("IMG_0001.jpg", jpegName("IMG_0001.heif"))
        assertEquals("part-12.jpg", jpegName("part-12"))
        // Within the 200 characters Dispatch takes of a file's name.
        assertEquals("a".repeat(196) + ".jpg", jpegName("a".repeat(200)))
        assertEquals("é".repeat(196) + ".jpg", jpegName("é".repeat(198) + ".heic"))
    }

    @Test
    fun keepsAJpegMadeOfAPictureAsItsFilesUntilLetGo() {
        val file = WaitingFile("mms-1-1", 0, "content://mms/part/1", "IMG_0001.HEIC", "image/heic", 731, id = 7)
        val jpeg = jpegs.of(file, byteArrayOf(1, 2, 3, 4))!!

        assertEquals(listOf<Byte>(1, 2, 3, 4), pictures.made.single().toList())
        assertEquals(file.copy(uri = jpeg.uri, name = "IMG_0001.jpg", mime = "image/jpeg", size = 3), jpeg)
        assertTrue(jpegs.holds(jpeg.uri))
        assertFalse(jpegs.holds(file.uri))
        assertEquals(3L, jpegs.size(jpeg.uri))
        assertEquals(listOf<Byte>(-1, -40, -1), jpegs.open(jpeg.uri).use { it.readBytes() }.toList())

        jpegs.remove(file.uri)
        assertEquals(3L, jpegs.size(jpeg.uri))
        jpegs.remove(jpeg.uri)
        assertNull(jpegs.size(jpeg.uri))
    }

    @Test
    fun makesNoneOfAPictureAndroidCantDecode() {
        val jpegs = Jpegs(File(folder, "jpegs"), FakePictures(jpeg = { null }))
        assertNull(jpegs.of(WaitingFile("mms-1-1", 0, "content://mms/part/1", "IMG_0001.HEIC", "image/heic", 731, id = 7), byteArrayOf(1)))
        assertFalse(File(folder, "jpegs").exists())
    }

    @Test
    fun letsGoOfEveryJpegButThoseOfFilesWaiting() {
        val waiting = jpegs.of(WaitingFile("mms-1-1", 0, "content://mms/part/1", "a.heic", "image/heic", 1, id = 1), byteArrayOf(1))!!
        val left = jpegs.of(WaitingFile("mms-2-2", 0, "content://mms/part/2", "b.heic", "image/heic", 1, id = 2), byteArrayOf(1))!!

        jpegs.keepOnly(listOf(waiting.uri, "content://mms/part/3"))

        assertEquals(3L, jpegs.size(waiting.uri))
        assertNull(jpegs.size(left.uri))
    }
}
