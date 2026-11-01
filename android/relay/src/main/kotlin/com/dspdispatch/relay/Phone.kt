package com.dspdispatch.relay

import android.Manifest
import android.annotation.SuppressLint
import android.content.Context
import android.content.pm.PackageManager
import android.os.Build
import android.telephony.SubscriptionManager
import android.telephony.TelephonyManager
import androidx.core.content.ContextCompat

/** What the app tells its DSP about the phone as it pairs, and which SIMs the phone holds. */
object Phone {
    /** The phone's maker and model, as Settings shows them. */
    val model: String
        get() = listOf(Build.MANUFACTURER.replaceFirstChar { it.uppercase() }, Build.MODEL)
            .distinct().joinToString(" ").take(MODEL_MOST)

    /**
     * The phone's own number, or empty when Android won't say: not allowed, no SIM knows it, or the
     * phone's telephony isn't there or failed.
     */
    fun number(context: Context): String = guarded { number(context, SubscriptionManager.getDefaultSmsSubscriptionId()) }.orEmpty()

    /**
     * The subscriptions of the phone's SIMs, each with its number, empty where Android won't say.
     * None before Android 10, which names them only to apps reading the phone's state.
     */
    fun subscriptions(context: Context): Map<Int, String> = guarded { readSubscriptions(context) }.orEmpty()

    private fun readSubscriptions(context: Context): Map<Int, String> {
        val slots = context.getSystemService(TelephonyManager::class.java).slots()
        val subscriptions = when {
            Build.VERSION.SDK_INT >= Build.VERSION_CODES.UPSIDE_DOWN_CAKE -> (0 until slots).map(SubscriptionManager::getSubscriptionId)
            Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q -> {
                val manager = context.getSystemService(SubscriptionManager::class.java)
                // Deprecated from Android 14, where getSubscriptionId replaces it.
                @Suppress("DEPRECATION")
                (0 until slots).flatMap { manager.getSubscriptionIds(it)?.toList().orEmpty() }
            }
            else -> emptyList()
        }
        return subscriptions.filter { it != SubscriptionManager.INVALID_SUBSCRIPTION_ID }.associateWith { number(context, it) }
    }

    /** How many SIMs the phone takes at once. */
    private fun TelephonyManager.slots(): Int = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
        activeModemCount
    } else {
        // Deprecated from Android 11, where activeModemCount replaces it.
        @Suppress("DEPRECATION")
        phoneCount
    }

    // Lint warns against reading a hardware identifier to track a user. The number is the DSP's
    // dispatch number, what the app exists to relay, and goes only to the DSP it pairs with.
    @SuppressLint("HardwareIds")
    private fun number(context: Context, subscription: Int): String {
        val allowed = ContextCompat.checkSelfPermission(context, Manifest.permission.READ_PHONE_NUMBERS) ==
            PackageManager.PERMISSION_GRANTED
        if (!allowed) return ""
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
            return context.getSystemService(SubscriptionManager::class.java).getPhoneNumber(subscription)
        }
        val telephony = context.getSystemService(TelephonyManager::class.java).let {
            if (subscription == SubscriptionManager.INVALID_SUBSCRIPTION_ID) it else it.createForSubscriptionId(subscription)
        }
        // Deprecated from Android 13, where getPhoneNumber replaces it; the only way before it.
        @Suppress("DEPRECATION")
        return telephony.line1Number.orEmpty()
    }

    /** What [read] answers, or null when the phone's telephony isn't there, refused or failed. */
    private fun <T> guarded(read: () -> T): T? = try {
        read()
    } catch (_: IllegalArgumentException) {
        null
    } catch (_: IllegalStateException) {
        null
    } catch (_: SecurityException) {
        null
    } catch (_: UnsupportedOperationException) {
        null
    }

    /** As Dispatch holds the phone's model to it (`MODEL_MOST`). */
    private const val MODEL_MOST = 100
}
