package com.dspdispatch.relay

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class ServerTest {
    private val production = Server(production = true, productionDomain = "dispatch.test")
    private val dev = Server(production = false, productionDomain = "dispatch.test")

    @Test
    fun productionPairsOnlyWithProductionsDsps() {
        assertTrue(production.accepts("https://bh.dispatch.test"))
        assertTrue(production.accepts("https://bh2.dispatch.test/"))
        assertTrue(production.accepts("https://BH.Dispatch.test"))
        assertFalse(production.accepts("https://bh.elsewhere.test"))
        assertFalse(production.accepts("https://bh.dispatch.test.elsewhere.test"))
    }

    @Test
    fun devPairsOnlyWithDspsElsewhere() {
        assertTrue(dev.accepts("https://bh.elsewhere.test"))
        assertTrue(dev.accepts("https://bh.dev.other.test"))
        assertFalse(dev.accepts("https://bh.dispatch.test"))
    }

    @Test
    fun neitherPairsWithWhatIsNotADspAddress() {
        for (server in listOf(production, dev)) {
            assertFalse(server.accepts("https://dispatch.test"))
            assertFalse(server.accepts("https://bh.com"))
            assertFalse(server.accepts("https://a.b.dispatch.test"))
            assertFalse(server.accepts("https://b.dispatch.test"))
            assertFalse(server.accepts("https://bh-2.dispatch.test"))
            assertFalse(server.accepts("http://bh.dispatch.test"))
            assertFalse(server.accepts("https://bh.dispatch.test:8443"))
            assertFalse(server.accepts("https://someone@bh.dispatch.test"))
            assertFalse(server.accepts("https://bh.dispatch.test/api"))
            assertFalse(server.accepts("https://bh.dispatch.test?x=1"))
            assertFalse(server.accepts("not an address"))
            assertFalse(server.accepts(""))
        }
    }
}
