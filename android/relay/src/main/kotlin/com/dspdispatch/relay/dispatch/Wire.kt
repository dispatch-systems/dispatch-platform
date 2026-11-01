package com.dspdispatch.relay.dispatch

import kotlinx.serialization.Serializable
import kotlinx.serialization.json.Json

// What the app and its DSP say to each other, as features/communications/api/phone/README.md
// has it: JSON, times in epoch milliseconds, sizes in bytes, IDs opaque.

/** Reads Dispatch's answers, leaving out what this version of the app doesn't know yet. */
internal val wire = Json {
    ignoreUnknownKeys = true
    explicitNulls = false
}

@Serializable
data class PairRequest(val code: String, val number: String, val model: String, val version: String)

@Serializable
data class PairAnswer(val key: String, val phone: PairedPhone, val dsp: PairedDsp)

@Serializable
data class PairedPhone(val id: String, val name: String)

@Serializable
data class PairedDsp(val name: String)

/** The texts Dispatch has for the phone to send, oldest first. */
@Serializable
data class Outbox(val texts: List<OutgoingText> = emptyList())

/** A text to send from the phone; each [attempt] is sent once, and a later one is new. */
@Serializable
data class OutgoingText(
    val id: String,
    val number: String,
    val body: String,
    val attempt: Long,
    val files: List<OutgoingFile> = emptyList(),
)

@Serializable
data class OutgoingFile(val id: String, val name: String, val mime: String, val size: Long)

/** How Dispatch refuses a call: the app acts on [error], never the message. */
@Serializable
internal data class RefusalAnswer(val error: String)
