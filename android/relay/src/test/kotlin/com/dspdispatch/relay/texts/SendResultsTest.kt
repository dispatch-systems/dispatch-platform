package com.dspdispatch.relay.texts

import android.app.PendingIntent
import android.os.Build
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class SendResultsTest {
    @Test
    fun readsADeliveryReport() {
        // GSM's TP-Status: received, forwarded or replaced; still trying; given up.
        assertEquals(Delivery.DELIVERED, delivery(0x00, "3gpp"))
        assertEquals(Delivery.DELIVERED, delivery(0x02, "3gpp"))
        assertEquals(Delivery.PENDING, delivery(0x20, "3gpp"))
        assertEquals(Delivery.FAILED, delivery(0x41, "3gpp"))
        // CDMA's, shifted past GSM's: no error and delivered; accepted alone; a temporary error;
        // a permanent one.
        assertEquals(Delivery.DELIVERED, delivery(0x02 shl 16, "3gpp2"))
        assertEquals(Delivery.PENDING, delivery(0x00 shl 16, "3gpp2"))
        assertEquals(Delivery.PENDING, delivery((0x02 shl 24) or (0x02 shl 16), "3gpp2"))
        assertEquals(Delivery.FAILED, delivery((0x03 shl 24) or (0x02 shl 16), "3gpp2"))
    }

    @Test
    fun keepsADeliveryReportsIntentForTheReportAfterOneStillTrying() {
        // Android sends the same delivery intent for a report it's still trying, then the last.
        assertEquals(0, resultFlags(once = false, Build.VERSION_CODES.R) and PendingIntent.FLAG_ONE_SHOT)
        assertEquals(PendingIntent.FLAG_MUTABLE, resultFlags(once = false, Build.VERSION_CODES.S))
        assertEquals(PendingIntent.FLAG_ONE_SHOT or PendingIntent.FLAG_MUTABLE, resultFlags(once = true, Build.VERSION_CODES.S))
        assertEquals(PendingIntent.FLAG_ONE_SHOT, resultFlags(once = true, Build.VERSION_CODES.O))
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
