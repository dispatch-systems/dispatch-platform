package com.dspdispatch.relay.texts

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class SendResultsTest {
    @Test
    fun readsADeliveryReport() {
        // GSM's TP-Status: received, forwarded or replaced; still trying; given up.
        assertTrue(delivered(0x00, "3gpp"))
        assertTrue(delivered(0x02, "3gpp"))
        assertFalse(delivered(0x20, "3gpp"))
        assertFalse(delivered(0x41, "3gpp"))
        // CDMA's, shifted past GSM's: no error and delivered; accepted alone; a temporary error.
        assertTrue(delivered(0x02 shl 16, "3gpp2"))
        assertFalse(delivered(0x00 shl 16, "3gpp2"))
        assertFalse(delivered((0x02 shl 24) or (0x02 shl 16), "3gpp2"))
    }

    @Test
    fun saysWhyAndroidFailedAText() {
        assertNull(Failure.sms(-1))
        assertEquals("No service", Failure.sms(4))
        assertEquals("Couldn't send (code 1)", Failure.sms(1))
        assertNull(Failure.mms(-1))
        assertEquals("Mobile data is off", Failure.mms(8))
        assertEquals("Couldn't send (code 5)", Failure.mms(5))
    }

    @Test
    fun knowsTheRowAndroidWroteATextTo() {
        assertEquals(Table.SMS to 12L, rowAt("content://sms/sent/12"))
        assertEquals(Table.SMS to 12L, rowAt("content://sms/12"))
        assertEquals(Table.MMS to 34L, rowAt("content://mms/sent/34"))
        assertNull(rowAt("content://sms/sent"))
        assertNull(rowAt("content://com.android.contacts/contacts/1"))
    }
}
