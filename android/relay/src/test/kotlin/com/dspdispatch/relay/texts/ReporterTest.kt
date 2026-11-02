package com.dspdispatch.relay.texts

import com.dspdispatch.relay.dispatch.Direction
import com.dspdispatch.relay.dispatch.AppVersion
import com.dspdispatch.relay.dispatch.Dispatch
import com.dspdispatch.relay.dispatch.Refusal
import com.dspdispatch.relay.dispatch.TextAnswer
import com.dspdispatch.relay.dispatch.TextsAnswer
import com.dspdispatch.relay.dispatch.TextsReport
import com.dspdispatch.relay.dispatch.wire
import java.io.ByteArrayInputStream
import java.io.File
import java.net.URI
import java.nio.file.Files
import java.util.concurrent.TimeUnit
import kotlin.coroutines.EmptyCoroutineContext
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.coroutines.test.runTest
import kotlinx.coroutines.withContext
import mockwebserver3.Dispatcher
import mockwebserver3.MockResponse
import mockwebserver3.MockWebServer
import mockwebserver3.RecordedRequest
import okhttp3.OkHttpClient
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Assert.fail
import org.junit.Before
import org.junit.Test

class ReporterTest {
    private val server = MockWebServer()
    private val queue = MemoryQueue()
    private val provider = FakeProvider()
    private val folder = Files.createTempDirectory("jpegs").toFile()
    private val pictures = FakePictures(jpeg = { JPEG })
    private val reporter = reporter(pictures)
    private lateinit var site: String

    @Before
    fun start() {
        server.start()
        site = server.url("/").toString()
    }

    @After
    fun stop() {
        server.close()
        folder.deleteRecursively()
    }

    @Test
    fun reportsTextsThenSendsEachFilesBytes() = runTest {
        waiting("mms-1-1", WaitingFile("mms-1-1", 0, "content://mms/part/1", "door.jpg", "image/jpeg", 3))
        provider.bytes["content://mms/part/1"] = byteArrayOf(1, 2, 3, 4)
        server.enqueue(json(200, """{"texts":[{"clientId":"mms-1-1","id":"text_1","files":["file_1"]}]}"""))
        server.enqueue(json(200, """{"ok":true}"""))

        assertTrue(reporter.report(site, KEY))
        val report = server.takeRequest()
        assertEquals("/api/communications/phone/texts", report.url.encodedPath)
        assertEquals("Bearer $KEY", report.headers["Authorization"])
        assertEquals(site.trimEnd('/'), report.headers["Origin"])
        assertEquals(
            """{"texts":[{"clientId":"mms-1-1","direction":"in","number":"(206) 555-0101","body":"Door","at":1,""" +
                """"files":[{"name":"door.jpg","mime":"image/jpeg","size":3}]}]}""",
            report.body?.utf8(),
        )
        assertTrue(queue.texts.isEmpty())
        assertFalse(reporter.report(site, KEY))

        assertTrue(reporter.upload(site, KEY))
        val upload = server.takeRequest()
        assertEquals("/api/communications/phone/files/file_1", upload.url.encodedPath)
        assertEquals("application/octet-stream", upload.headers["Content-Type"])
        // As many bytes as the part holds now, whatever it held as it was read.
        assertEquals("4", upload.headers["Content-Length"])
        assertEquals(listOf<Byte>(1, 2, 3, 4), upload.body?.toByteArray()?.toList())
        assertTrue(queue.files.isEmpty())
        assertFalse(reporter.upload(site, KEY))
    }

    @Test
    fun keepsAFileWhileDispatchIsBusy() = runTest {
        answered(WaitingFile("mms-1-1", 0, "content://mms/part/1", "door.jpg", "image/jpeg", 3, "file_1"))
        provider.bytes["content://mms/part/1"] = byteArrayOf(1, 2, 3)
        server.enqueue(json(503, """{"error":"uploads_busy"}"""))
        try {
            reporter.upload(site, KEY)
            fail("Not refused")
        } catch (refusal: Refusal) {
            assertTrue(refusal.passing)
        }
        assertEquals(1, queue.files.size)
    }

    @Test
    fun keepsAFileWhileTheProviderIsAway() = runTest {
        answered(WaitingFile("mms-1-1", 0, "content://mms/part/1", "door.jpg", "image/jpeg", 3, "file_1"))
        provider.bytes["content://mms/part/1"] = byteArrayOf(1, 2, 3)
        provider.away += "content://mms/part/1"
        try {
            reporter.upload(site, KEY)
            fail("Read")
        } catch (_: Unavailable) {
        }
        assertEquals(0, server.requestCount)
        assertEquals(1, queue.files.size)

        server.enqueue(json(200, """{"ok":true}"""))
        assertTrue(reporter.upload(site, KEY))
        assertEquals("/api/communications/phone/files/file_1", server.takeRequest().url.encodedPath)
        assertTrue(queue.files.isEmpty())
    }

