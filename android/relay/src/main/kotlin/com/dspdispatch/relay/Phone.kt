package com.dspdispatch.relay

import android.Manifest
import android.annotation.SuppressLint
import android.content.Context
import android.content.pm.PackageManager
import android.os.Build
import android.telephony.SubscriptionManager
import android.telephony.TelephonyManager
import androidx.core.content.ContextCompat

/** What the app tells its DSP about the phone as it pairs. */
object Phone {
    /** The phone's maker and model, as Settings shows them. */
    val model: String
        get() = listOf(Build.MANUFACTURER.replaceFirstChar { it.uppercase() }, Build.MODEL)
            .distinct().joinToString(" ").take(MODEL_MOST)

    /** The phone's own number, or empty when Android won't say: not allowed, or no SIM knows it. */
    @SuppressLint("HardwareIds")
    fun number(context: Context): String {
        val allowed = ContextCompat.checkSelfPermission(context, Manifest.permission.READ_PHONE_NUMBERS) ==
            PackageManager.PERMISSION_GRANTED
        if (!allowed) return ""
        val number = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
            context.getSystemService(SubscriptionManager::class.java)
                .getPhoneNumber(SubscriptionManager.getDefaultSmsSubscriptionId())
        } else {
            @Suppress("DEPRECATION")
            context.getSystemService(TelephonyManager::class.java).line1Number
        }
        return number.orEmpty()
    }

    /** As Dispatch holds the phone's model to it (`MODEL_MOST`). */
    private const val MODEL_MOST = 100
}
