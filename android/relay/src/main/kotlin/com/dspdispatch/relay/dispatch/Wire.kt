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
 * The texts Dispatch has for the phone to send, oldest first; the videos the phone keeps that
 * someone is playing, by Dispatch's IDs of them, whose bytes it sends; and the app's latest version
 * while the DSP hands one out.
 */
@Serializable
data class Outbox(val texts: List<OutgoingText> = emptyList(), val wanted: List<String> = emptyList(), val app: AppVersion? = null)

/** A version of the app: its name, as people see it, and the `versionCode` Android compares. */
@Serializable
data class AppVersion(val version: String, val versionCode: Int)

/** The app's latest version as the DSP hands it out, with its APK's size in bytes and SHA-256. */
@Serializable
data class AppDetails(val version: String, val versionCode: Int, val size: Long, val sha256: String) {
    val app get() = AppVersion(version, versionCode)
}

/**
 * A text to send from the phone; each [attempt] is sent once, and a later one is new. Never started
 * once [expires] has passed, where Dispatch gives it.
 */
@Serializable
data class OutgoingText(
    val id: String,
    val number: String,
    val body: String,
    val attempt: Long,
    val files: List<OutgoingFile> = emptyList(),
    val expires: Long? = null,
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

/**
 * A picture or file of a reported text, its bytes sent once Dispatch answered the text; a video
 * [onPhone] stays on the phone, only the picture of its first frame sent, its bytes once asked for.
 */
@Serializable
data class ReportedFile(val name: String, val mime: String, val size: Long, val onPhone: Boolean = false)

/** Dispatch's IDs of the texts reported, in the order reported. */
@Serializable
data class TextsAnswer(val texts: List<TextAnswer>)

/** Dispatch's ID of the text reported as [clientId], and of each of its files in order. */
@Serializable
data class TextAnswer(val clientId: String, val id: String, val files: List<String> = emptyList())

/** How an attempt at a text of the outbox went, as the phone reports it. */
@Serializable
enum class SendStatus {
    @SerialName("sent")
    SENT,

    @SerialName("delivered")
    DELIVERED,

    @SerialName("failed")
    FAILED,
}

/** How the attempts at the outbox's texts went, at most `MOST_TEXTS` at once. */
@Serializable
data class StatusesReport(val statuses: List<ReportedStatus>)

/** How the [attempt] at the text Dispatch knows as [id] went, at [at]; [error] says why it failed. */
@Serializable
data class ReportedStatus(val id: String, val attempt: Long, val status: SendStatus, val at: Long, val error: String? = null)

/**
 * How the phone stands, said with each wait on the outbox: the Set up [setup] steps undone, by
 * their keys; whether Android lets it [read] and [send] texts; its default texting app
 * ([textingApp]), by its package, where it has one; its [sim]; what [waiting] to go to Dispatch;
 * the picture messages received the texting app hasn't downloaded ([undownloaded]), where the
 * phone lets the app count them; and what last went wrong.
 */
@Serializable
data class Health(
    val setup: List<String>,
    val read: Boolean,
    val send: Boolean,
    val sim: Sim,
    val waiting: Backlog,
    val undownloaded: Int? = null,
    val textingApp: String? = null,
    val sendError: Lapse? = null,
    val unexpected: Lapse? = null,
)

/** The SIM the phone texts from: in it and with the network, not in it, or in it with no network. */
@Serializable
enum class Sim {
    @SerialName("ready")
    READY,

    @SerialName("absent")
    ABSENT,

    @SerialName("no_signal")
    NO_SIGNAL,
}

/** How many texts read, files of them, and statuses wait on the phone to go to Dispatch. */
@Serializable
data class Backlog(val texts: Int, val files: Int, val statuses: Int)

/** Something that went wrong: [error], when ([at]), and, for one the app didn't expect, what it was [doing]. */
@Serializable
data class Lapse(val error: String, val at: Long, @SerialName("where") val doing: String? = null)

/** What Dispatch answers a call that only needs doing. */
@Serializable
internal data class Done(val ok: Boolean)

/** How Dispatch refuses a call: the app acts on [error], never the message. */
@Serializable
internal data class RefusalAnswer(val error: String)