    @Test
    fun leavesAFileGoneFromThePhoneOrFromDispatch() = runTest {
        answered(WaitingFile("mms-1-1", 0, "content://mms/part/1", "gone.jpg", "image/jpeg", 3, "file_1"))
        answered(WaitingFile("mms-2-2", 0, "content://mms/part/2", "door.jpg", "image/jpeg", 3, "file_2"))
        provider.bytes["content://mms/part/2"] = byteArrayOf(1, 2, 3)
        server.enqueue(json(404, """{"error":"phones_file_not_found"}"""))
        assertTrue(reporter.upload(site, KEY))
        assertTrue(reporter.upload(site, KEY))
        assertEquals("/api/communications/phone/files/file_2", server.takeRequest().url.encodedPath)
        assertTrue(queue.files.isEmpty())
    }

    @Test
    fun waitsForTextingAfterARestartKeepingItsFile() = runTest {
        // As the relay starts again: Dispatch answered the text, its file waits, Texting is off.
        answered(WaitingFile("mms-1-1", 0, "content://mms/part/1", "door.jpg", "image/jpeg", 3, "file_1"))
        provider.bytes["content://mms/part/1"] = byteArrayOf(1, 2, 3)
        provider.allowed = false
        server.enqueue(json(200, """{"ok":true}"""))
        backgroundScope.launch { reporter.run(site, KEY, Channel()) }
        delay(HOUR_MS)
        assertEquals(0, provider.refused)
        assertEquals(0, server.requestCount)
        assertEquals(1, queue.files.size)

        provider.allowed = true
        assertEquals("/api/communications/phone/files/file_1", sent()?.url?.encodedPath)
    }

    @Test
    fun waitsForTextingTurnedOffAsAFileIsRead() = runTest {
        answered(WaitingFile("mms-1-1", 0, "content://mms/part/1", "door.jpg", "image/jpeg", 3, "file_1"))
        provider.bytes["content://mms/part/1"] = byteArrayOf(1, 2, 3)
        provider.turnedOffAtRead = true
        server.enqueue(json(200, """{"ok":true}"""))
        backgroundScope.launch { reporter.run(site, KEY, Channel()) }
        delay(HOUR_MS)
        assertEquals(1, provider.refused)
        assertEquals(0, server.requestCount)
        assertEquals(1, queue.files.size)

        provider.allowed = true
        assertEquals(listOf<Byte>(1, 2, 3), sent()?.body?.toByteArray()?.toList())
    }

    @Test
    fun leavesATextDispatchRefusesAloneAndReportsTheRest() = runTest {
        (1..5).forEach { waiting("sms-$it-$it", number = if (it == 4) "Carrier" else "(206) 555-0101") }
        server.dispatcher = object : Dispatcher() {
            override fun dispatch(request: RecordedRequest): MockResponse {
                val report = wire.decodeFromString<TextsReport>(request.body!!.utf8())
                if (report.texts.any { it.number == "Carrier" }) return json(400, """{"error":"invalid_input"}""")
                return json(200, wire.encodeToString(TextsAnswer(report.texts.map { TextAnswer(it.clientId, "id_${it.clientId}") })))
            }
        }
        var reports = 0
        while (reporter.report(site, KEY)) reports++
        // 1-5 refused; 1-2; 3-5 refused; 3; 4-5 refused; 4 refused, and left; 5.
        assertEquals(7, reports)
        assertTrue(queue.texts.isEmpty())
    }

    @Test
    fun keepsTextsWhileDispatchCantTakeThem() = runTest {
        waiting("sms-1-1")
        server.enqueue(json(401, """{"error":"sign_in_required"}"""))
        try {
            reporter.report(site, KEY)
            fail("Not refused")
        } catch (refusal: Refusal) {
            assertEquals(Refusal.SIGN_IN_REQUIRED, refusal.code)
        }
        assertEquals(setOf("sms-1-1"), queue.texts.keys)
        assertNull(queue.upload())
    }

