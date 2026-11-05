package com.dspdispatch.relay.pairing

import java.security.KeyStoreException
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class PairingsTest {
    private val paired = Paired("https://blu.dispatch.test", "Blue Heron", "phone_1", "Dispatch", 1)
    private val kept = MemoryKept()

    /** The keystore: away for [away] more opens, then answering [opened]. */
    private var away = 0
    private var opened: String? = null
    private val vault = object : Locks {
        override fun seal(secret: String) = "sealed $secret"

        override fun open(sealed: String): String? {
            if (away-- > 0) throw Vault.Unreachable(KeyStoreException("System error"))
            return opened
        }
    }

    @Test
    fun forgetsAPairingWhoseKeyTurnsOutLostOnceTheKeystoreAnswersForThePhoneToPairAgain() = runTest {
        Pairings(kept, vault).save(paired, "the key")
        away = 3
        val pairings = Pairings(kept, vault)
        // Away as the app starts: still paired, its key read again.
        assertEquals(Standing.Pairing(paired), pairings.standing.value)
        assertNull(pairings.key())
        assertEquals(Standing.Unpaired, pairings.standing.value)
        assertEquals(Standing.Unpaired, Pairings(kept, vault).standing.value)

        val again = paired.copy(phoneId = "phone_2")
        opened = "a new key"
        pairings.save(again, "a new key")
        assertEquals(Standing.Pairing(again), pairings.standing.value)
        assertEquals("a new key", pairings.key())
    }

    private class MemoryKept : Kept {
        private val values = mutableMapOf<String, String>()

        override fun read(name: String) = values[name]

        override fun replace(values: Map<String, String>) {
            this.values.clear()
            this.values += values
        }
    }
}
