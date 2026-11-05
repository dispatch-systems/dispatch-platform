package com.dspdispatch.relay.texts

import com.dspdispatch.relay.dispatch.AppVersion
import com.dspdispatch.relay.dispatch.Dispatch
import com.dspdispatch.relay.dispatch.Refusal
import kotlinx.coroutines.test.runTest
import mockwebserver3.Dispatcher
import mockwebserver3.MockResponse
import mockwebserver3.MockWebServer
import mockwebserver3.RecordedRequest
import okhttp3.OkHttpClient
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Assert.fail
import org.junit.Before
import org.junit.Test

class StatusesTest {
    private val server = MockWebServer()
    private val database = memoryDatabase()
    private val sends = database.sends()
    private val statuses = Statuses(Dispatch(OkHttpClient(), AppVersion("1.4.0", 14)), sends)
    private lateinit var site: String

    @Before
    fun start() {
        server.start()
        site = server.url("/").toString()
    }

    @After
    fun stop() {
        server.close()
        database.close()
    }

    @Test
    fun tellsDispatchHowEachAttemptWentUntilItAnswers() = runTest {
        val sent = sends.start(Send("text_1", 41, "+14255550199", "On my way", 1))
        sends.handed(sent, 1, false, 2)
        sends.heard(Heard("text_1", 41, 0, Event.SENT, null, null, 3))
        sends.start(Send("text_2", 7, "+14255550199", "Hi", 1)).let { sends.failed(it, "No service", 4) }

        server.enqueue(json(503, """{"error":"platform_busy"}"""))
        try {
            statuses.tell(site, KEY)
            fail("Not refused")
        } catch (refusal: Refusal) {
            assertTrue(refusal.passing)
        }
        assertEquals(2, sends.statuses(10).size)
        assertEquals(0, statuses.told.value)

        server.enqueue(json(200, """{"ok":true}"""))
        assertTrue(statuses.tell(site, KEY))
        server.takeRequest()
        val request = server.takeRequest()
        assertEquals("/api/communications/phone/statuses", request.url.encodedPath)
        assertEquals("Bearer $KEY", request.headers["Authorization"])
        assertEquals(site.trimEnd('/'), request.headers["Origin"])
        assertEquals(
            """{"statuses":[{"id":"text_1","attempt":41,"status":"sent","at":3},""" +
                """{"id":"text_2","attempt":7,"status":"failed","at":4,"error":"No service"}]}""",
            request.body?.utf8(),
        )
        assertEquals(1, statuses.told.value)
        assertFalse(statuses.tell(site, KEY))
    }

    @Test
    fun leavesAStatusDispatchRefusesAloneAndTellsTheRest() = runTest {
        for ((at, id) in listOf("text_1", "text_bad", "text_3").withIndex()) {
            sends.start(Send(id, 1, "+14255550199", "Hi", 1)).let { sends.failed(it, "No service", at.toLong()) }
        }
        val told = mutableListOf<String>()
        server.dispatcher = object : Dispatcher() {
            override fun dispatch(request: RecordedRequest): MockResponse {
                val ids = Regex(""""id":"(\w+)"""").findAll(request.body!!.utf8()).map { it.groupValues[1] }.toList()
                if ("text_bad" in ids) return json(400, """{"error":"invalid_input"}""")
                told += ids
                return json(200, """{"ok":true}""")
            }
        }
        while (statuses.tell(site, KEY)) continue
        assertEquals(listOf("text_1", "text_3"), told)
        assertTrue(sends.statuses(10).isEmpty())
    }

    private fun json(code: Int, body: String) =
        MockResponse.Builder().code(code).addHeader("Content-Type", "application/json").body(body).build()

    private companion object {
        val KEY = "k".repeat(64)
    }
}
