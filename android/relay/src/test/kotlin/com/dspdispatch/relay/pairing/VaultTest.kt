package com.dspdispatch.relay.pairing

import java.security.KeyStoreException
import java.security.UnrecoverableKeyException
import javax.crypto.AEADBadTagException
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class VaultTest {
    @Test
    fun readsTheKeyAgainOnceTheKeystoreAnswersAfterAWrappedFailure() = runTest {
        var reads = 0
        val key = reached {
            Vault.opened {
                // As Android's keystore wraps a failure to reach its service as it looks the key up.
                if (++reads == 1) throw UnrecoverableKeyException("Failed to obtain information about key").apply { initCause(KeyStoreException("System error")) }
                "the key"
            }
        }
        assertEquals("the key", key)
        assertEquals(2, reads)
    }

    @Test
    fun takesAKeySealedByAKeyTheKeystoreNoLongerHasForLost() = runTest {
        assertNull(reached { Vault.opened { throw AEADBadTagException("mac check in GCM failed") } })
    }
}
