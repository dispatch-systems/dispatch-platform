package com.dspdispatch.relay.pairing

import java.net.URLDecoder

/**
 * What a DSP's pairing QR code holds, `dispatch-phone:pair?site=<address>&code=<code>`: the DSP's
 * address, and the one-time code that pairs a phone with it.
 */
data class PairingLink(val site: String, val code: String) {
    companion object {
        const val SCHEME = "dispatch-phone"
        private const val PREFIX = "$SCHEME:pair?"

        /** The link [text] holds, or null when it isn't one. */
        fun parse(text: String): PairingLink? {
            if (!text.startsWith(PREFIX)) return null
            val fields = text.removePrefix(PREFIX).split('&').mapNotNull { field ->
                val (name, value) = field.split('=', limit = 2).takeIf { it.size == 2 } ?: return@mapNotNull null
                name to runCatching { URLDecoder.decode(value, Charsets.UTF_8.name()) }.getOrNull()
            }.toMap()
            val site = fields["site"]?.takeIf { it.isNotBlank() } ?: return null
            val code = fields["code"]?.takeIf { it.isNotBlank() } ?: return null
            return PairingLink(site, code)
        }
    }
}
