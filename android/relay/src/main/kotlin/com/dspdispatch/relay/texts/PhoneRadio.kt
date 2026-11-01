package com.dspdispatch.relay.texts

import android.Manifest
import android.content.Context
import android.content.pm.PackageManager
import android.os.Build
import android.os.Bundle
import android.telephony.SmsManager
import androidx.core.content.ContextCompat
import androidx.core.content.FileProvider
import com.dspdispatch.relay.Phone
import java.io.File

/** The [Radio] of Android's `SmsManager`, on the SIM the phone paired with. */
class PhoneRadio(private val context: Context) : Radio {
    override fun allowed() =
        ContextCompat.checkSelfPermission(context, Manifest.permission.SEND_SMS) == PackageManager.PERMISSION_GRANTED

    override fun line(number: String, subscription: Int): Line {
        val chosen = subscription(Phone.subscriptions(context, subscription), number, Phone.textingSubscription())
        return PhoneLine(context, manager(chosen))
    }

    private fun manager(subscription: Int?): SmsManager {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
            val manager = context.getSystemService(SmsManager::class.java)
            return if (subscription == null) manager else manager.createForSubscriptionId(subscription)
        }
        // Deprecated from Android 12, where the system service and createForSubscriptionId replace them.
        @Suppress("DEPRECATION")
        return if (subscription == null) SmsManager.getDefault() else SmsManager.getSmsManagerForSubscriptionId(subscription)
    }
}

/** A SIM's [Line], each part's result reaching [SendResults]. */
private class PhoneLine(private val context: Context, private val manager: SmsManager) : Line {
    override fun divide(body: String): List<String> = manager.divideMessage(body)

    override fun mmsLimits(): MmsLimits? = mmsLimits(config())

    override fun sms(send: Send, parts: List<String>) {
        val sent = parts.indices.map { SendResults.sent(context, send, it) }
        val delivered = parts.indices.map { SendResults.delivered(context, send, it) }
        when {
            parts.size == 1 -> manager.sendTextMessage(send.number, null, parts.single(), sent.single(), delivered.single())
            joined(parts.size, config()) ->
                manager.sendMultipartTextMessage(send.number, null, ArrayList(parts), ArrayList(sent), ArrayList(delivered))
            else -> parts.indices.forEach { manager.sendTextMessage(send.number, null, parts[it], sent[it], delivered[it]) }
        }
    }

    override fun mms(send: Send, pdu: File) {
        // Android's picture message service reads the message by this address, granted to it as
        // it takes the message.
        val uri = FileProvider.getUriForFile(context, "${context.packageName}$MMS_AUTHORITY", pdu)
        manager.sendMultimediaMessage(context, uri, null, null, SendResults.mmsSent(context, send))
    }

    private fun config(): CarrierConfig? = carrierBundle()?.let(::BundleConfig)

    // Android 8 and 9 answer null while their picture message service is away, though the SDK
    // now says it never does.
    private fun carrierBundle(): Bundle? = manager.carrierConfigValues
}

/** A [CarrierConfig] as Android gives it. */
private class BundleConfig(private val bundle: Bundle) : CarrierConfig {
    override fun flag(name: String, default: Boolean) = bundle.getBoolean(name, default)

    override fun number(name: String) = bundle.getInt(name)
}

/** The authority, after the app's ID, of the picture messages handed to Android (`mms_files.xml`). */
const val MMS_AUTHORITY = ".mms"
