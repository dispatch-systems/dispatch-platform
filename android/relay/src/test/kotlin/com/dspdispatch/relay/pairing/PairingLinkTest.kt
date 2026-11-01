package com.dspdispatch.relay.pairing

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class PairingLinkTest {
    @Test
    fun readsTheAddressAndCodeAsDispatchWritesThem() {
        assertEquals(
            PairingLink("https://bh.dispatch.test", "K7Q2M9XA"),
            PairingLink.parse("dispatch-phone:pair?site=https://bh.dispatch.test&code=K7Q2M9XA"),
        )
        assertEquals(
            PairingLink("http://bh.localhost:5180", "K7Q2-M9XA"),
            PairingLink.parse("dispatch-phone:pair?code=K7Q2-M9XA&site=http%3A%2F%2Fbh.localhost%3A5180"),
        )
    }

    @Test
    fun readsBackWhatItWrites() {
        val link = PairingLink("http://bh.localhost:5180", "K7Q2 M9XA&x")
        assertEquals(link, PairingLink.parse(link.text))
    }

    @Test
    fun refusesWhatIsNotAPairingLink() {
        assertNull(PairingLink.parse("https://bh.dispatch.test"))
        assertNull(PairingLink.parse("dispatch-phone:pair?site=https://bh.dispatch.test"))
        assertNull(PairingLink.parse("dispatch-phone:pair?code=K7Q2M9XA"))
        assertNull(PairingLink.parse("dispatch-phone:pair?site=&code=K7Q2M9XA"))
        assertNull(PairingLink.parse("dispatch-phone:other?site=https://bh.dispatch.test&code=K7Q2M9XA"))
        assertNull(PairingLink.parse(""))
    }
}
