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

    /**
     * The phone's own number, or empty when Android won't say: not allowed, no SIM knows it, or the
     * phone's telephony isn't there or failed.
     */
    // Lint warns against reading a hardware identifier to track a user. The number is the DSP's
    // dispatch number, what the app exists to relay, and goes only to the DSP it pairs with.
    @SuppressLint("HardwareIds")
    fun number(context: Context): String {
        val allowed = ContextCompat.checkSelfPermission(context, Manifest.permission.READ_PHONE_NUMBERS) ==
            PackageManager.PERMISSION_GRANTED
        if (!allowed) return ""
        return try {
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
                context.getSystemService(SubscriptionManager::class.java)
                    .getPhoneNumber(SubscriptionManager.getDefaultSmsSubscriptionId())
            } else {
                // Deprecated from Android 13, where getPhoneNumber replaces it; the only way before it.
                @Suppress("DEPRECATION")
                context.getSystemService(TelephonyManager::class.java).line1Number.orEmpty()
            }
        } catch (_: IllegalStateException) {
            ""
        } catch (_: SecurityException) {
            ""
        } catch (_: UnsupportedOperationException) {
            ""
        }
    }

    /** As Dispatch holds the phone's model to it (`MODEL_MOST`). */
    private const val MODEL_MOST = 100
}
