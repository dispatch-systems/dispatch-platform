package com.dspdispatch.relay.dispatch

import kotlinx.serialization.SerialName
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

/**
 * The texts Dispatch has for the phone to send, oldest first, and the app's latest version while
 * the DSP hands one out.
 */
@Serializable
data class Outbox(val texts: List<OutgoingText> = emptyList(), val app: AppVersion? = null)

/** A version of the app: its name, as people see it, and the `versionCode` Android compares. */
@Serializable
data class AppVersion(val version: String, val versionCode: Int)

/** The app's latest version as the DSP hands it out, with its APK's size in bytes and SHA-256. */
@Serializable
data class AppDetails(val version: String, val versionCode: Int, val size: Long, val sha256: String) {
    val app get() = AppVersion(version, versionCode)
}

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

/** Which way a text went: to the phone, or from it. */
@Serializable
enum class Direction {
    @SerialName("in")
    IN,

    @SerialName("out")
    OUT,
}

/** Texts the phone received or sent from its own texting app, at most `MOST_TEXTS` at once. */
@Serializable
data class TextsReport(val texts: List<ReportedText>)

/** A text as the phone reports it, once by [clientId], which is never reused on the phone. */
@Serializable
data class ReportedText(
    val clientId: String,
    val direction: Direction,
    val number: String,
    val body: String,
    val at: Long,
    val files: List<ReportedFile> = emptyList(),
)

/** A picture or file of a reported text, its bytes sent once Dispatch answered the text. */
@Serializable
data class ReportedFile(val name: String, val mime: String, val size: Long)

/** Dispatch's IDs of the texts reported, in the order reported. */
@Serializable
data class TextsAnswer(val texts: List<TextAnswer>)

/** Dispatch's ID of the text reported as [clientId], and of each of its files in order. */
@Serializable
data class TextAnswer(val clientId: String, val id: String, val files: List<String> = emptyList())

/** What Dispatch answers a call that only needs doing. */
@Serializable
internal data class Done(val ok: Boolean)

/** How Dispatch refuses a call: the app acts on [error], never the message. */
@Serializable
internal data class RefusalAnswer(val error: String)
