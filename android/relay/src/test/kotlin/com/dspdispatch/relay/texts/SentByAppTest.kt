package com.dspdispatch.relay.texts

import com.dspdispatch.relay.dispatch.Direction
import kotlinx.coroutines.test.runTest
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Test

class SentByAppTest {
    private val database = memoryDatabase()
    private val sends = database.sends()
    private val own = SentByApp(sends, APP)

    @After
    fun close() = database.close()

    @Test
    fun knowsTheRowAndroidSaidItWroteASendTo() = runTest {
        val send = handed("text_1", "+14255550199", "On my way")
        sends.heard(Heard(send.id, send.attempt, 0, Event.SENT, null, "sms-7", AT + 1))
        assertEquals(Whose.DISPATCH, own.whose(sms(7, "(425) 555-0199", "On my way")))
        // Its own row known, it claims no other, as the phone's texting app sending the same.
        assertEquals(Whose.PHONE, own.whose(sms(8, "(425) 555-0199", "On my way", creator = TEXTING_APP)))
    }

    @Test
    fun takesTheProvidersWordThatTheAppSentIt() = runTest {
        assertEquals(Whose.DISPATCH, own.whose(sms(7, "(425) 555-0199", "Sent long ago", creator = APP)))
        assertEquals(Whose.PHONE, own.whose(sms(8, "(425) 555-0199", "Sent long ago", creator = TEXTING_APP)))
    }

    @Test
    fun matchesARowByItsNumberAndWordsWhenAndroidDidntSay() = runTest {
        val send = handed("text_1", "+14255550199", "On my way")
        sends.heard(Heard(send.id, send.attempt, 0, Event.SENT, null, null, AT + 1))
        assertEquals(Whose.DISPATCH, own.whose(sms(7, "(425) 555-0199", "On my way")))
        assertEquals("each send claims one row", Whose.PHONE, own.whose(sms(8, "(425) 555-0199", "On my way")))
        assertEquals(Whose.PHONE, own.whose(sms(9, "(425) 555-0198", "On my way")))
    }

    @Test
    fun waitsToKnowWhileATextToTheSameNumberIsUnderWay() = runTest {
        val send = handed("text_1", "+14255550199", "On my way")
        val texted = sms(7, "(425) 555-0199", "Typed on the phone")
        assertEquals(Whose.UNSURE, own.whose(texted))
        assertEquals(Whose.PHONE, own.whose(sms(8, "(425) 555-0198", "To someone else")))
        sends.heard(Heard(send.id, send.attempt, 0, Event.SENT, null, "sms-6", AT + 1))
        assertEquals(Whose.PHONE, own.whose(texted))
    }

    @Test
    fun knowsAPictureMessageByItsRecipient() = runTest {
        handed("text_1", "+14255550199", "Door")
        val row = Row(Table.MMS, 3, AT / 1_000 * 1_000, Direction.OUT)
        assertEquals(Whose.DISPATCH, own.whose(Message(row, listOf("+14255550199/TYPE=PLMN"), "Door", emptyList())))
    }

    private suspend fun handed(id: String, number: String, body: String): Send {
        val send = sends.start(Send(id, 1, number, body, AT))
        sends.handed(send, 1, AT)
        return send
    }

    private fun sms(id: Long, number: String, body: String, creator: String? = null) =
        Message(Row(Table.SMS, id, AT + 2, Direction.OUT, creator), listOf(number), body, emptyList())

    private companion object {
        const val AT = 1_793_041_515_123L
        const val APP = "com.dspdispatch.relay"
        const val TEXTING_APP = "com.example.texting"
    }
}
