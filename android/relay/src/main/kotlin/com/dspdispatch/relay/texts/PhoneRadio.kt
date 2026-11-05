package com.dspdispatch.relay.texts

import android.Manifest
import android.content.Context
import android.content.pm.PackageManager
import android.os.Build
import android.os.Bundle
import android.provider.Settings
import android.telephony.SmsManager
import androidx.core.content.ContextCompat
import androidx.core.content.FileProvider
import com.dspdispatch.relay.Phone
import java.io.File

/** The [Radio] of Android's `SmsManager`, on the SIM the phone paired with. */
class PhoneRadio(private val context: Context) : Radio {
    override fun allowed() =
        ContextCompat.checkSelfPermission(context, Manifest.permission.SEND_SMS) == PackageManager.PERMISSION_GRANTED

    override fun smsLimit() = SmsLimit.of { name ->
        try {
            Settings.Global.getString(context.contentResolver, name)
        } catch (_: SecurityException) {
            // From Android 12, Android refuses an app a hidden setting it hasn't marked readable.
            null
        }
    }

    override fun line(number: String, subscription: Int): Line {
        val chosen = subscription(Phone.subscriptions(context, subscription), number, Phone.textingSubscription())
        return PhoneLine(context, manager(chosen))
    }

    override fun relayed(number: String, subscription: Int) =
        relayed(Phone.subscriptions(context, subscription), Phone.slots(context), number, Phone.textingSubscription())

    private fun manager(subscription: Int?): SmsManager {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
            val manager = context.getSystemService(SmsManager::class.java)
            return if (subscription == null) manager else manager.createForSubscriptionId(subscription)
        }
        // Deprecated from Android 12, where the system service and createForSubscriptionId replace
        // them, used above; before it, the only ways to reach a SIM's SmsManager.
        @Suppress("DEPRECATION")
        return if (subscription == null) SmsManager.getDefault() else SmsManager.getSmsManagerForSubscriptionId(subscription)
    }
}

/** A SIM's [Line], each part's result reaching [SendResults]. */
private class PhoneLine(private val context: Context, private val manager: SmsManager) : Line {
    override fun divide(body: String): Parts {
        val texts = manager.divideMessage(body)
        return Parts(texts, joined(texts.size, config()))
    }

    override fun mmsLimits(): MmsLimits? = mmsLimits(config())

    override fun sms(send: Send, parts: Parts) {
        val texts = parts.texts
        val sent = texts.indices.map { SendResults.sent(context, send, it) }
        val delivered = texts.indices.map { SendResults.delivered(context, send, it) }
        if (parts.joined) {
            manager.sendMultipartTextMessage(send.number, null, ArrayList(texts), ArrayList(sent), ArrayList(delivered))
        } else {
            texts.indices.forEach { manager.sendTextMessage(send.number, null, texts[it], sent[it], delivered[it]) }
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
