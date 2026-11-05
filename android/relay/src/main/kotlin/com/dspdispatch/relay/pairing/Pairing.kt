package com.dspdispatch.relay.pairing

import android.content.Context
import android.telephony.SubscriptionManager
import androidx.core.content.edit
import com.dspdispatch.relay.connection.backoff
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.Json

/**
 * The DSP this phone is paired with, as which of its phones, and when it paired ([at]): [number]
 * is the phone's own as it paired, empty when Android didn't say, and [subscription] its SIM's,
 * none when the phone asked each time; they pick the SIM its texts go out from.
 */
@Serializable
data class Paired(
    val site: String,
    val dsp: String,
    val phoneId: String,
    val phoneName: String,
    val at: Long,
    val number: String = "",
    val subscription: Int = SubscriptionManager.INVALID_SUBSCRIPTION_ID,
)

/** Where the phone stands with its DSP: paired, never paired, or removed by the DSP. */
sealed interface Standing {
    data object Unpaired : Standing

    data class Removed(val dsp: String) : Standing

    data class Pairing(val paired: Paired) : Standing
}

/**
 * The phone's pairing, kept in the app's own storage, its key sealed by the [Vault]. Nothing of
 * it leaves the phone in a backup (`data_extraction_rules.xml`).
 */
class Pairings(context: Context) {
    private val prefs = context.getSharedPreferences("pairing", Context.MODE_PRIVATE)
    private val state = MutableStateFlow(read())

    /** Where the phone stands now, as it changes. */
    val standing: StateFlow<Standing> = state.asStateFlow()

    /**
     * The key the phone calls its DSP with, while it's paired; null once it's lost for good, as
     * after the keystore lost what sealed it. Read again after a growing pause while the keystore
     * can't be read.
     */
    suspend fun key(): String? {
        val sealed = prefs.getString(KEY, null) ?: return null
        var failures = 0
        while (true) {
            try {
                return Vault.open(sealed)
            } catch (_: Vault.Unreachable) {
                delay(backoff(++failures))
            }
        }
    }

    fun save(paired: Paired, key: String) {
        prefs.edit {
            clear()
            putString(PAIRED, Json.encodeToString(paired))
            putString(KEY, Vault.seal(key))
        }
        state.value = Standing.Pairing(paired)
    }

    /** The DSP removed this phone, or no longer knows its key: forgets the key. */
    fun removed(paired: Paired) {
        prefs.edit {
            clear()
            putString(REMOVED, paired.dsp)
        }
        state.value = Standing.Removed(paired.dsp)
    }

    private fun read(): Standing {
        val paired = prefs.getString(PAIRED, null)?.let { runCatching { Json.decodeFromString<Paired>(it) }.getOrNull() }
        val removed = prefs.getString(REMOVED, null)
        return when {
            paired != null && !lost() -> Standing.Pairing(paired)
            removed != null -> Standing.Removed(removed)
            else -> Standing.Unpaired
        }
    }

    /**
     * Whether the key is gone for good, or was never kept. Not while the keystore can't be read
     * just now: the phone stays paired, and the relay reads its key again.
     */
    private fun lost(): Boolean = try {
        prefs.getString(KEY, null)?.let(Vault::open) == null
    } catch (_: Vault.Unreachable) {
        false
    }

    private companion object {
        const val PAIRED = "paired"
        const val KEY = "key"
        const val REMOVED = "removed"
    }
}
