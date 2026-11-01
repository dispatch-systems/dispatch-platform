package com.dspdispatch.relay.texts

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class RadioTest {
    @Test
    fun sendsFromTheSimThePhonePairedWith() {
        val sims = mapOf(1 to "+14255550100", 2 to "(206) 555-0100")
        assertEquals(2, subscription(sims, "+12065550100", default = 1))
        assertEquals("a number no SIM has", 1, subscription(sims, "+13605550100", default = 1))
        assertEquals("a number Android didn't say", 1, subscription(sims, "", default = 1))
        assertEquals("no default, as when Android asks each time", 1, subscription(sims, "", default = -1))
        assertEquals("SIMs Android won't name", 3, subscription(emptyMap(), "+12065550100", default = 3))
        assertNull(subscription(emptyMap(), "+12065550100", default = -1))
    }
}