    @Test
    fun reportsAHeifPictureAsAJpegMadeOfItThenSendsTheJpeg() = runTest {
        waiting("mms-1-1", WaitingFile("mms-1-1", 0, "content://mms/part/1", "IMG_0001.HEIC", "image/heic", 4))
        provider.bytes["content://mms/part/1"] = byteArrayOf(1, 2, 3, 4)
        server.enqueue(json(503, """{"error":"service_unavailable"}"""))
        server.enqueue(json(200, """{"texts":[{"clientId":"mms-1-1","id":"text_1","files":["file_1"]}]}"""))
        server.enqueue(json(200, """{"ok":true}"""))

        try {
            reporter.report(site, KEY)
            fail("Not refused")
        } catch (_: Refusal) {
        }
        assertTrue(reporter.report(site, KEY))
        server.takeRequest()
        assertEquals(
            """{"texts":[{"clientId":"mms-1-1","direction":"in","number":"(206) 555-0101","body":"Door","at":1,""" +
                """"files":[{"name":"IMG_0001.jpg","mime":"image/jpeg","size":3}]}]}""",
            server.takeRequest().body?.utf8(),
        )
        // Made once, as it was first reported, and kept for each report after.
        assertEquals(listOf(listOf<Byte>(1, 2, 3, 4)), pictures.made.map { it.first })

        assertTrue(reporter.upload(site, KEY))
        val upload = server.takeRequest()
        assertEquals("/api/communications/phone/files/file_1", upload.url.encodedPath)
        assertEquals(JPEG.toList(), upload.body?.toByteArray()?.toList())
        assertTrue(queue.files.isEmpty())
        assertEquals(0, folder.list()!!.size)
    }

    @Test
    fun sendsAHeifPictureAndroidCantDecodeAsItIs() = runTest {
        sendsTheHeifPictureAsItIs(FakePictures(jpeg = { null }))
    }

    @Test
    fun sendsAHeifPictureThePhoneHasntTheMemoryForAsItIs() = runTest {
        sendsTheHeifPictureAsItIs(FakePictures(jpeg = { throw OutOfMemoryError() }))
    }

    @Test
    fun sendsAHeifPictureAsItIsBeforeAndroid9() = runTest {
        val pictures = FakePictures(decodesHeif = false, jpeg = { JPEG })
        sendsTheHeifPictureAsItIs(pictures)
        assertTrue(pictures.made.isEmpty())
    }

    @Test
    fun keepsAPictureAsItWasFirstReportedThoughAJpegCanBeMadeOfItLater() = runTest {
        // Where the JPEG goes, something that can't be written to, as storage that's full.
        val full = File(folder, "jpegs").apply { writeText("") }
        val reporter = reporter(pictures, full)
        waiting("mms-1-1", WaitingFile("mms-1-1", 0, "content://mms/part/1", "IMG_0001.HEIC", "image/heic", 4))
        provider.bytes["content://mms/part/1"] = byteArrayOf(1, 2, 3, 4)
        // Dispatch keeps the first report, but its answer is lost.
        server.enqueue(json(503, """{"error":"service_unavailable"}"""))
        server.enqueue(json(200, """{"texts":[{"clientId":"mms-1-1","id":"text_1","files":["file_1"]}]}"""))
        server.enqueue(json(200, """{"ok":true}"""))

        try {
            reporter.report(site, KEY)
            fail("Not refused")
        } catch (_: Refusal) {
        }
        full.delete()
        assertTrue(reporter.report(site, KEY))
        repeat(2) { assertTrue(HEIC in server.takeRequest().body!!.utf8()) }
        assertEquals(1, pictures.made.size)
        assertTrue(reporter.upload(site, KEY))
        assertEquals(listOf<Byte>(1, 2, 3, 4), server.takeRequest().body?.toByteArray()?.toList())
        assertEquals(0, folder.list()!!.size)
    }

