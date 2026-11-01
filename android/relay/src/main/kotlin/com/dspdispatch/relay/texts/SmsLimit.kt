package com.dspdispatch.relay.texts

/**
 * Android's limit on the SMS parts an app that isn't the phone's texting app sends: at most [most]
 * in any [windowMs], past which it holds each text behind an "Allow / Don't allow" prompt on the
 * phone (`SmsUsageMonitor`). A picture message isn't counted.
 */
data class SmsLimit(val most: Int, val windowMs: Long) {
    /**
     * Whether a text of [parts] parts would come too near the limit, [counted] parts handed since
     * [since]: it's kept [MARGIN_PARTS] short, Android's count being its own, not the app's to read.
     */
    fun crossedBy(counted: Int, parts: Int) = counted + parts > most - MARGIN_PARTS

    /**
     * From when the parts handed count for a text handed [at]: the window, and [SLACK_MS] more, as
     * Android stamps each part a moment after the app counts it, so keeps it a moment longer.
     */
    fun since(at: Long) = at - windowMs - SLACK_MS

    companion object {
        /** Android's own where the phone doesn't say otherwise (`SmsUsageMonitor`'s defaults). */
        val ANDROID = SmsLimit(30, 30 * 60 * 1000L)

        /** The phone's settings of the limit, as `Settings.Global` names them. */
        const val MOST_SETTING = "sms_outgoing_check_max_count"
        const val WINDOW_SETTING = "sms_outgoing_check_interval_ms"

        private const val MARGIN_PARTS = 2
        private const val SLACK_MS = 60_000L

        /**
         * The limit as the phone's [setting]s have it, each Android's own where the phone doesn't
         * say or won't let the app read it (null), or says nothing usable.
         */
        fun of(setting: (String) -> String?): SmsLimit = SmsLimit(
            setting(MOST_SETTING)?.toIntOrNull()?.takeIf { it > 0 } ?: ANDROID.most,
            setting(WINDOW_SETTING)?.toLongOrNull()?.takeIf { it > 0 } ?: ANDROID.windowMs,
        )
    }
}
