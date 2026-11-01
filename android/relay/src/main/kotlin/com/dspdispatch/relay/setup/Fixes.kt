package com.dspdispatch.relay.setup

import android.Manifest
import android.annotation.SuppressLint
import android.content.ActivityNotFoundException
import android.content.Intent
import android.net.Uri
import android.os.Build
import android.provider.Settings
import androidx.activity.ComponentActivity
import androidx.activity.result.contract.ActivityResultContracts
import androidx.core.content.IntentCompat
import androidx.lifecycle.lifecycleScope
import com.dspdispatch.relay.relay
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

/**
 * What a step's buttons do, on [activity]: asks for a permission, or opens the Android screen that
 * changes the step, and checks the steps again as the person comes back. Make it as the activity
 * is made, which Android needs of what an activity asks of another.
 */
class Fixes(private val activity: ComponentActivity) {
    // The activity's context, and the app's parts with it, come only once it's created.
    private val setup get() = activity.relay.setup
    private val name get() = activity.packageName

    private val permissions = activity.registerForActivityResult(ActivityResultContracts.RequestMultiplePermissions()) { results ->
        val refused = results.filterValues { !it }.keys
        // Once a permission is refused for good, Android asks no more and answers at once: App
        // info is then the only place to give it. Android 15+ answers so too for a texting
        // permission it restricts, whose "Allow restricted settings" is in App info as well.
        if (refused.isNotEmpty() && refused.none(activity::shouldShowRequestPermissionRationale)) open(details(Settings.ACTION_APPLICATION_DETAILS_SETTINGS))
        recheck()
    }
    private val screen = activity.registerForActivityResult(ActivityResultContracts.StartActivityForResult()) { recheck() }

    /** Does what [fix] does for [item]. */
    fun apply(item: Item, fix: Fix) {
        when (fix) {
            Fix.ASK_TEXTING -> permissions.launch(Probe.TEXTING)
            Fix.ASK_PHONE_NUMBER -> permissions.launch(arrayOf(Manifest.permission.READ_PHONE_NUMBERS))
            Fix.ASK_NOTIFICATIONS -> if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
                permissions.launch(arrayOf(Manifest.permission.POST_NOTIFICATIONS))
            }
            Fix.NOTIFICATION_SETTINGS ->
                open(Intent(Settings.ACTION_APP_NOTIFICATION_SETTINGS).putExtra(Settings.EXTRA_APP_PACKAGE, name))
            Fix.BATTERY_REQUEST -> open(batteryRequest())
            Fix.APP_INFO -> open(details(Settings.ACTION_APPLICATION_DETAILS_SETTINGS))
            Fix.UNUSED_APPS -> open(IntentCompat.createManageUnusedAppRestrictionsIntent(activity, name))
            Fix.BACKGROUND_DATA -> open(details(Settings.ACTION_IGNORE_BACKGROUND_DATA_RESTRICTIONS_SETTINGS))
            Fix.INSTALL_UPDATES -> open(details(Settings.ACTION_MANAGE_UNKNOWN_APP_SOURCES))
            Fix.MAKER_BATTERY -> openByHand(item.step) { setup.probe.makerBattery() }
            Fix.TEXTING_APP -> openByHand(item.step) { setup.probe.textingApp() }
            Fix.CONFIRM -> {
                setup.confirmed(item.step)
                recheck()
            }
        }
    }

    // Lint warns that Google Play allows this request only to apps that need it. Relaying texts
    // around the clock is what this app is for, and it isn't in Google Play.
    @SuppressLint("BatteryLife")
    private fun batteryRequest() = details(Settings.ACTION_REQUEST_IGNORE_BATTERY_OPTIMIZATIONS)

    /** Android's screen [action] for this app. */
    private fun details(action: String) = Intent(action, Uri.fromParts("package", name, null))

    /** Opens a screen of a step only the person can say is done, which can then be marked done. */
    private fun openByHand(step: Step, find: () -> Intent?) {
        activity.lifecycleScope.launch {
            val intent = withContext(Dispatchers.IO) { find() } ?: return@launch
            setup.opened(step)
            open(intent)
        }
    }

    /** Opens [intent], or this app's App info where the phone lacks that screen or keeps it from apps. */
    private fun open(intent: Intent) {
        try {
            screen.launch(intent)
        } catch (_: ActivityNotFoundException) {
            appInfoInstead(intent)
        } catch (_: SecurityException) {
            appInfoInstead(intent)
        }
    }

    private fun appInfoInstead(intent: Intent) {
        if (intent.action != Settings.ACTION_APPLICATION_DETAILS_SETTINGS) open(details(Settings.ACTION_APPLICATION_DETAILS_SETTINGS))
    }

    private fun recheck() {
        activity.lifecycleScope.launch { setup.check() }
    }
}
