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

    // Android 8 and 9 name only the SIM the phone paired on and the default for texts.
    @Test
    fun sendsFromThePairedSimOnAndroid8And9() {
        assertEquals("the default changed after pairing", 1, subscription(mapOf(1 to "+12065550100", 2 to "+14255550100"), "+12065550100", default = 2))
        assertEquals("asking each time", 1, subscription(mapOf(1 to "+12065550100"), "+12065550100", default = -1))
        assertEquals("a SIM taken out, another the default", 2, subscription(mapOf(1 to "", 2 to "+14255550100"), "+12065550100", default = 2))
    }

    @Test
    fun relaysThePairedSimsTextsAloneWhileThePhoneHoldsSeveral() {
        val sims = mapOf(1 to "+14255550100", 2 to "(206) 555-0100")
        assertEquals(2, relayed(sims, "+12065550100", default = 1))
        assertEquals("a number Android didn't say", 1, relayed(sims, "", default = 1))
        assertNull("one SIM, every text, as a SIM swapped keeps them", relayed(mapOf(3 to "+14255550100"), "+12065550100", default = 3))
        assertNull("SIMs Android won't name", relayed(emptyMap(), "+12065550100", default = 3))
    }
}
