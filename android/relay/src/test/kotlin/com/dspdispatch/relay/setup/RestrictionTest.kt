package com.dspdispatch.relay.setup

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class RestrictionTest {
    @Test
    fun `stands for an app installed from a file until texting is allowed`() {
        assertEquals(true, restriction(fromFile = true, texting = false, unrestricted = false))
        assertEquals(false, restriction(fromFile = true, texting = true, unrestricted = false))
    }

    @Test
    fun `is lifted once Android let the person choose on texting, though they refused it`() {
        assertEquals(false, restriction(fromFile = true, texting = false, unrestricted = true))
    }

    @Test
    fun `never applies to an app installed otherwise`() {
        assertNull(restriction(fromFile = false, texting = false, unrestricted = false))
    }
}
