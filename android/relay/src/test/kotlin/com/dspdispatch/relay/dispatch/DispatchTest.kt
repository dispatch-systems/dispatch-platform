package com.dspdispatch.relay.dispatch

import java.io.File
import kotlinx.coroutines.test.runTest
import mockwebserver3.MockResponse
import mockwebserver3.MockWebServer
import okhttp3.OkHttpClient
import okio.Buffer
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Assert.fail
import org.junit.Before
import org.junit.Test

class DispatchTest {
    private val server = MockWebServer()
    private val dispatch = Dispatch(OkHttpClient())
    private lateinit var site: String

    @Before
    fun start() {
        server.start()
        site = server.url("/").toString()
    }

    @After
    fun stop() = server.close()

    @Test
    fun pairsByTheCodeAndAnswersTheKey() = runTest {
        server.enqueue(json(200, """{"key":"${"a".repeat(64)}","phone":{"id":"phone_1","name":"Dispatch"},"dsp":{"name":"Blue Heron"},"new":1}"""))
        val answer = dispatch.pair(site, PairRequest("K7Q2M9XA", "+12065550100", "Pixel 8a", "0.1.0"))
        assertEquals("a".repeat(64), answer.key)
        assertEquals(PairedPhone("phone_1", "Dispatch"), answer.phone)
        assertEquals("Blue Heron", answer.dsp.name)
        val request = server.takeRequest()
        assertEquals("POST", request.method)
        assertEquals("/api/communications/phone/pair", request.url.encodedPath)
        assertEquals(
            """{"code":"K7Q2M9XA","number":"+12065550100","model":"Pixel 8a","version":"0.1.0"}""",
            request.body?.utf8(),
        )
        assertEquals(site.trimEnd('/'), request.headers["Origin"])
        assertNull(request.headers["Authorization"])
        assertNull(request.headers["Cookie"])
    }

    @Test
    fun refusesByDispatchsCode() = runTest {
        server.enqueue(json(401, """{"error":"phones_pairing_invalid","message":"That code isn't valid."}"""))
        val refusal = refused { dispatch.pair(site, PairRequest("K7Q2M9XA", "", "Pixel", "0.1.0")) }
        assertEquals(401, refusal.status)
        assertEquals(Refusal.PAIRING_INVALID, refusal.code)
        assertTrue(!refusal.passing && !refusal.removed)
    }

    @Test
    fun waitsOnTheOutboxWithTheKey() = runTest {
        server.enqueue(
            json(
                200,
                """{"texts":[{"id":"text_1","number":"+14255550199","body":"On my way","attempt":41,
                   "files":[{"id":"file_1","name":"door.jpg","mime":"image/jpeg","size":48213}]}]}""",
            ),
        )
        val outbox = dispatch.outbox(site, "k".repeat(64))
        assertEquals(
            listOf(OutgoingText("text_1", "+14255550199", "On my way", 41, listOf(OutgoingFile("file_1", "door.jpg", "image/jpeg", 48213)))),
            outbox.texts,
        )
        val request = server.takeRequest()
        assertEquals("GET", request.method)
        assertEquals("/api/communications/phone/outbox", request.url.encodedPath)
        assertEquals("Bearer ${"k".repeat(64)}", request.headers["Authorization"])
    }

    @Test
    fun knowsARemovedPhoneAndAPassingRefusal() = runTest {
        server.enqueue(json(401, """{"error":"phones_removed","message":"…"}"""))
        assertTrue(refused { dispatch.outbox(site, "k") }.removed)
        // Dispatch answers this while the DSP is suspended too: the key may work again.
        server.enqueue(json(401, """{"error":"sign_in_required","message":"…"}"""))
        assertFalse(refused { dispatch.outbox(site, "k") }.removed)
        server.enqueue(json(503, """{"error":"platform_busy","message":"…"}"""))
        assertTrue(refused { dispatch.outbox(site, "k") }.passing)
        server.enqueue(MockResponse.Builder().code(502).body("<html>Bad gateway</html>").build())
        assertEquals("http_502", refused { dispatch.outbox(site, "k") }.code)
    }

    @Test
    fun downloadsAPictureOfTheOutboxAsItComes() = runTest {
        val into = File.createTempFile("file", null).apply { deleteOnExit() }
        server.enqueue(MockResponse.Builder().code(200).body(Buffer().write(byteArrayOf(1, 2, 3))).build())
        dispatch.download(site, "k".repeat(64), "file_1", into)
        assertEquals(listOf<Byte>(1, 2, 3), into.readBytes().toList())
        val request = server.takeRequest()
        assertEquals("GET", request.method)
        assertEquals("/api/communications/phone/files/file_1", request.url.encodedPath)
        assertEquals("Bearer ${"k".repeat(64)}", request.headers["Authorization"])

        server.enqueue(json(404, """{"error":"phones_file_not_found","message":"…"}"""))
        assertEquals(Refusal.FILE_NOT_FOUND, refused { dispatch.download(site, "k", "file_2", into) }.code)
    }

    @Test
    fun reportsHowEachAttemptWent() = runTest {
        server.enqueue(json(200, """{"ok":true}"""))
        dispatch.statuses(site, "k", StatusesReport(listOf(ReportedStatus("text_1", 41, SendStatus.DELIVERED, 5))))
        val request = server.takeRequest()
        assertEquals("POST", request.method)
        assertEquals("/api/communications/phone/statuses", request.url.encodedPath)
        assertEquals("""{"statuses":[{"id":"text_1","attempt":41,"status":"delivered","at":5}]}""", request.body?.utf8())
    }

    private suspend fun refused(call: suspend () -> Unit): Refusal {
        try {
            call()
        } catch (refusal: Refusal) {
            return refusal
        }
        fail("Not refused")
        error("unreachable")
    }

    private fun json(code: Int, body: String) =
        MockResponse.Builder().code(code).addHeader("Content-Type", "application/json").body(body).build()
}
