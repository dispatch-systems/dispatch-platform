package com.dspdispatch.relay.texts

import com.dspdispatch.relay.dispatch.ReportedFile
import com.dspdispatch.relay.dispatch.ReportedText
import com.dspdispatch.relay.dispatch.TextsReport
import com.dspdispatch.relay.dispatch.wire

/** As much of [waiting], oldest first, as one report carries: its count and its size both. */
internal fun batch(waiting: List<WaitingText>): TextsReport {
    var bytes = EMPTY_BYTES
    val texts = mutableListOf<ReportedText>()
    for (text in waiting.asSequence().take(MOST_TEXTS).map(WaitingText::reported)) {
        val more = wire.encodeToString(text).toByteArray().size + if (texts.isEmpty()) 0 else 1
        // The first goes alone even past the size, for Dispatch to refuse, and so leave, alone.
        if (texts.isNotEmpty() && bytes + more > MOST_BYTES) break
        bytes += more
        texts += text
    }
    return TextsReport(texts)
}

/** The most texts one report carries (`MOST_TEXTS`). */
internal const val MOST_TEXTS = 100

/** The most bytes of JSON one call carries, core's request body limit. */
private const val MOST_BYTES = 64 * 1024

/** A report's bytes but its texts, `{"texts":[]}`. */
private val EMPTY_BYTES = wire.encodeToString(TextsReport(emptyList())).length

private fun WaitingText.reported() = ReportedText(
    text.clientId,
    text.direction,
    text.number,
    text.body,
    text.at,
    files.sortedBy(WaitingFile::position).map { ReportedFile(it.name, it.mime, it.size) },
)
