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
        val own = MmsLimits(300 * 1024, Size(LARGEST_SIDE, LARGEST_SIDE))
        assertEquals(own, mmsLimits(null))
        assertEquals(own, mmsLimits(config()))
        val carriers = config(
            SmsManager.MMS_CONFIG_MAX_MESSAGE_SIZE to 600 * 1024,
            SmsManager.MMS_CONFIG_MAX_IMAGE_WIDTH to 640,
            SmsManager.MMS_CONFIG_MAX_IMAGE_HEIGHT to 480,
        )
        assertEquals(MmsLimits(600 * 1024, Size(640, 480)), mmsLimits(carriers))
        assertNull(mmsLimits(config(SmsManager.MMS_CONFIG_MMS_ENABLED to false)))
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
