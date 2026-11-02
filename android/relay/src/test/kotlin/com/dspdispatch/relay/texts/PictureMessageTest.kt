package com.dspdispatch.relay.texts

import java.io.File
import java.nio.file.Files
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class PictureMessageTest {
    private val folder = Files.createTempDirectory("files").toFile()
    private var pictures = FakePictures()
    private val asked get() = pictures.asked.map { it.first to it.second }

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
        pictures = FakePictures { null }
        assertNull(compose(20_000, file("door.jpg", "image/jpeg", 50_000)))
    }

    @Test
    fun scalesAPictureDownToTheCarriersWidthAndHeightThoughItsBytesFit() {
        // Android can't read the third's size.
        pictures = FakePictures(mapOf("wide.jpg" to Size(3_000, 2_000), "tall.jpg" to Size(480, 640)))
        val files = listOf("wide.jpg", "tall.jpg", "odd.heic").map { file(it, "image/jpeg", 1_000) }
        val pdu = compose(300_000, *files.toTypedArray(), within = Size(640, 480))!!
        assertEquals("only the one too large", listOf("wide.jpg" to Size(640, 480)), pictures.asked.map { it.first to it.third })
        val text = String(pdu, Charsets.ISO_8859_1)
        assertTrue("either way up, and one Android can't size, as they are", "tall.jpg" in text && "odd.heic" in text)
    }

    @Test
    fun givesAPictureScaledToTheCarriersWidthAndHeightTheRoomItNeeds() {
        // A small PNG, too wide, whose JPEG at the carrier's width is larger than the PNG was.
        val jpeg = 1_107
        pictures = FakePictures(mapOf("wide.png" to Size(3_000, 200))) { most -> ByteArray(jpeg).takeIf { most >= jpeg } }
        val pdu = compose(300_000, file("wide.png", "image/png", 153), file("door.jpg", "image/jpeg", 1_000), within = Size(640, 480))!!
        assertEquals(listOf("wide.png"), asked.map { it.first })
        assertTrue("an even share of the room", asked.single().second > 100_000)
        val text = String(pdu, Charsets.ISO_8859_1)
        assertTrue("wide.jpg" in text && "wide.png" !in text && "door.jpg" in text)
    }

    @Test
    fun knowsWhatFitsWithinTheCarriersPictureSizeEitherWayUp() {
        val limit = Size(640, 480)
        assertTrue(Size(640, 480).fits(limit))
        assertTrue(Size(480, 640).fits(limit))
        assertFalse(Size(641, 300).fits(limit))
        assertEquals(640, Size(3_000, 2_000).longestWithin(limit))
        assertEquals(640, Size(2_000, 3_000).longestWithin(limit))
        assertEquals("a long one by its short side", 960, Size(4_000, 1_000).longestWithin(Size(1_000, 240)))
        assertEquals("not scaled up", 300, Size(300, 200).longestWithin(limit))
    }

    @Test
    fun sendsAPictureAtMostAsLargeAsPhonesShowAPictureMessages() {
        assertEquals(LARGEST_SIDE, Size(4_000, 3_000).sentWithin(Size(5_000, 5_000)))
        assertEquals("the carrier's, smaller", 640, Size(4_000, 3_000).sentWithin(Size(640, 480)))
        assertEquals("not scaled up", 300, Size(300, 200).sentWithin(Size(5_000, 5_000)))
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
        assertEquals("from the size its caller asks", 4_000 to 85, sides.first())
        assertTrue(sides.zipWithNext().all { (a, b) -> b.first < a.first || b.second < a.second })
        assertNull(fit(10, 4_000) { side, _ -> ByteArray(side) })
        assertEquals(listOf(300 to 85), tried(1_000_000, 300))
        assertEquals("larger than a picture message's, if it fits", listOf(4_096 to 85), tried(20 * 1024 * 1024, 4_096))
    }

    @Test
    fun triesASmallPictureAtItsOwnSize() {
        assertEquals(listOf(128 to 85), tried(1_000_000, 128))
        assertEquals("never smaller", listOf(128 to 85, 128 to 70), tried(10, 128))
    }

    @Test
    fun makesNoJpegWhenAndroidCant() {
        assertNull(fit(1_000_000, 300) { _, _ -> null })
    }

    /** Each side and quality [fit] tries, within [most] bytes from [longest], of a JPEG [side] bytes. */
    private fun tried(most: Int, longest: Int) = mutableListOf<Pair<Int, Int>>().also { tried ->
        fit(most, longest) { side, quality ->
            tried += side to quality
            ByteArray(side)
        }
    }

    private fun compose(most: Int, vararg files: Attachment, within: Size = Size(LARGEST_SIDE, LARGEST_SIDE)) =
        PictureMessage(pictures).compose("+14255550199", "Door code 4412", files.toList(), MmsLimits(most, within), "T1", 1)

    private fun file(name: String, mime: String, size: Int) = Attachment(name, mime, File(folder, name).apply { writeBytes(ByteArray(size)) })
}
