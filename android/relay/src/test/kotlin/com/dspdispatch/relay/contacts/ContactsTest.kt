package com.dspdispatch.relay.contacts

import com.dspdispatch.relay.dispatch.AppVersion
import com.dspdispatch.relay.dispatch.ContactsReport
import com.dspdispatch.relay.dispatch.Dispatch
import com.dspdispatch.relay.dispatch.NamedNumber
import com.dspdispatch.relay.dispatch.Outbox
import com.dspdispatch.relay.dispatch.SavedContact
import com.dspdispatch.relay.dispatch.SavedNumber
import com.dspdispatch.relay.pairing.Paired
import java.util.concurrent.TimeUnit
import kotlin.coroutines.EmptyCoroutineContext
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.emptyFlow
import kotlinx.coroutines.launch
import kotlinx.coroutines.test.runTest
import kotlinx.coroutines.withContext
import kotlinx.coroutines.yield
import mockwebserver3.MockResponse
import mockwebserver3.MockWebServer
import okhttp3.OkHttpClient
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test

class ContactsTest {
    private val server = MockWebServer()
    private val book = FakeBook()
    private val told = MemoryTold()
    private val keeping = MemoryKeeping()
    private val contacts = Contacts(Dispatch(OkHttpClient(), AppVersion("1.4.0", 14)), book, told, keeping, io = EmptyCoroutineContext)
    private lateinit var paired: Paired

    @Before
    fun start() {
        server.start()
        paired = Paired(server.url("/").toString(), "Northline", PHONE, "HR Line", 1)
    }

    @After
    fun stop() = server.close()

    @Test
    fun learnsFromEachWaitWhetherItsLineKeepsContacts() {
        assertFalse(contacts.heard(paired, Outbox()))
        assertFalse(contacts.kept(paired))
        assertTrue("news, as Set up's steps change", contacts.heard(paired, Outbox(contacts = emptyList())))
        assertTrue(contacts.kept(paired))
        assertFalse(contacts.heard(paired, Outbox(contacts = emptyList())))
        // Another pairing's line kept them, not this one's.
        assertFalse(contacts.kept(paired.copy(phoneId = "phone_2")))
        assertTrue(contacts.heard(paired, Outbox()))
        assertFalse(contacts.kept(paired))
    }

    @Test
    fun savesTheNamesHandedAndSaysSoNeverTellingThemBackAsThePhonesOwn() = runTest {
        contacts.heard(paired, outbox(SavedContact(DANA, "Dana Reyes", 5), SavedContact(SAM, "Sam", 6)))
        server.enqueue(ok())

        contacts.sync(paired, KEY)
        assertEquals(listOf(NamedNumber(DANA, "Dana Reyes"), NamedNumber(SAM, "Sam")), book.saved)
        val report = sent()
        assertEquals("/api/communications/phone/contacts", report.url.encodedPath)
        assertEquals("Bearer $KEY", report.headers["Authorization"])
        assertEquals("""{"saved":[{"number":"$DANA","at":5},{"number":"$SAM","at":6}]}""", report.body?.utf8())
        assertEquals(mapOf(DANA to "Dana Reyes", SAM to "Sam"), told.all(PHONE))

        // Said once: nothing more goes until the phone's contacts change, or another name is handed.
        contacts.sync(paired, KEY)
        assertEquals(1, server.requestCount)
        contacts.heard(paired, outbox(SavedContact(DANA, "Dana R", 9)))
        server.enqueue(ok())
        contacts.sync(paired, KEY)
        assertEquals("""{"saved":[{"number":"$DANA","at":9}]}""", sent().body?.utf8())
    }

    @Test
    fun tellsOnlyWhatChangedOfThePhonesOwnContacts() = runTest {
        told.told(PHONE, listOf(NamedNumber(DANA, "Dana"), NamedNumber(SAM, "Sam")), emptyList())
        told.told("phone_old", listOf(NamedNumber(KIM, "Kim")), emptyList())
        book.contacts = mapOf(DANA to "Dana Reyes", KIM to "Kim")
        server.enqueue(ok())

        contacts.sync(paired, KEY)
        assertEquals(
            """{"changed":[{"number":"$KIM","name":"Kim"},{"number":"$DANA","name":"Dana Reyes"}],"removed":["$SAM"]}""",
            sent().body?.utf8(),
        )
        assertEquals(mapOf(DANA to "Dana Reyes", KIM to "Kim"), told.all(PHONE))
        // Unchanged since, nothing is read or told.
        contacts.sync(paired, KEY)
        assertEquals(1, server.requestCount)
    }

