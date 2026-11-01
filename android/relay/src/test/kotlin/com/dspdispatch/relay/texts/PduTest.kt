package com.dspdispatch.relay.texts

import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class PduTest {
    @Test
    fun encodesASendRequest() {
        val pdu = Pdu.sendReq("+1 (425) 555-0199", "T1", 100_000_000, "smil.xml", listOf(PduPart("text/plain", "text.txt", "Hi".toByteArray())))
        assertArrayEquals(
            octets(
                0x8C, 0x80, // m-send-req
                0x98, "T1", 0, // its transaction
                0x8D, 0x92, // MMS 1.2
                0x85, 4, 0x05, 0xF5, 0xE1, 0x00, // its date, in seconds
                0x89, 1, 0x81, // from this phone, whose number Android fills in
                0x97, "+14255550199/TYPE=PLMN", 0,
                0x8A, 0x80, // personal
                0x88, 5, 0x81, 3, 0x09, 0x3A, 0x80, // kept a week
                0x8F, 0x81, // normal priority
                0x86, 0x81, // no delivery report
                0x90, 0x81, // no read report
                // multipart/related, from its layout: 31 octets, past what one octet holds.
                0x84, 31, 31, 0xB3, 0x8A, "<smil.xml>", 0, 0x89, "application/smil", 0,
                1, // one part
                47, 2, // its headers' octets and its own
                23, "text/plain", 0, 0x81, 0xEA, 0x85, "text.txt", 0, // in UTF-8, named
                0xC0, 0x22, "<text.txt>", 0, // its ID
                0x8E, "text.txt", 0, // its location
                "Hi",
            ),
            pdu,
        )
    }

    @Test
    fun writesAPartsLengthInAsManyOctetsAsItTakes() {
        val pdu = Pdu.sendReq("72166", "T1", 1, "smil.xml", listOf(PduPart("image/jpeg", "a.jpg", ByteArray(200))))
        val headers = octets(18, "image/jpeg", 0, 0x85, "a.jpg", 0, 0xC0, 0x22, "<a.jpg>", 0, 0x8E, "a.jpg", 0)
        val part = octets(headers.size) + octets(0x81, 0x48) + headers + ByteArray(200)
        assertArrayEquals(part, pdu.copyOfRange(pdu.size - part.size, pdu.size))
    }

    @Test
    fun readsTheCarriersAnswer() {
        val answer = octets(0x8C, 0x81, 0x98, "T1", 0, 0x8D, 0x92, 0x93, 3, 0xEA, "k", 0, 0x92, 0xE3, 0x8B, "id", 0)
        assertEquals(0xE3, Pdu.responseStatus(answer))
        assertEquals(Pdu.RESPONSE_OK, Pdu.responseStatus(octets(0x8C, 0x81, 0x98, "T1", 0, 0x92, 0x80)))
        assertNull(Pdu.responseStatus(octets(0x8C, 0x81, 0x98, "T1", 0)))
        assertNull(Pdu.responseStatus(ByteArray(0)))
    }

    private fun octets(vararg values: Any): ByteArray = values.flatMap { value ->
        when (value) {
            is Int -> listOf(value.toByte())
            is String -> value.toByteArray().toList()
            else -> error("not an octet: $value")
        }
    }.toByteArray()
}
