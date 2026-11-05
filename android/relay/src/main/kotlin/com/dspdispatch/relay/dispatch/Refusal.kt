package com.dspdispatch.relay.dispatch

/** Dispatch answered a call with a refusal: its HTTP [status] and its [code]. */
class Refusal(val status: Int, val code: String) : Exception("$status $code") {
    /** Whether the same call may simply be made again after a pause. */
    val passing get() = status >= 500 || status == 408 || status == 429

    /**
     * Whether the DSP removed this phone, so its key works no more. Not `sign_in_required`: Dispatch
     * answers that while the DSP is suspended too, and the key works again once it's resumed.
     */
    val removed get() = code == PHONES_REMOVED

    companion object {
        const val PHONES_REMOVED = "phones_removed"
        const val SIGN_IN_REQUIRED = "sign_in_required"
        const val PAIRING_INVALID = "phones_pairing_invalid"
        const val PHONES_OFF = "phones_off"
        const val INVALID_INPUT = "invalid_input"
        const val REQUEST_TOO_LARGE = "request_too_large"
        const val FILE_NOT_FOUND = "phones_file_not_found"
        const val UPLOAD_TOO_LARGE = "upload_too_large"
        const val UPLOAD_INCOMPLETE = "upload_incomplete"
    }
}
