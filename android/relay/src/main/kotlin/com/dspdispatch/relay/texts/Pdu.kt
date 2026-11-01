package com.dspdispatch.relay.texts

import java.io.ByteArrayOutputStream

/** A part of a picture message: its type, its name, unique within the message, and its bytes. */
class PduPart(val mime: String, val name: String, val bytes: ByteArray)

/**
 * Picture messages in the bytes carriers take them in, as the OMA's MMS Encapsulation Protocol
 * (1.2) and WAP's WSP encoding have them: the M-Send.req the app sends, and the M-Send.conf the
 * carrier answers. Android has no public builder of them.
 */
object Pdu {
    /** The carrier took the message ([responseStatus]'s `Ok`). */
    const val RESPONSE_OK = 0x80

    /**
     * The M-Send.req of [parts] from this phone to [to], `multipart/related` starting at its
     * layout, [start], one of [parts]; [transaction] names it, unique among the phone's.
     */
    fun sendReq(to: String, transaction: String, date: Long, start: String, parts: List<PduPart>): ByteArray = Octets().apply {
        octet(MESSAGE_TYPE)
        octet(M_SEND_REQ)
        octet(TRANSACTION_ID)
        text(transaction)
        octet(MMS_VERSION)
        short(VERSION_1_2)
        octet(DATE)
        long(date)
        octet(FROM)
        sized { octet(INSERT_ADDRESS_TOKEN) }
        octet(TO)
        text("${to.filterIndexed { i, c -> c in '0'..'9' || (i == 0 && c == '+') }}$PLMN")
        octet(MESSAGE_CLASS)
        octet(PERSONAL)
        octet(EXPIRY)
        sized {
            octet(RELATIVE_TOKEN)
            long(EXPIRY_SECONDS)
        }
        octet(PRIORITY)
        octet(NORMAL)
        octet(DELIVERY_REPORT)
        octet(NO)
        octet(READ_REPORT)
        octet(NO)
        // The content type comes last of the headers, then the body.
        octet(CONTENT_TYPE)
        sized {
            short(MULTIPART_RELATED)
            octet(START)
            text("<$start>")
            octet(TYPE)
            text(SMIL)
        }
        uintvar(parts.size.toLong())
        for (part in parts) {
            val headers = Octets().apply {
                sized {
                    text(part.mime)
                    if (part.mime.startsWith(TEXT)) {
                        octet(CHARSET)
                        short(UTF_8)
                    }
                    octet(NAME)
                    text(part.name)
                }
                octet(CONTENT_ID)
                quoted("<${part.name}>")
                octet(CONTENT_LOCATION)
                text(part.name)
            }.bytes()
            uintvar(headers.size.toLong())
            uintvar(part.bytes.size.toLong())
            bytes(headers)
            bytes(part.bytes)
        }
    }.bytes()

    /** The X-Mms-Response-Status of the M-Send.conf [pdu], or null when it has none. */
    fun responseStatus(pdu: ByteArray): Int? {
        var at = 0
        while (at + 1 < pdu.size) {
            val field = pdu[at].toInt() and BYTE
            val value = pdu[at + 1].toInt() and BYTE
            if (field and SHORT_FLAG == 0) return null
            if (field == RESPONSE_STATUS) return value.takeIf { it and SHORT_FLAG != 0 }
            // Past the value, whichever of WSP's forms it takes.
            at = when {
                value < LENGTH_QUOTE -> at + 2 + value
                value == LENGTH_QUOTE -> uintvar(pdu, at + 2).let { (length, next) -> next + length.toInt() }
                value < SHORT_FLAG -> pdu.endOfText(at + 1) + 1
                else -> at + 2
            }
            if (at <= 0) return null
        }
        return null
    }

    /** Where the text-string at [from] ends, its 0, or -1 when it doesn't. */
    private fun ByteArray.endOfText(from: Int) = (from until size).firstOrNull { this[it].toInt() == 0 } ?: -1

