package com.dspdispatch.relay.setup

import android.content.Context
import androidx.core.content.edit
import com.dspdispatch.relay.Notifications
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.withContext

/**
 * Set up: the steps the app needs of the phone, as last checked, and whether Set up was finished.
 * What only the person can say is done is kept in the app's own storage.
 */
class Setup(private val context: Context) {
    private val prefs = context.getSharedPreferences("setup", Context.MODE_PRIVATE)
    val probe = Probe(context)

    private val checked = MutableStateFlow<List<Item>?>(null)

    /** The steps that apply to the phone, each done or not; null until first checked. */
    val items: StateFlow<List<Item>?> = checked.asStateFlow()

    private val done = MutableStateFlow(prefs.getBoolean(FINISHED, false))

    /** Whether Set up was finished, after which the running screen shows instead. */
    val finished: StateFlow<Boolean> = done.asStateFlow()

    /**
     * Checks every step again. With [notify], a step undone since the last notifying check is
     * notified, once; a step done again takes its notification away.
     */
    suspend fun check(notify: Boolean = false): List<Item> {
        val items = withContext(Dispatchers.IO) { checklist(probe.read(byHand(Step.BATTERY), byHand(Step.CHAT_FEATURES))) }
        checked.value = items
        val undone = items.filterNot(Item::done).map(Item::step).toSet()
        Step.entries.filterNot(undone::contains).forEach { Notifications.cancelUndone(context, it) }
        if (notify && done.value) {
            val notified = prefs.getStringSet(NOTIFIED, emptySet()).orEmpty()
            undone.filterNot { it.name in notified }.forEach { Notifications.undone(context, it) }
            prefs.edit { putStringSet(NOTIFIED, undone.map(Step::name).toSet()) }
        }
        return items
    }

    /** The screen of a step only the person can say is done was opened. */
    fun opened(step: Step) {
        if (byHand(step) == ByHand.NONE) prefs.edit { putString(step.name, ByHand.OPENED.name) }
    }

    /** The person says a step Android can't report is done. */
    fun confirmed(step: Step) = prefs.edit { putString(step.name, ByHand.DONE.name) }

    /** Set up is done: the running screen shows from now on. */
    fun finish() {
        prefs.edit { putBoolean(FINISHED, true) }
        done.value = true
    }

    private fun byHand(step: Step) =
        prefs.getString(step.name, null)?.let { runCatching { ByHand.valueOf(it) }.getOrNull() } ?: ByHand.NONE

    private companion object {
        const val FINISHED = "finished"
        const val NOTIFIED = "notified"
    }
}
