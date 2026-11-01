package com.dspdispatch.relay.texts

import com.dspdispatch.relay.dispatch.Direction
import kotlinx.coroutines.test.runTest
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class OwnRowsTest {
    private val database = memoryDatabase()
    private val sends = database.sends()
    private val provider = FakeProvider()
    private val own = OwnRows(provider, sends, APP)

    @After
    fun close() = database.close()

    @Test
    fun knowsTheRowASendsResultNames() = runTest {
        val send = handed("text_1", "On my way")
        val sent = sms(7, "On my way", APP)
        provider.messages += sent
        assertEquals(sent.row.key, own.row(heard(send), "content://sms/sent/7"))
        assertNull("gone", own.row(heard(send), "content://sms/sent/8"))
    }

    @Test
    fun leavesARowWhoseIdWasUsedAgainBeforeTheResultWasHeard() = runTest {
        val send = handed("text_1", "On my way")
        // The app's row deleted, the provider gives its ID to a text typed on the phone, before
        // Android's word of the send reaches the app.
        val typed = Message(Row(Table.SMS, 7, AT + 60_000, Direction.OUT, TEXTING_APP), listOf("(425) 555-0198"), "Gate code?", emptyList())
        provider.messages += typed
        val heard = heard(send)
        sends.heard(heard.copy(row = own.row(heard, "content://sms/sent/7")))
        assertEquals(Whose.PHONE, SentByApp(sends, APP).whose(typed))
    }

    @Test
    fun takesARowNamingNoAppByItsNumberAndWords() = runTest {
        val send = handed("text_1", "On my way")
        provider.messages += listOf(sms(7, "On my way"), sms(8, "Typed on the phone"), sms(9, "On my way", TEXTING_APP))
        assertEquals(listOf("sms-7-${AT + 2}", null, null), (7..9).map { own.row(heard(send), "content://sms/sent/$it") })
        provider.messages += Message(Row(Table.SMS, 10, AT - 120_000, Direction.OUT, APP), listOf("+14255550199"), "On my way", emptyList())
        assertNull("written before it was handed", own.row(heard(send), "content://sms/sent/10"))
    }

    private suspend fun handed(id: String, body: String): Send {
        val send = sends.start(Send(id, 1, "+14255550199", body, AT))
        sends.handed(send, 1, false, AT)
        return checkNotNull(sends.find(id, 1))
    }

    private fun heard(send: Send) = Heard(send.id, send.attempt, 0, Event.SENT, null, null, AT + 61_000)

    private fun sms(id: Long, body: String, creator: String? = null) =
        Message(Row(Table.SMS, id, AT + 2, Direction.OUT, creator), listOf("(425) 555-0199"), body, emptyList())

    private companion object {
        const val AT = 1_793_041_515_123L
        const val APP = "com.dspdispatch.relay"
        const val TEXTING_APP = "com.example.texting"
    }
}
