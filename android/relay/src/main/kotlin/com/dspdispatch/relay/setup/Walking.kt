package com.dspdispatch.relay.setup

import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.serialization.json.Json

/**
 * The guided Set up under way, if it is, kept in [saved] as it changes: Android may end the app's
 * process while the person is in one of the screens the walk opens, and the walk goes on as they
 * come back.
 */
class Walking(private val saved: Saved) {
    /** Where the walk under way is kept, as text; null where none is. */
    interface Saved {
        /** The walk kept, if one is. */
        fun read(): String?

        /** Keeps [text], or nothing where null. */
        fun write(text: String?)
    }

    private val state = MutableStateFlow(saved.read()?.let { runCatching { Json.decodeFromString<Walk>(it) }.getOrNull() })

    /** The walk under way, if it is. */
    val walk: StateFlow<Walk?> = state.asStateFlow()

    /** Starts a walk, which takes in the steps undone at the next check. */
    fun start() {
        change { Walk() }
    }

    /** Leaves the walk. */
    fun stop() {
        change { null }
    }

    /** Moves the walk on to what [items] has undone; whether that ended it. */
    fun on(items: List<Item>): Boolean {
        var ended = false
        change { walk -> walk?.next(items).also { ended = walk != null && it == null } }
        return ended
    }

    /**
     * Android refused texting without asking, with [items] as checked then: the walk shows the way
     * through in Texting's place. Whether a walk is under way to show it.
     */
    fun refused(items: List<Item>): Boolean = change { it?.copy(blocked = Blocked.of(items, it.restrictedSettingsOpened)) } != null

    /**
     * The person came back from a screen the walk opened. Back from Restricted settings, which no
     * app can read, texting is asked for again: Android's answer tells whether they were allowed,
     * and refused again without asking, the walk shows the permission's own way through.
     */
    fun back() {
        change { walk -> if (walk?.blocked == Blocked.RESTRICTED) walk.copy(blocked = null, restrictedSettingsOpened = true) else walk }
    }

    /** Changes the walk by [to], keeping what it makes; the walk it made. */
    @Synchronized
    private fun change(to: (Walk?) -> Walk?): Walk? {
        val walk = to(state.value)
        if (walk != state.value) {
            state.value = walk
            saved.write(walk?.let { Json.encodeToString(it) })
        }
        return walk
    }
}
