package com.dspdispatch.relay.texts

import com.dspdispatch.relay.dispatch.Direction
import kotlin.coroutines.EmptyCoroutineContext
import kotlinx.coroutines.awaitCancellation
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.launch
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class ReaderTest {
    private val provider = FakeProvider()
    private val queue = MemoryQueue()
    private val inbox = Inbox(provider, queue, { Whose.PHONE }) { PAIRED + HOUR_MS }

    // The provider says it changed once, as the reader starts, and never again.
    private val reader = Reader(inbox, Texting { true }, flow { emit(Unit); awaitCancellation() }, EmptyCoroutineContext)

    @Test
    fun readsAPictureMessageAgainUntilItsWholeWithoutAWordFromTheProvider() = runTest {
        inbox.begin("phone_1", PAIRED)
        // Its row is written first, its parts and addresses after.
        val row = Row(Table.MMS, 5, PAIRED + 10, Direction.IN)
        provider.writing += row
        backgroundScope.launch { reader.run {} }
        delay(Reader.SETTLE_MS + 1)
        assertTrue(queue.texts.isEmpty())
        assertEquals(setOf(row.key), queue.unread.keys)

        provider.writing.clear()
        provider.messages += Message(row, listOf("(206) 555-0105"), "Door", listOf(Part("content://mms/part/9", "door.jpg", "image/jpeg", 3)))
        delay(MINUTE_MS)
        assertEquals(setOf(row.key), queue.texts.keys)
        assertTrue(queue.unread.isEmpty())
    }

    @Test
    fun readsAgainWithoutAWordFromTheProvider() = runTest {
        inbox.begin("phone_1", PAIRED)
        backgroundScope.launch { reader.run {} }
        delay(Reader.SETTLE_MS + 1)
        val row = Row(Table.SMS, 2, PAIRED + 10, Direction.IN)
        provider.messages += Message(row, listOf("(206) 555-0102"), "Gate code?", emptyList())
        delay(Reader.RECONCILE_MS + Reader.SETTLE_MS)
        assertEquals(setOf(row.key), queue.texts.keys)
    }

    private companion object {
        const val PAIRED = 1_793_041_515_000L
        const val MINUTE_MS = 60 * 1000L
        const val HOUR_MS = 60 * MINUTE_MS
    }
}
