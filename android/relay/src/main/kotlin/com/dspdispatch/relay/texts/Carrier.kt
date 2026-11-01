package com.dspdispatch.relay.texts

import android.telephony.SmsManager

/** A carrier's texting settings, as Android gives them (`SmsManager.getCarrierConfigValues`). */
interface CarrierConfig {
    /** The setting [name], or [default] when the carrier doesn't say. */
    fun flag(name: String, default: Boolean): Boolean

    /** The setting [name], 0 when the carrier doesn't say. */
    fun number(name: String): Int
}

/** What a carrier takes of a picture message: at most [most] bytes, each picture within [picture]. */
data class MmsLimits(val most: Int, val picture: Size)

/**
 * What the carrier of [config] takes of a picture message, or null when it takes none: Android's
 * own where it doesn't say, or where Android didn't answer, and pictures no larger than the app
 * sends them.
 */
internal fun mmsLimits(config: CarrierConfig?): MmsLimits? {
    if (config?.flag(SmsManager.MMS_CONFIG_MMS_ENABLED, true) == false) return null
    fun setting(name: String, default: Int) = config?.number(name)?.takeIf { it > 0 } ?: default
    return MmsLimits(
        setting(SmsManager.MMS_CONFIG_MAX_MESSAGE_SIZE, DEFAULT_MMS_MOST),
        Size(setting(SmsManager.MMS_CONFIG_MAX_IMAGE_WIDTH, LARGEST_SIDE), setting(SmsManager.MMS_CONFIG_MAX_IMAGE_HEIGHT, LARGEST_SIDE)),
    )
}

/**
 * Whether a text of [parts] parts goes as one message, its parts joined, by the carrier of
 * [config]: some take a long text's parts only as texts of their own.
 */
internal fun joined(parts: Int, config: CarrierConfig?): Boolean =
    parts > 1 && config?.flag(SmsManager.MMS_CONFIG_SEND_MULTIPART_SMS_AS_SEPARATE_MESSAGES, false) != true

/** Android's own when a carrier doesn't say (`MmsConfig`'s maxMessageSize). */
private const val DEFAULT_MMS_MOST = 300 * 1024
