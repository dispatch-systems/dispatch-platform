package com.dspdispatch.relay.texts

import android.app.PendingIntent
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.os.Build
import android.telephony.SmsManager
import android.telephony.SmsMessage
import androidx.core.net.toUri
import com.dspdispatch.relay.relay
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch

/**
 * Hears what Android says of each text the app handed it, a part at a time: sent or failed, and
 * delivered where the carrier reports it. Android may say it after the app stopped, starting it.
 */
class SendResults : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        val heard = heard(context, intent, resultCode, System.currentTimeMillis()) ?: return
        // Kept before Android may stop the app again: goAsync holds it while the write takes the
        // database off the main thread.
        val pending = goAsync()
        CoroutineScope(Dispatchers.IO).launch {
            try {
                context.relay.texts.heard(heard, intent.getStringExtra(URI))
            } finally {
                pending.finish()
            }
        }
    }

    private fun heard(context: Context, intent: Intent, code: Int, at: Long): Heard? {
        val id = intent.getStringExtra(ID) ?: return null
        val attempt = intent.getLongExtra(ATTEMPT, -1).takeIf { it >= 0 } ?: return null
        val part = intent.getIntExtra(PART, 0)
        return when (intent.action) {
            SMS_SENT -> Heard(id, attempt, part, Event.SENT, Failure.sms(code), null, at)
            SMS_DELIVERED -> Heard(id, attempt, part, Event.DELIVERED, null, null, at).takeIf { delivered(context, intent) }
            MMS_SENT -> Heard(id, attempt, part, Event.SENT, Failure.mms(code) ?: refused(intent), null, at)
            else -> null
        }
    }

    // Android reports a part delivered as the carrier does: a report that it's still trying keeps
    // the same PendingIntent for the next. A final one retires it.
    private fun delivered(context: Context, intent: Intent): Boolean {
        val pdu = intent.getByteArrayExtra(PDU) ?: return false
        val format = intent.getStringExtra(FORMAT) ?: return false
        val status = SmsMessage.createFromPdu(pdu, format)?.status ?: return false
        val delivery = delivery(status, format)
        if (delivery != Delivery.PENDING) retire(context, intent)
        return delivery == Delivery.DELIVERED
    }

    private fun retire(context: Context, heard: Intent) {
        val intent = Intent(context, SendResults::class.java).setAction(heard.action).setData(heard.data)
        PendingIntent.getBroadcast(context, 0, intent, PendingIntent.FLAG_NO_CREATE or mutable())?.cancel()
    }

    /** Why the carrier refused a picture message Android sent, by its answer, if it did. */
    private fun refused(intent: Intent): String? {
        val answer = intent.getByteArrayExtra(SmsManager.EXTRA_MMS_DATA) ?: return null
        val status = Pdu.responseStatus(answer) ?: return null
        return Failure.MMS_REFUSED.takeIf { status != Pdu.RESPONSE_OK }
    }

    companion object {
        private const val SMS_SENT = "com.dspdispatch.relay.SMS_SENT"
        private const val SMS_DELIVERED = "com.dspdispatch.relay.SMS_DELIVERED"
        private const val MMS_SENT = "com.dspdispatch.relay.MMS_SENT"
        private const val ID = "id"
        private const val ATTEMPT = "attempt"
        private const val PART = "part"

        // What Android adds to a result (SMSDispatcher's, and MmsService's). The URI is where it
        // wrote the text in the provider, as it does for an app that isn't the phone's texting app.
        private const val URI = "uri"
        private const val PDU = "pdu"
        private const val FORMAT = "format"

        /** What Android sends as [part] of [send] is sent, or fails. */
        fun sent(context: Context, send: Send, part: Int) = result(context, SMS_SENT, send, part, once = true)

        /** What Android sends each time the carrier reports on [part] of [send]'s delivery. */
        fun delivered(context: Context, send: Send, part: Int) = result(context, SMS_DELIVERED, send, part, once = false)

        /** What Android sends as the picture message of [send] is sent, or fails. */
        fun mmsSent(context: Context, send: Send) = result(context, MMS_SENT, send, 0, once = true)

        // Each its own: Android tells PendingIntents apart by their intents' actions and data, not
        // their extras.
        private fun result(context: Context, action: String, send: Send, part: Int, once: Boolean): PendingIntent {
            val intent = Intent(context, SendResults::class.java)
                .setAction(action)
                .setData("relay-send:${send.id}/${send.attempt}/$part".toUri())
                .putExtra(ID, send.id)
                .putExtra(ATTEMPT, send.attempt)
                .putExtra(PART, part)
            return PendingIntent.getBroadcast(context, 0, intent, resultFlags(once, Build.VERSION.SDK_INT))
        }

        private fun mutable() = resultFlags(once = false, Build.VERSION.SDK_INT)
    }
}

/**
 * A result's PendingIntent's flags on Android [sdk]: mutable, as Android adds the result's details
 * to it, and used [once] unless Android may send it again, as a delivery report it's still trying.
 */
internal fun resultFlags(once: Boolean, sdk: Int): Int {
    val mutable = if (sdk >= Build.VERSION_CODES.S) PendingIntent.FLAG_MUTABLE else 0
    return if (once) PendingIntent.FLAG_ONE_SHOT or mutable else mutable
}

/** Where a carrier's report leaves a part's delivery. */
internal enum class Delivery {
    DELIVERED,

    /** Still trying: another report follows. */
    PENDING,
    FAILED,
}

/**
 * Where a delivery report's [status] (`SmsMessage.getStatus`) leaves the part: GSM's TP-Status,
 * done below 0x20, still trying below 0x40, else failed; CDMA's error class and message status,
 * delivered with no error, failed for a permanent one, else still trying.
 */
internal fun delivery(status: Int, format: String): Delivery = if (format == CDMA) {
    val error = (status shr CDMA_CLASS_SHIFT) and CDMA_CLASS_BITS
    when {
        error == CDMA_PERMANENT -> Delivery.FAILED
        error == 0 && ((status shr CDMA_STATUS_SHIFT) and CDMA_STATUS_BITS) == CDMA_DELIVERED -> Delivery.DELIVERED
        else -> Delivery.PENDING
    }
} else {
    when (status and BYTE) {
        in 0 until GSM_PENDING -> Delivery.DELIVERED
        in GSM_PENDING until GSM_FAILED -> Delivery.PENDING
        else -> Delivery.FAILED
    }
}

private const val CDMA = "3gpp2"
private const val CDMA_CLASS_SHIFT = 24
private const val CDMA_CLASS_BITS = 0x03
private const val CDMA_STATUS_SHIFT = 16
private const val CDMA_STATUS_BITS = 0x3F
private const val CDMA_DELIVERED = 2
private const val CDMA_PERMANENT = 3
private const val GSM_PENDING = 0x20
private const val GSM_FAILED = 0x40
private const val BYTE = 0xFF
