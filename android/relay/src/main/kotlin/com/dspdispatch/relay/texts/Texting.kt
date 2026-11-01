package com.dspdispatch.relay.texts

import android.Manifest
import android.content.Context
import android.content.pm.PackageManager
import androidx.core.content.ContextCompat
import kotlinx.coroutines.delay

/** Whether the phone lets the app read its texts and hear them arrive, the setup's Texting step. */
fun textingAllowed(context: Context): Boolean = listOf(Manifest.permission.READ_SMS, Manifest.permission.RECEIVE_SMS)
    .all { ContextCompat.checkSelfPermission(context, it) == PackageManager.PERMISSION_GRANTED }

/** The setup's Texting step, as the relay waits on it: it runs from pairing, Texting maybe later. */
fun interface Texting {
    /** Whether the phone lets the app read its texts now. */
    fun allowed(): Boolean

    /** Returns once the phone lets the app read its texts. */
    suspend fun await() {
        while (!allowed()) delay(ALLOWED_CHECK_MS)
    }
}

private const val ALLOWED_CHECK_MS = 5_000L
