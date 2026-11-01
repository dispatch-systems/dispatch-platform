package com.dspdispatch.relay.texts

import android.Manifest
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.database.ContentObserver
import android.os.Handler
import android.os.Looper
import android.provider.Telephony
import androidx.core.content.ContextCompat
import kotlinx.coroutines.channels.awaitClose
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.callbackFlow
import kotlinx.coroutines.flow.conflate

/**
 * Each time the phone's texts may have changed, and once at the start: as the texting provider
 * says it changed, and as a text arrives, a nudge before the texting app writes it.
 */
fun changes(context: Context): Flow<Unit> = callbackFlow {
    trySend(Unit)
    val observer = object : ContentObserver(Handler(Looper.getMainLooper())) {
        override fun onChange(selfChange: Boolean) {
            trySend(Unit)
        }
    }
    val arrived = object : BroadcastReceiver() {
        override fun onReceive(context: Context, intent: Intent) {
            trySend(Unit)
        }
    }
    val resolver = context.contentResolver
    // Picture messages' changes are said of all texts' address, not their own, on some phones.
    for (uri in listOf(Telephony.Sms.CONTENT_URI, Telephony.Mms.CONTENT_URI, Telephony.MmsSms.CONTENT_URI)) {
        resolver.registerContentObserver(uri, true, observer)
    }
    // Only the phone's own texting service may send it (BROADCAST_SMS).
    ContextCompat.registerReceiver(
        context,
        arrived,
        IntentFilter(Telephony.Sms.Intents.SMS_RECEIVED_ACTION),
        Manifest.permission.BROADCAST_SMS,
        null,
        ContextCompat.RECEIVER_EXPORTED,
    )
    awaitClose {
        resolver.unregisterContentObserver(observer)
        context.unregisterReceiver(arrived)
    }
}.conflate()