    /** The uintvar at [from] in [pdu], and where the next value starts. */
    private fun uintvar(pdu: ByteArray, from: Int): Pair<Long, Int> {
        var value = 0L
        var at = from
        while (at < pdu.size) {
            val octet = pdu[at++].toInt() and BYTE
            value = (value shl SEVEN_BITS) or (octet and LOW_BITS).toLong()
            if (octet and SHORT_FLAG == 0) return value to at
        }
        return value to pdu.size
    }

    /** WSP's encodings of the values a picture message's headers take. */
    private class Octets {
        private val out = ByteArrayOutputStream()

        fun bytes(): ByteArray = out.toByteArray()

        fun octet(octet: Int) = out.write(octet)

        fun bytes(bytes: ByteArray) = out.write(bytes)

        fun short(value: Int) = octet(value or SHORT_FLAG)

        fun long(value: Long) {
            val octets = generateSequence(value) { it ushr Byte.SIZE_BITS }.takeWhile { it != 0L }
                .map { (it and BYTE.toLong()).toInt() }.toList().reversed().ifEmpty { listOf(0) }
            octet(octets.size)
            octets.forEach(::octet)
        }

        fun uintvar(value: Long) {
            val groups = generateSequence(value) { it ushr SEVEN_BITS }.takeWhile { it != 0L }
                .map { (it and LOW_BITS.toLong()).toInt() }.toList().reversed().ifEmpty { listOf(0) }
            groups.forEachIndexed { i, group -> octet(if (i < groups.lastIndex) group or SHORT_FLAG else group) }
        }

        /** A text-string: quoted when its first octet would read as a short integer. */
        fun text(value: String) {
            val bytes = value.toByteArray(Charsets.UTF_8)
            if (bytes.isNotEmpty() && bytes[0].toInt() and SHORT_FLAG != 0) octet(TEXT_QUOTE)
            bytes(bytes)
            octet(0)
        }

        fun quoted(value: String) {
            octet(QUOTE)
            bytes(value.toByteArray(Charsets.UTF_8))
            octet(0)
        }

        /** What [value] writes, after its value-length. */
        fun sized(value: Octets.() -> Unit) {
            val bytes = Octets().apply(value).bytes()
            if (bytes.size < LENGTH_QUOTE) {
                octet(bytes.size)
            } else {
                octet(LENGTH_QUOTE)
                uintvar(bytes.size.toLong())
            }
            bytes(bytes)
        }
    }

    // The headers' field codes, each with its high bit set.
    private const val CONTENT_TYPE = 0x84
    private const val DATE = 0x85
    private const val DELIVERY_REPORT = 0x86
    private const val EXPIRY = 0x88
    private const val FROM = 0x89
    private const val MESSAGE_CLASS = 0x8A
    private const val MESSAGE_TYPE = 0x8C
    private const val MMS_VERSION = 0x8D
    private const val PRIORITY = 0x8F
    private const val READ_REPORT = 0x90
    private const val RESPONSE_STATUS = 0x92
    private const val TO = 0x97
    private const val TRANSACTION_ID = 0x98

    // Their values.
    private const val M_SEND_REQ = 0x80
    private const val VERSION_1_2 = 0x12
    private const val INSERT_ADDRESS_TOKEN = 0x81
    private const val PERSONAL = 0x80
    private const val RELATIVE_TOKEN = 0x81
    private const val EXPIRY_SECONDS = 7 * 24 * 60 * 60L
    private const val NORMAL = 0x81
    private const val NO = 0x81
    private const val PLMN = "/TYPE=PLMN"
    private const val MULTIPART_RELATED = 0x33
    private const val SMIL = "application/smil"
    private const val TEXT = "text/"
    private const val UTF_8 = 106

    // The parameters of a content type, and the headers of a part.
    private const val CHARSET = 0x81
    private const val NAME = 0x85
    private const val TYPE = 0x89
    private const val START = 0x8A
    private const val CONTENT_LOCATION = 0x8E
    private const val CONTENT_ID = 0xC0

    // WSP's own.
    private const val SHORT_FLAG = 0x80
    private const val LOW_BITS = 0x7F
    private const val SEVEN_BITS = 7
    private const val BYTE = 0xFF
    private const val LENGTH_QUOTE = 31
    private const val TEXT_QUOTE = 0x7F
    private const val QUOTE = 0x22
}
