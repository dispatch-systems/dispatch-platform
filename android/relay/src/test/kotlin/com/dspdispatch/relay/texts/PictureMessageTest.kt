package com.dspdispatch.relay.texts

import java.io.File
import java.nio.file.Files
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class PictureMessageTest {
    private val folder = Files.createTempDirectory("files").toFile()
    private val asked = mutableListOf<Pair<String, Int>>()
    private val message = PictureMessage { file, most -> ByteArray(most / 2).also { asked += file.name to most } }

    @After
    fun clean() {
        folder.deleteRecursively()
    }

    @Test
    fun sendsFilesThatFitAsTheyAre() {
        val pdu = compose(30_000, file("door.jpg", "image/jpeg", 1_000), file("notes.pdf", "application/pdf", 2_000))!!
        assertTrue(asked.isEmpty())
        val text = String(pdu, Charsets.ISO_8859_1)
        for (part in listOf("door.jpg", "notes.pdf", "Door code 4412", "<img src=\"door.jpg\"", "<ref src=\"notes.pdf\"", "<text src=\"text.txt\"")) {
            assertTrue(part, part in text)
        }
    }

    @Test
    fun scalesPicturesDownToFitLeavingTheRestAsTheyAre() {
        val pdu = compose(
            20_000,
            file("small.png", "image/png", 1_000),
            file("door.jpg", "image/jpeg", 50_000),
            file("gate.heic", "image/heic", 60_000),
            file("notes.pdf", "application/pdf", 5_000),
        )!!
        assertTrue(pdu.size <= 20_000)
        assertEquals(listOf("door.jpg", "gate.heic"), asked.map { it.first })
        assertEquals("an even share each", asked[0].second, asked[1].second)
        val text = String(pdu, Charsets.ISO_8859_1)
        assertTrue("small.png" in text && "gate.jpg" in text && "image/jpeg" in text && "image/heic" !in text)
    }

    @Test
    fun failsWhatCantFit() {
        assertNull(compose(5_000, file("video.mp4", "video/mp4", 10_000)))
        val wont = PictureMessage { _, _ -> null }
        val door = file("door.jpg", "image/jpeg", 50_000)
        assertNull(wont.compose("+14255550199", "", listOf(door), 20_000, "T1", 1))
    }

    @Test
    fun givesEachPictureAnEvenShareOrWhatItNeeds() {
        assertEquals(listOf(100L, 400L, 400L), allot(900, listOf(100, 500, 1_000)))
        assertEquals(listOf(100L, 500L), allot(10_000, listOf(100, 500)))
        assertEquals(listOf(300L, 300L, 300L), allot(900, listOf(1_000, 1_000, 1_000)))
    }

    @Test
    fun scalesAPictureDownUntilItFits() {
        val sides = mutableListOf<Pair<Int, Int>>()
        // About as a JPEG grows: with its pixels, and its quality.
        val fitted = fit(200_000, 4_000) { side, quality ->
            sides += side to quality
            ByteArray(side * side * quality / 1_000)
        }!!
        assertTrue(fitted.size <= 200_000)
        assertEquals(LARGEST_SIDE to 85, sides.first())
        assertTrue(sides.zipWithNext().all { (a, b) -> b.first < a.first || b.second < a.second })
        assertNull(fit(10, 4_000) { side, _ -> ByteArray(side) })
        assertEquals(listOf(300 to 85), mutableListOf<Pair<Int, Int>>().also { tried -> fit(1_000_000, 300) { s, q -> tried += s to q; ByteArray(1) } })
    }

    private fun compose(most: Int, vararg files: Attachment) =
        message.compose("+14255550199", "Door code 4412", files.toList(), most, "T1", 1)

    private fun file(name: String, mime: String, size: Int) = Attachment(name, mime, File(folder, name).apply { writeBytes(ByteArray(size)) })
}
