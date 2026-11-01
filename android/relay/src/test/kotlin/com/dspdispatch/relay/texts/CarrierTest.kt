package com.dspdispatch.relay.texts

import android.telephony.SmsManager
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class CarrierTest {
    @Test
    fun takesAndroidsOwnWhereTheCarrierDoesntSayOrAndroidDidntAnswer() {
        assertEquals(300 * 1024, mmsMost(null))
        assertEquals(300 * 1024, mmsMost(config()))
        assertEquals(600 * 1024, mmsMost(config(SmsManager.MMS_CONFIG_MAX_MESSAGE_SIZE to 600 * 1024)))
        assertNull(mmsMost(config(SmsManager.MMS_CONFIG_MMS_ENABLED to false)))
    }

    @Test
    fun joinsALongTextsPartsUnlessTheCarrierTakesThemApart() {
        assertTrue(joined(2, null))
        assertTrue(joined(2, config()))
        assertFalse(joined(2, config(SmsManager.MMS_CONFIG_SEND_MULTIPART_SMS_AS_SEPARATE_MESSAGES to true)))
        assertFalse(joined(1, null))
    }

    private fun config(vararg settings: Pair<String, Any>) = object : CarrierConfig {
        private val values = settings.toMap()

        override fun flag(name: String, default: Boolean) = values[name] as Boolean? ?: default

        override fun number(name: String) = values[name] as Int? ?: 0
    }
}
