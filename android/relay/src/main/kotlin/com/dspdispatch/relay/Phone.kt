package com.dspdispatch.relay

import android.Manifest
import android.annotation.SuppressLint
import android.content.Context
import android.content.pm.PackageManager
import android.os.Build
import android.telephony.SubscriptionManager
import android.telephony.TelephonyManager
import androidx.core.content.ContextCompat
import com.dspdispatch.relay.dispatch.Sim

/** What the app tells its DSP about the phone as it pairs, and which SIMs the phone holds and how they stand. */
object Phone {
    /** The phone's maker and model, as Settings shows them. */
    val model: String
        get() = listOf(Build.MANUFACTURER.replaceFirstChar { it.uppercase() }, Build.MODEL)
            .distinct().joinToString(" ").take(MODEL_MOST)

    /** The subscription of the SIM the phone texts from, or none when it asks each time. */
    fun textingSubscription(): Int = SubscriptionManager.getDefaultSmsSubscriptionId()

    /**
     * The phone's own number on [subscription], or empty when Android won't say: not allowed, no
     * SIM knows it, or the phone's telephony isn't there or failed.
     */
    fun number(context: Context, subscription: Int): String = guarded { readNumber(context, subscription) }.orEmpty()

    /**
     * How the SIM of [subscription] stands, or the phone's own where null, as Android says it now:
     * with no network to text on where Android can't say.
     */
    fun sim(context: Context, subscription: Int?): Sim = guarded { readSim(context, subscription) } ?: Sim.NO_SIGNAL

    private fun readSim(context: Context, subscription: Int?): Sim {
        val telephony = context.getSystemService(TelephonyManager::class.java).let {
            if (subscription == null) it else it.createForSubscriptionId(subscription)
        }
        return when (telephony.simState) {
            TelephonyManager.SIM_STATE_ABSENT -> Sim.ABSENT
            // Its network's code, there only while it's registered on one.
            TelephonyManager.SIM_STATE_READY -> if (telephony.networkOperator.isNullOrEmpty()) Sim.NO_SIGNAL else Sim.READY
            else -> Sim.NO_SIGNAL
        }
    }

    /**
     * The subscriptions of the phone's SIMs, each with its number, empty where Android won't say.
     * Before Android 10, which lists them only to apps reading the phone's state, those it names
     * without: [paired], the one the phone paired on, then its default for texts.
     */
    fun subscriptions(context: Context, paired: Int): Map<Int, String> = guarded { readSubscriptions(context, paired) }.orEmpty()

    private fun readSubscriptions(context: Context, paired: Int): Map<Int, String> {
        val slots = context.getSystemService(TelephonyManager::class.java).slots()
        val subscriptions = when {
            Build.VERSION.SDK_INT >= Build.VERSION_CODES.UPSIDE_DOWN_CAKE -> (0 until slots).map(SubscriptionManager::getSubscriptionId)
            Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q -> {
                val manager = context.getSystemService(SubscriptionManager::class.java)
                // Deprecated from Android 14, where getSubscriptionId replaces it, used above. On
                // Android 10 to 13 it's the only way to name a slot's SIM without reading the
                // phone's state.
                @Suppress("DEPRECATION")
                (0 until slots).flatMap { manager.getSubscriptionIds(it)?.toList().orEmpty() }
            }
            else -> listOf(paired, textingSubscription())
        }
        return subscriptions.filter { it != SubscriptionManager.INVALID_SUBSCRIPTION_ID }.associateWith { readNumber(context, it) }
    }

    /** How many SIMs the phone takes at once. */
    private fun TelephonyManager.slots(): Int = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
        activeModemCount
    } else {
        // Deprecated from Android 11, where activeModemCount replaces it, used above; before it,
        // the only way to count the phone's SIM slots.
        @Suppress("DEPRECATION")
        phoneCount
    }

    // Lint warns against reading a hardware identifier to track a user. The number is the DSP's
    // dispatch number, what the app exists to relay, and goes only to the DSP it pairs with.
    @SuppressLint("HardwareIds")
    private fun readNumber(context: Context, subscription: Int): String {
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