    @Test
    fun keepsEachFileAsItWasFirstReportedAcrossARestart() = runTest {
        val texts = Files.createTempDirectory("texts").toFile()
        try {
            val file = File(texts, TextsDatabase.NAME)
            // The second picture Android can't decode at first, as the phone's short of memory.
            var decodes = false
            val pictures = FakePictures(jpeg = { picture -> JPEG.takeIf { decodes || picture[0] == 1.toByte() } })
            provider.bytes["content://mms/part/1"] = byteArrayOf(1, 2, 3, 4)
            provider.bytes["content://mms/part/2"] = byteArrayOf(9, 8, 7, 6)
            server.enqueue(json(503, """{"error":"service_unavailable"}"""))
            server.enqueue(json(200, """{"texts":[{"clientId":"mms-1-1","id":"text_1","files":["file_1","file_2"]}]}"""))
            server.enqueue(json(200, """{"ok":true}"""))
            server.enqueue(json(200, """{"ok":true}"""))

            fileDatabase(file).run {
                waiting(
                    "mms-1-1",
                    WaitingFile("mms-1-1", 0, "content://mms/part/1", "IMG_0001.HEIC", "image/heic", 4),
                    WaitingFile("mms-1-1", 1, "content://mms/part/2", "IMG_0002.HEIC", "image/heic", 4),
                    queue = queue(),
                )
                try {
                    reporter(pictures, queue = queue()).report(site, KEY)
                    fail("Not refused")
                } catch (_: Refusal) {
                }
                close()
            }
            decodes = true
            fileDatabase(file).run {
                val reporter = reporter(pictures, queue = queue())
                assertTrue(reporter.report(site, KEY))
                val files = """"files":[{"name":"IMG_0001.jpg","mime":"image/jpeg","size":3},{"name":"IMG_0002.HEIC","mime":"image/heic","size":4}]"""
                repeat(2) { assertTrue(files in server.takeRequest().body!!.utf8()) }
                assertEquals("each made once, before the restart", 2, pictures.made.size)
                assertTrue(reporter.upload(site, KEY) && reporter.upload(site, KEY))
                assertEquals(JPEG.toList(), server.takeRequest().body?.toByteArray()?.toList())
                assertEquals(listOf<Byte>(9, 8, 7, 6), server.takeRequest().body?.toByteArray()?.toList())
                close()
            }
            assertEquals(0, folder.list()!!.size)
        } finally {
            texts.deleteRecursively()
        }
    }

    @Test
    fun sendsAPictureAnEarlierAppKeptAsItIs() = runTest {
        // Reported by the earlier app as it is, its answer lost before the app was updated.
        val texts = Files.createTempDirectory("texts").toFile()
        try {
            val file = File(texts, TextsDatabase.NAME)
            earlierDatabase(
                file,
                1,
                "INSERT INTO texts VALUES ('mms-1-1', 'IN', '(206) 555-0101', 'Door', 1)",
                "INSERT INTO files VALUES ('mms-1-1', 0, 'content://mms/part/1', 'IMG_0001.HEIC', 'image/heic', 4, NULL, 1)",
            )
            provider.bytes["content://mms/part/1"] = byteArrayOf(1, 2, 3, 4)
            server.enqueue(json(200, """{"texts":[{"clientId":"mms-1-1","id":"text_1","files":["file_1"]}]}"""))
            server.enqueue(json(200, """{"ok":true}"""))

            fileDatabase(file).run {
                val reporter = reporter(pictures, queue = queue())
                assertTrue(reporter.report(site, KEY))
                assertTrue(HEIC in server.takeRequest().body!!.utf8())
                assertTrue(reporter.upload(site, KEY))
                assertEquals(listOf<Byte>(1, 2, 3, 4), server.takeRequest().body?.toByteArray()?.toList())
                close()
            }
            assertTrue(pictures.made.isEmpty())
        } finally {
            texts.deleteRecursively()
        }
    }

    @Test
    fun keepsAFileAsItWasFirstReportedThoughDispatchRefusesTheReport() = runTest {
        waiting("mms-1-1", WaitingFile("mms-1-1", 0, "content://mms/part/1", "IMG_0001.HEIC", "image/heic", 4))
        waiting("mms-2-2", WaitingFile("mms-2-2", 0, "content://mms/part/2", "IMG_0002.HEIC", "image/heic", 4))
        provider.bytes["content://mms/part/1"] = byteArrayOf(1, 2, 3, 4)
        provider.bytes["content://mms/part/2"] = byteArrayOf(5, 6, 7, 8)
        // Refused together, then the first alone, which leaves it.
        server.enqueue(json(400, """{"error":"invalid_input"}"""))
        server.enqueue(json(400, """{"error":"invalid_input"}"""))
        server.enqueue(json(200, """{"texts":[{"clientId":"mms-2-2","id":"text_2","files":["file_2"]}]}"""))

        repeat(3) { assertTrue(reporter.report(site, KEY)) }
        assertTrue(server.takeRequest().body!!.utf8().let { "IMG_0001.jpg" in it && "IMG_0002.jpg" in it })
        assertTrue("IMG_0001.jpg" in server.takeRequest().body!!.utf8())
        assertTrue(""""files":[{"name":"IMG_0002.jpg","mime":"image/jpeg","size":3}]""" in server.takeRequest().body!!.utf8())
        assertEquals("each made once", listOf(listOf<Byte>(1, 2, 3, 4), listOf<Byte>(5, 6, 7, 8)), pictures.made.map { it.first })
        assertEquals(listOf(queue.files.single().uri), folder.listFiles()!!.map { it.toURI().toString() })
    }

