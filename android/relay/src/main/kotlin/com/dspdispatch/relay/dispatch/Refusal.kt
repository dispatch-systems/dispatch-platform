package com.dspdispatch.relay.dispatch

/** Dispatch answered a call with a refusal: its HTTP [status] and its [code]. */
class Refusal(val status: Int, val code: String) : Exception("$status $code") {
    /** Whether the same call may simply be made again after a pause. */
    val passing get() = status >= 500 || status == 408 || status == 429

    /** Whether this phone isn't paired with the DSP anymore: it was removed, or its key is unknown. */
    val unpaired get() = code == PHONES_REMOVED || code == SIGN_IN_REQUIRED

    companion object {
        const val PHONES_REMOVED = "phones_removed"
        const val SIGN_IN_REQUIRED = "sign_in_required"
        const val PAIRING_INVALID = "phones_pairing_invalid"
        const val PHONES_OFF = "phones_off"
    }
}
