package com.dspdispatch.relay.texts

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class SmsLimitTest {
    @Test
    fun readsTheLimitThePhoneSets() {
        val settings = mapOf(SmsLimit.MOST_SETTING to "50", SmsLimit.WINDOW_SETTING to "600000")
        assertEquals(SmsLimit(50, 600_000), SmsLimit.of(settings::get))
    }

    @Test
    fun takesAndroidsOwnWhereThePhoneDoesntSay() {
        assertEquals(SmsLimit(30, 30 * 60 * 1000L), SmsLimit.of { null })
        assertEquals(SmsLimit.ANDROID, SmsLimit.of { "none" })
        assertEquals(SmsLimit.ANDROID, SmsLimit.of { "0" })
        assertEquals(SmsLimit.ANDROID.copy(most = 50), SmsLimit.of(mapOf(SmsLimit.MOST_SETTING to "50")::get))
    }

    @Test
    fun stopsTwoPartsShortOfTheLimit() {
        val limit = SmsLimit.ANDROID
        assertFalse(limit.crossedBy(27, 1))
        assertTrue(limit.crossedBy(28, 1))
        // A long text's parts all count.
        assertFalse(limit.crossedBy(25, 3))
        assertTrue(limit.crossedBy(25, 4))
        assertTrue(limit.crossedBy(0, 29))
    }

    @Test
    fun countsAMinutePastTheWindow() {
        assertEquals(AT - 31 * 60 * 1000L, SmsLimit.ANDROID.since(AT))
    }

    private companion object {
        const val AT = 1_793_041_515_000L
    }
}
