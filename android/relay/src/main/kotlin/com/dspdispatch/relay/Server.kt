package com.dspdispatch.relay

import java.net.URI

/**
 * Which DSPs a build of the app pairs with, by their addresses, `https://<code>.<domain>`, the
 * DSP's short code alone before the server's domain: Production's build only Production's DSPs,
 * under [productionDomain]; Dev's build only DSPs elsewhere, so neither pairs with the other's.
 */
class Server(private val production: Boolean, private val productionDomain: String) {
    /** Whether [site], a DSP's address as its pairing code gives it, is one this build pairs with. */
    fun accepts(site: String): Boolean {
        val address = runCatching { URI(site) }.getOrNull() ?: return false
        val bare = address.rawUserInfo == null && address.port == -1 &&
            address.rawPath.orEmpty().let { it.isEmpty() || it == "/" } &&
            address.rawQuery == null && address.rawFragment == null
        val (code, domain) = address.host?.lowercase()?.split('.', limit = 2)
            ?.takeIf { it.size == 2 } ?: return false
        return address.scheme == "https" && bare && CODE.matches(code) && '.' in domain &&
            (domain == productionDomain) == production
    }

    companion object {
        /** This build's. */
        val THIS_BUILD = Server(BuildConfig.PRODUCTION, BuildConfig.PRODUCTION_DOMAIN)

        /** A DSP's short code, as Dispatch's `config::short_code` has it. */
        private val CODE = Regex("[a-z0-9]{2,16}")
    }
}
