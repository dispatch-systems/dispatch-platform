package com.dspdispatch.relay.setup

import androidx.annotation.StringRes
import com.dspdispatch.relay.R

/** What the app needs of the phone to run all day, every day, in the order Set up lists it. */
enum class Step(@param:StringRes val label: Int, @param:StringRes val undone: Int) {
    TEXTING(R.string.step_texting, R.string.undone_texting),
    PHONE_NUMBER(R.string.step_phone_number, R.string.undone_phone_number),
    NOTIFICATIONS(R.string.step_notifications, R.string.undone_notifications),
    BATTERY(R.string.step_battery, R.string.undone_battery),
    UNUSED_APPS(R.string.step_unused_apps, R.string.undone_unused_apps),
    BACKGROUND_DATA(R.string.step_background_data, R.string.undone_background_data),
    RESTRICTED_SETTINGS(R.string.step_restricted_settings, R.string.undone_restricted_settings),
    CHAT_FEATURES(R.string.step_chat_features, R.string.undone_chat_features),
    INSTALL_UPDATES(R.string.step_install_updates, R.string.undone_install_updates),
}

/** What a step's button does, and what it says. */
enum class Fix(@param:StringRes val label: Int) {
    ASK_TEXTING(R.string.allow),
    ASK_PHONE_NUMBER(R.string.allow),
    ASK_NOTIFICATIONS(R.string.allow),
    NOTIFICATION_SETTINGS(R.string.open),

    /** The settings of the first of the app's notification channels the person blocked. */
    CHANNEL_SETTINGS(R.string.open),
    BATTERY_REQUEST(R.string.allow),
    APP_INFO(R.string.open),
    MAKER_BATTERY(R.string.open),
    UNUSED_APPS(R.string.open),
    BACKGROUND_DATA(R.string.open),
    TEXTING_APP(R.string.open),
    INSTALL_UPDATES(R.string.open),

    /** A step Android can't report, marked done by hand once its screen has been opened. */
    CONFIRM(R.string.done),
}

/** Where a step Android can't report stands: never opened, opened, or marked done by hand. */
enum class ByHand {
    NONE,
    OPENED,
    DONE,
    ;

    /**
     * What the person saying it's done makes of it: done only once its screen was opened, so a
     * second tap, before the checklist caught up, never marks a screen never opened.
     */
    fun confirmed(): ByHand = if (this == OPENED) DONE else this
}

/** Whether the phone's unused-app restrictions apply to the app, if the phone has them. */
enum class UnusedApps { NOT_AVAILABLE, ON, OFF }

/**
 * What the phone says of each step, read at one moment ([Probe]); null where a step doesn't apply
 * to this phone.
 */
data class Facts(
    val texting: Boolean,
    val phoneNumber: Boolean,
    /**
     * Android 13+ only: whether the app may notify, whether its notifications are on, and whether
     * the channels it posts on, the relay's and Set up's, are.
     */
    val notificationsAllowed: Boolean?,
    val notificationsOn: Boolean,
    val channelsOn: Boolean,
    val backgroundRestricted: Boolean,
    val ignoringBatteryOptimizations: Boolean,
    /** Where each of the controls the phone's maker adds, that open here, stands, in order; none on most phones. */
    val makerBattery: List<ByHand>,
    val unusedApps: UnusedApps,
    val backgroundData: Boolean,
    /** Whether Android still restricts a setting the app needs; null where it never did. */
    val restricted: Boolean?,
    /** Whether the phone has a texting app to open. */
    val textingApp: Boolean,
    val chatFeaturesByHand: ByHand,
    val installUpdates: Boolean,
)

/** One step of Set up: whether it's done, and else what its buttons do, the last the main one. */
data class Item(val step: Step, val done: Boolean, val fixes: List<Fix>) {
    /** The main fix, as the running screen's banner offers it. */
    val fix: Fix? get() = fixes.lastOrNull()
}

/** The steps that apply to the phone, as [facts] has them. */
fun checklist(facts: Facts): List<Item> = with(facts) {
    listOfNotNull(
        item(Step.TEXTING, texting, Fix.ASK_TEXTING),
        item(Step.PHONE_NUMBER, phoneNumber, Fix.ASK_PHONE_NUMBER),
        notificationsAllowed?.let { notifications(it) },
        battery(),
        if (unusedApps == UnusedApps.NOT_AVAILABLE) null else item(Step.UNUSED_APPS, unusedApps == UnusedApps.OFF, Fix.UNUSED_APPS),
        item(Step.BACKGROUND_DATA, backgroundData, Fix.BACKGROUND_DATA),
        restricted?.let { item(Step.RESTRICTED_SETTINGS, !it, Fix.APP_INFO) },
        if (textingApp) byHand(Step.CHAT_FEATURES, chatFeaturesByHand, Fix.TEXTING_APP) else null,
        item(Step.INSTALL_UPDATES, installUpdates, Fix.INSTALL_UPDATES),
    )
}

/** Asked for first, then turned on for the app, then for each of its channels. */
private fun Facts.notifications(allowed: Boolean): Item = when {
    !allowed -> item(Step.NOTIFICATIONS, false, Fix.ASK_NOTIFICATIONS)
    !notificationsOn -> item(Step.NOTIFICATIONS, false, Fix.NOTIFICATION_SETTINGS)
    else -> item(Step.NOTIFICATIONS, channelsOn, Fix.CHANNEL_SETTINGS)
}

/**
 * Android's own restriction first, lifted on the app's Battery page where the request can't;
 * then each of the maker's controls, where it has them, in turn, which only the person can say is
 * done.
 */
private fun Facts.battery(): Item = when {
    backgroundRestricted -> item(Step.BATTERY, false, Fix.APP_INFO)
    !ignoringBatteryOptimizations -> item(Step.BATTERY, false, Fix.BATTERY_REQUEST)
    makerBattery.isNotEmpty() -> byHand(Step.BATTERY, makerBattery.firstOrNull { it != ByHand.DONE } ?: ByHand.DONE, Fix.MAKER_BATTERY)
    else -> item(Step.BATTERY, true, Fix.BATTERY_REQUEST)
}

private fun item(step: Step, done: Boolean, fix: Fix) = Item(step, done, if (done) emptyList() else listOf(fix))

private fun byHand(step: Step, state: ByHand, open: Fix) = when (state) {
    ByHand.NONE -> Item(step, false, listOf(open))
    ByHand.OPENED -> Item(step, false, listOf(open, Fix.CONFIRM))
    ByHand.DONE -> Item(step, true, emptyList())
}
