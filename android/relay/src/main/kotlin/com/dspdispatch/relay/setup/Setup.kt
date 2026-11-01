package com.dspdispatch.relay.setup

import android.content.Context
import android.content.Intent
import androidx.core.content.edit
import com.dspdispatch.relay.Notifications
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.withContext

/**
 * Set up: the steps the app needs of the phone, as last checked, the guided walk through those
 * undone, and whether Set up was finished. What only the person can say is done, and the walk
 * under way, are kept in the app's own storage.
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

    private val walking = Walking(object : Walking.Saved {
        override fun read() = prefs.getString(WALK, null)

        override fun write(text: String?) = prefs.edit { if (text == null) remove(WALK) else putString(WALK, text) }
    })

    /** The guided Set up under way, if it is, kept across Android ending the app's process. */
    val walk: StateFlow<Walk?> = walking.walk

    /**
     * Checks every step again. With [notify], a step undone and not yet notified is notified, once
     * it can be; a step done again takes its notification away, and is notified again if undone
     * later.
     */
    suspend fun check(notify: Boolean = false): List<Item> {
        val facts = withContext(Dispatchers.IO) {
            probe.read({ byHand(makerKey(it)) }, byHand(Step.CHAT_FEATURES.name), prefs.getBoolean(TEXTING_UNRESTRICTED, false))
        }
        if (facts.texting) textingUnrestricted()
        val items = checklist(facts)
        checked.value = items
        walkOn(items)
        val undone = items.filterNot(Item::done).map(Item::step).toSet()
        Step.entries.filterNot(undone::contains).forEach { Notifications.cancelUndone(context, it) }
        val before = prefs.getStringSet(NOTIFIED, emptySet()).orEmpty().mapNotNull { name -> Step.entries.find { it.name == name } }.toSet()
        val post = if (notify && done.value) { step: Step -> Notifications.undone(context, step) } else null
        val after = notified(before, undone, post)
        if (after != before) prefs.edit { putStringSet(NOTIFIED, after.map(Step::name).toSet()) }
        return items
    }

    /**
     * The screen of [step] only the person can say is done, marked opened; for the battery, the
     * first of the maker's controls not yet done. Null where the phone has none.
     */
    suspend fun open(step: Step): Intent? = withContext(Dispatchers.IO) { byHandScreen(step) }?.let { (key, intent) ->
        if (byHand(key) == ByHand.NONE) prefs.edit { putString(key, ByHand.OPENED.name) }
        intent
    }

    /** The person says the screen of [step] they opened, which Android can't report, is done. */
    suspend fun confirm(step: Step) {
        val key = withContext(Dispatchers.IO) { byHandScreen(step) }?.first ?: return
        val now = byHand(key)
        val confirmed = now.confirmed()
        if (confirmed != now) prefs.edit { putString(key, confirmed.name) }
    }

    /** Starts the guided walk through the steps undone now; Set up is finished at once where none is. */
    suspend fun start() {
        walking.start()
        check()
    }

    /** Leaves the guided walk, for the screen it started from. */
    fun stop() = walking.stop()

    /**
     * Android let the person choose on texting, by its own prompt or by texting allowed, which it
     * does only once it no longer restricts texting: kept, as Android keeps it from apps.
     */
    fun textingUnrestricted() {
        if (!prefs.getBoolean(TEXTING_UNRESTRICTED, false)) prefs.edit { putBoolean(TEXTING_UNRESTRICTED, true) }
    }

    /**
     * Android refused texting without asking: checked again, the walk shows the way through,
     * Restricted settings where Android restricts texting, else App info. Whether a walk is under
     * way to show it.
     */
    suspend fun textingRefused(): Boolean = walking.refused(check())

    /** The person came back from a screen a step opened: after Restricted settings, the walk asks for texting again. */
    fun back() = walking.back()

    /** Moves the walk on to what [items] has undone; at its end, Set up is finished. */
    private fun walkOn(items: List<Item>) {
        if (walking.on(items) && !done.value) finish()
    }

    /** Set up is done: the running screen shows from now on. */
    private fun finish() {
        prefs.edit { putBoolean(FINISHED, true) }
        done.value = true
    }

    /** The screen of [step] the person is to set by hand now, with the key its state is kept by. */
    private fun byHandScreen(step: Step): Pair<String, Intent>? = when (step) {
        Step.BATTERY -> probe.makerBattery().entries.map { makerKey(it.key) to it.value }.firstOrNull { byHand(it.first) != ByHand.DONE }
        Step.CHAT_FEATURES -> probe.textingApp()?.let { step.name to it }
        else -> null
    }

    private fun makerKey(control: String) = "${Step.BATTERY.name}.$control"

    private fun byHand(key: String) =
        prefs.getString(key, null)?.let { runCatching { ByHand.valueOf(it) }.getOrNull() } ?: ByHand.NONE

    private companion object {
        const val FINISHED = "finished"
        const val NOTIFIED = "notified"
        const val WALK = "walk"
        const val TEXTING_UNRESTRICTED = "texting_unrestricted"
    }
}

/**
 * The steps notified as undone after a check that found [undone] undone: those of [before] still
 * undone, and, where the check notifies, each other one [post] posted. A step done again leaves,
 * so that it's notified again once undone; one not posted stays out, to be posted by a later check.
 */
internal fun notified(before: Set<Step>, undone: Set<Step>, post: ((Step) -> Boolean)?): Set<Step> {
    val kept = before intersect undone
    return if (post == null) kept else kept + (undone - kept).filter(post)
}
