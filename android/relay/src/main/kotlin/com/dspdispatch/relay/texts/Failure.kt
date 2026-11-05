package com.dspdispatch.relay.texts

/**
 * Why a text of the outbox failed, as Dispatch shows it to the DSP's staff: short, and the same
 * whatever the phone's language.
 */
object Failure {
    const val NOT_ALLOWED = "Sending texts isn't allowed on the phone"
    const val FILE_GONE = "Picture no longer available"
    const val TOO_LARGE = "Too large for a picture message"
    const val MMS_OFF = "Picture messages are off for this carrier"
    const val NO_RESULT = "The phone never said whether it sent"
    const val NOT_SENT = "Couldn't send"
    const val MMS_REFUSED = "The carrier refused the picture message"
    const val FILE_UNAVAILABLE = "Couldn't download the picture"

    /** Past the time Dispatch gave it to start by: its code, which Dispatch tells in its own words. */
    const val EXPIRED = "expired"

    /** Why Android failed a part of a text, by its result code (`SmsManager.RESULT_*`), or null for sent. */
    fun sms(code: Int): String? = when (code) {
        RESULT_OK -> null
        RADIO_OFF -> "Phone's radio is off"
        NO_SERVICE -> "No service"
        LIMIT_EXCEEDED -> "Android's sending limit reached"
        FDN_CHECK_FAILURE -> "Blocked by the phone's fixed dialing numbers"
        SHORT_CODE_NOT_ALLOWED, SHORT_CODE_NEVER_ALLOWED -> "Short code not allowed"
        RADIO_NOT_AVAILABLE -> "Phone's radio unavailable"
        NETWORK_REJECT -> "The network refused it"
        BLOCKED_DURING_EMERGENCY -> "Blocked during an emergency call"
        else -> "$NOT_SENT (code $code)"
    }

    /** Why Android failed a picture message, by its result code (`SmsManager.MMS_ERROR_*`), or null for sent. */
    fun mms(code: Int): String? = when (code) {
        RESULT_OK -> null
        MMS_INVALID_APN, MMS_CONFIGURATION_ERROR -> "Picture messages aren't set up on the phone"
        MMS_UNABLE_CONNECT, MMS_HTTP_FAILURE -> "Couldn't reach the carrier's picture message server"
        MMS_NO_DATA_NETWORK, MMS_DATA_DISABLED -> "Mobile data is off"
        MMS_INVALID_SUBSCRIPTION, MMS_INACTIVE_SUBSCRIPTION -> "No SIM to send from"
        MMS_DISABLED_BY_CARRIER -> MMS_OFF
        else -> "$NOT_SENT (code $code)"
    }

    // Android's result codes, as SmsManager has them: most are from after the oldest Android the app
    // runs on, and each means the same on every version.
    private const val RESULT_OK = -1 // Activity.RESULT_OK
    private const val RADIO_OFF = 2
    private const val NO_SERVICE = 4
    private const val LIMIT_EXCEEDED = 5
    private const val FDN_CHECK_FAILURE = 6
    private const val SHORT_CODE_NOT_ALLOWED = 7
    private const val SHORT_CODE_NEVER_ALLOWED = 8
    private const val RADIO_NOT_AVAILABLE = 9
    private const val NETWORK_REJECT = 10
    private const val BLOCKED_DURING_EMERGENCY = 29
    private const val MMS_INVALID_APN = 2
    private const val MMS_UNABLE_CONNECT = 3
    private const val MMS_HTTP_FAILURE = 4
    private const val MMS_CONFIGURATION_ERROR = 7
    private const val MMS_NO_DATA_NETWORK = 8
    private const val MMS_INVALID_SUBSCRIPTION = 9
    private const val MMS_INACTIVE_SUBSCRIPTION = 10
    private const val MMS_DATA_DISABLED = 11
    private const val MMS_DISABLED_BY_CARRIER = 12
}
