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

/** What [open] answers once the keystore can be read: asked again after a growing pause while it can't. */
internal suspend fun <T> reached(open: () -> T): T {
    var failures = 0
    while (true) {
        try {
            return open()
        } catch (_: Vault.Unreachable) {
            delay(backoff(++failures))
        }
    }
}

/** Where the phone stands with its DSP: paired, never paired, or removed by the DSP. */
sealed interface Standing {
    data object Unpaired : Standing

    data class Removed(val dsp: String) : Standing

    data class Pairing(val paired: Paired) : Standing
}

/** Where the pairing is kept, each value by its name: the app's own storage. */
internal interface Kept {
    /** The value kept as [name], if one is. */
    fun read(name: String): String?

    /** Keeps [values] in place of everything kept. */
    fun replace(values: Map<String, String>)
}

/** What seals the pairing's key, and opens it again ([Vault]). */
internal interface Locks {
    fun seal(secret: String): String

    /** [sealed] opened, or null once it can never be; [Vault.Unreachable] while it can't be just now. */
    fun open(sealed: String): String?
}

/**
 * The phone's pairing, [kept] in the app's own storage, its key sealed by the [vault]. Nothing of
 * it leaves the phone in a backup (`data_extraction_rules.xml`).
 */
class Pairings internal constructor(private val kept: Kept, private val vault: Locks) {
    constructor(context: Context) : this(Stored(context), Vault)

    private val state = MutableStateFlow(read())

    /** Where the phone stands now, as it changes. */
    val standing: StateFlow<Standing> = state.asStateFlow()

    /**
     * The key the phone calls its DSP with, while it's paired; read again after a growing pause
     * while the keystore can't be read. Null once it's lost for good, as after the keystore lost
     * what sealed it, when the pairing is forgotten, for the phone to pair again.
     */
    suspend fun key(): String? {
        val paired = (state.value as? Standing.Pairing)?.paired ?: return null
        val key = kept.read(KEY)?.let { sealed -> reached { vault.open(sealed) } }
        if (key == null) forget(paired)
        return key
    }

    fun save(paired: Paired, key: String) {
        kept.replace(mapOf(PAIRED to Json.encodeToString(paired), KEY to vault.seal(key)))
        state.value = Standing.Pairing(paired)
    }

    /** The DSP removed this phone, or no longer knows its key: forgets the key. */
    fun removed(paired: Paired) {
        kept.replace(mapOf(REMOVED to paired.dsp))
        state.value = Standing.Removed(paired.dsp)
    }

    /** Forgets [paired], whose key is lost for good, unless the phone paired again since. */
    private fun forget(paired: Paired) {
        if ((state.value as? Standing.Pairing)?.paired != paired) return
        kept.replace(emptyMap())
        state.value = Standing.Unpaired
    }

    private fun read(): Standing {
        val paired = kept.read(PAIRED)?.let { runCatching { Json.decodeFromString<Paired>(it) }.getOrNull() }
        val removed = kept.read(REMOVED)
        return when {
            paired != null && !lost() -> Standing.Pairing(paired)
            removed != null -> Standing.Removed(removed)
            else -> Standing.Unpaired
        }
    }

    /**
     * Whether the key is gone for good, or was never kept. Not while the keystore can't be read
     * just now: the phone stays paired, and the relay reads its key again ([key]).
     */
    private fun lost(): Boolean = try {
        kept.read(KEY)?.let(vault::open) == null
    } catch (_: Vault.Unreachable) {
        false
    }

    /** The pairing [Kept] in the app's own storage. */
    private class Stored(context: Context) : Kept {
        private val prefs = context.getSharedPreferences("pairing", Context.MODE_PRIVATE)

        override fun read(name: String): String? = prefs.getString(name, null)

        override fun replace(values: Map<String, String>) = prefs.edit {
            clear()
            values.forEach { (name, value) -> putString(name, value) }
        }
    }

    private companion object {
        const val PAIRED = "paired"
        const val KEY = "key"
        const val REMOVED = "removed"
    }
}
