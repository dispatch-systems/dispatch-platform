package com.dspdispatch.relay.texts

import android.content.Context
import androidx.core.content.edit
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow

/**
 * Whether a text that would cross Android's SMS limit goes as a picture message, which Android
 * doesn't count, rather than wait on the phone's prompt: on until turned off, kept in the app's
 * own storage across restarts.
 */
class MmsWhenBusy(context: Context) {
    private val prefs = context.getSharedPreferences(NAME, Context.MODE_PRIVATE)
    private val state = MutableStateFlow(prefs.getBoolean(ON, true))

    /** Whether it's on now, as it changes. */
    val on: StateFlow<Boolean> = state.asStateFlow()

    /** Turns it [on] or off. */
    fun set(on: Boolean) {
        prefs.edit { putBoolean(ON, on) }
        state.value = on
    }

    private companion object {
        const val NAME = "texts"
        const val ON = "mms_when_busy"
    }
}
