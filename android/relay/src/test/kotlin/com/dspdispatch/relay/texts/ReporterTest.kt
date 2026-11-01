package com.dspdispatch.relay.texts

import com.dspdispatch.relay.dispatch.Direction
import com.dspdispatch.relay.dispatch.Dispatch
import com.dspdispatch.relay.dispatch.Refusal
import com.dspdispatch.relay.dispatch.TextAnswer
import com.dspdispatch.relay.dispatch.TextsAnswer
import com.dspdispatch.relay.dispatch.TextsReport
import com.dspdispatch.relay.dispatch.wire
import kotlinx.coroutines.test.runTest
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
    private val reporter = Reporter(Dispatch(OkHttpClient()), queue, provider)
    private lateinit var site: String

    @Before
    fun start() {
        server.start()
        site = server.url("/").toString()
    }

    @After
    fun stop() = server.close()

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

    private suspend fun waiting(clientId: String, vararg files: WaitingFile, number: String = "(206) 555-0101") {
        val at = clientId.substringAfterLast('-').toLong()
        val body = if (files.isEmpty()) "Gate code?" else "Door"
        queue.read(Seen(clientId, at, Direction.IN), listOf(Waiting(clientId, Direction.IN, number, body, at)), files.toList())
    }

    private suspend fun answered(file: WaitingFile) = queue.read(Seen(file.clientId, 1, Direction.IN), emptyList(), listOf(file))

    private fun json(code: Int, body: String) =
        MockResponse.Builder().code(code).addHeader("Content-Type", "application/json").body(body).build()

    private companion object {
        val KEY = "k".repeat(64)
    }
}
