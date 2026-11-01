package com.dspdispatch.relay.texts

import com.dspdispatch.relay.dispatch.Direction
import com.dspdispatch.relay.dispatch.ReportedFile
import com.dspdispatch.relay.dispatch.wire
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class BatchTest {
    @Test
    fun carriesAtMostAHundredTexts() {
        assertEquals(100, batch(texts(150, "Gate code?")).texts.size)
    }

    @Test
    fun carriesAtMostWhatACallMay() {
        val report = batch(texts(20, "a".repeat(10_000)))
        assertEquals(6, report.texts.size)
        assertTrue(wire.encodeToString(report).toByteArray().size <= 64 * 1024)
    }

    @Test
    fun carriesAToolLargeTextAloneForDispatchToRefuse() {
        assertEquals(1, batch(texts(2, "\u0001".repeat(11_000))).texts.size)
    }

    @Test
    fun reportsFilesInTheirOrder() {
        val text = Waiting("mms-1-1", Direction.IN, "(206) 555-0101", "", 1)
        val files = listOf(1, 0).map { WaitingFile(text.clientId, it, "content://mms/part/$it", "p$it.jpg", "image/jpeg", 5) }
        assertEquals(
            listOf(ReportedFile("p0.jpg", "image/jpeg", 5), ReportedFile("p1.jpg", "image/jpeg", 5)),
            batch(listOf(WaitingText(text, files))).texts.single().files,
        )
    }

    private fun texts(count: Int, body: String) =
        (1..count).map { WaitingText(Waiting("sms-$it-$it", Direction.IN, "(206) 555-0101", body, it.toLong()), emptyList()) }
}