    @Test
    fun letsGoOfTheJpegOfATextDispatchRefuses() = runTest {
        waiting("mms-1-1", WaitingFile("mms-1-1", 0, "content://mms/part/1", "IMG_0001.HEIC", "image/heic", 4))
        provider.bytes["content://mms/part/1"] = byteArrayOf(1, 2, 3, 4)
        server.enqueue(json(400, """{"error":"invalid_input"}"""))
        assertTrue(reporter.report(site, KEY))
        assertTrue(queue.texts.isEmpty())
        assertEquals(0, folder.list()!!.size)
    }

    @Test
    fun letsGoOfJpegsOfNoFileWaitingAsItStarts() = runTest {
        val jpegs = Jpegs(folder, pictures)
        val kept = jpegs.of(WaitingFile("mms-1-1", 0, "content://mms/part/1", "a.heic", "image/heic", 4, id = 1)) { ByteArrayInputStream(byteArrayOf(1)) }!!
        val left = jpegs.of(WaitingFile("mms-2-2", 0, "content://mms/part/2", "b.heic", "image/heic", 4, id = 2)) { ByteArrayInputStream(byteArrayOf(1)) }!!
        waiting("mms-1-1", kept)

        backgroundScope.launch { reporter.run(site, KEY, Channel()) }
        server.enqueue(json(200, """{"texts":[{"clientId":"mms-1-1","id":"text_1","files":["file_1"]}]}"""))
        assertEquals("/api/communications/phone/texts", sent()?.url?.encodedPath)
        assertFalse(File(URI(left.uri)).exists())
        assertTrue(File(URI(queue.files.single().uri)).exists())
    }

    /** Reports a HEIF picture as it is, and sends its bytes as they are, as [pictures] makes no JPEG of it. */
    private suspend fun sendsTheHeifPictureAsItIs(pictures: FakePictures) {
        val reporter = reporter(pictures)
        waiting("mms-1-1", WaitingFile("mms-1-1", 0, "content://mms/part/1", "IMG_0001.HEIC", "image/heic", 4))
        provider.bytes["content://mms/part/1"] = byteArrayOf(1, 2, 3, 4)
        server.enqueue(json(200, """{"texts":[{"clientId":"mms-1-1","id":"text_1","files":["file_1"]}]}"""))
        server.enqueue(json(200, """{"ok":true}"""))

        assertTrue(reporter.report(site, KEY))
        assertTrue(""""files":[$HEIC]""" in server.takeRequest().body!!.utf8())
        assertTrue(reporter.upload(site, KEY))
        assertEquals(listOf<Byte>(1, 2, 3, 4), server.takeRequest().body?.toByteArray()?.toList())
        assertEquals(0, folder.list()?.size ?: 0)
    }

    private fun reporter(pictures: Pictures, jpegs: File = folder, queue: Queue = this.queue) = Reporter(
        Dispatch(OkHttpClient(), AppVersion("1.4.0", 14)),
        queue,
        provider,
        Texting { provider.allowed },
        Jpegs(jpegs, pictures),
        EmptyCoroutineContext,
    )

    private suspend fun waiting(clientId: String, vararg files: WaitingFile, number: String = "(206) 555-0101", queue: Queue = this.queue) {
        val at = clientId.substringAfterLast('-').toLong()
        val body = if (files.isEmpty()) "Gate code?" else "Door"
        queue.read(Seen(clientId, at, Direction.IN), listOf(Waiting(clientId, Direction.IN, number, body, at)), files.toList())
    }

    private suspend fun answered(file: WaitingFile) = queue.read(Seen(file.clientId, 1, Direction.IN), emptyList(), listOf(file))

    /** The next call Dispatch takes, waited for off the test's own thread, which the reporter runs on. */
    private suspend fun sent() = withContext(Dispatchers.IO) { server.takeRequest(10, TimeUnit.SECONDS) }

    private fun json(code: Int, body: String) =
        MockResponse.Builder().code(code).addHeader("Content-Type", "application/json").body(body).build()

    private companion object {
        val KEY = "k".repeat(64)

        /** A JPEG made of a picture: how one begins. */
        val JPEG = byteArrayOf(-1, -40, -1)

        /** The HEIF picture `content://mms/part/1` holds, as it's reported as it is. */
        const val HEIC = """{"name":"IMG_0001.HEIC","mime":"image/heic","size":4}"""
        const val HOUR_MS = 60 * 60 * 1000L
    }
}