    @Test
    fun keepsWhatWasntToldWhenDispatchDoesntAnswer() = runTest {
        contacts.heard(paired, outbox(SavedContact(DANA, "Dana", 5)))
        book.contacts = mapOf(SAM to "Sam")
        server.enqueue(MockResponse.Builder().code(503).body("""{"error":"platform_busy"}""").build())
        server.enqueue(ok())

        runCatching { contacts.sync(paired, KEY) }
        assertNull("Sam isn't told yet", told.all(PHONE)[SAM])
        contacts.sync(paired, KEY)
        sent()
        assertEquals(
            """{"saved":[{"number":"$DANA","at":5}],"changed":[{"number":"$SAM","name":"Sam"}]}""",
            sent().body?.utf8(),
        )
        assertEquals(mapOf(DANA to "Dana", SAM to "Sam"), told.all(PHONE))
    }

    @Test
    fun stopsOnceDispatchSaysItsLineKeepsNone() = runTest {
        contacts.heard(paired, outbox())
        book.contacts = mapOf(SAM to "Sam")
        server.enqueue(MockResponse.Builder().code(409).body("""{"error":"phones_contacts_not_kept"}""").build())

        val running = backgroundScope.launch { contacts.run(paired, KEY) }
        sent()
        while (contacts.kept(paired)) yield()
        running.cancel()
        assertEquals(1, server.requestCount)
    }

    @Test
    fun reportsInBatchesByCountAndSize() {
        val now = (1..450).associate { "+1425555${it.toString().padStart(4, '0')}" to "Name $it" }
        assertEquals(listOf(200, 200, 50), report(emptyMap(), now, emptyList()).map(ContactsReport::size))
        val long = (1..10).associate { "+1425555010$it" to "n".repeat(80) }
        assertEquals(listOf(2, 2, 2, 2, 2), report(emptyMap(), long, emptyList(), bytes = 300).map(ContactsReport::size))
        val saved = report(mapOf(SAM to "Sam"), emptyMap(), listOf(SavedNumber(DANA, 5)), most = 1)
        assertEquals(listOf(listOf(SavedNumber(DANA, 5)), emptyList()), saved.map(ContactsReport::saved))
        assertEquals(listOf(emptyList(), listOf(SAM)), saved.map(ContactsReport::removed))
    }

    @Test
    fun keepsANameAsDispatchDoes() {
        assertEquals("Dana Reyes", named("  Dana Reyes "))
        assertNull(named("   "))
        assertEquals("n".repeat(80), named("n".repeat(81)))
        // Cut by characters, never through one.
        assertEquals("😀".repeat(80), named("😀".repeat(81)))
    }

    private fun outbox(vararg saving: SavedContact) = Outbox(contacts = saving.toList())

    /** The next call Dispatch takes, waited for off the test's own thread. */
    private suspend fun sent() = withContext(Dispatchers.IO) { checkNotNull(server.takeRequest(10, TimeUnit.SECONDS)) }

    private fun ok() = MockResponse.Builder().code(200).addHeader("Content-Type", "application/json").body("""{"ok":true}""").build()

    /** The phone's contacts, as a test has them: always allowed. */
    private class FakeBook : Book {
        var contacts: Map<String, String> = emptyMap()
        val saved = mutableListOf<NamedNumber>()

        override fun allowed() = true

        override fun read() = contacts

        override fun save(contacts: List<NamedNumber>) {
            saved += contacts
            this.contacts += contacts.associate { it.number to it.name }
        }

        override fun changes(): Flow<Unit> = emptyFlow()
    }

    private class MemoryTold : Told {
        private val rows = mutableMapOf<Pair<String, String>, String>()

        override suspend fun all(phone: String) = rows.filterKeys { it.first == phone }.mapKeys { it.key.second }

        override suspend fun told(phone: String, changed: List<NamedNumber>, removed: List<String>) {
            changed.forEach { rows[phone to it.number] = it.name }
            removed.forEach { rows.remove(phone to it) }
        }

        override suspend fun only(phone: String) {
            rows.keys.removeAll { it.first != phone }
        }
    }

    private class MemoryKeeping : Keeping {
        private var phone: String? = null

        override fun read() = phone

        override fun write(phone: String?) {
            this.phone = phone
        }
    }

    private companion object {
        val KEY = "k".repeat(64)
        const val PHONE = "phone_1"
        const val DANA = "+14255550199"
        const val SAM = "+14255550123"
        const val KIM = "+14255550142"
    }
}
