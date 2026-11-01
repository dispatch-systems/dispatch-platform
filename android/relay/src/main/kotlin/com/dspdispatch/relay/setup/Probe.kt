package com.dspdispatch.relay.setup

import android.Manifest
import android.app.ActivityManager
import android.app.AppOpsManager
import android.content.Context
import android.content.Intent
import android.content.pm.PackageInstaller
import android.content.pm.PackageManager
import android.net.ConnectivityManager
import android.os.Build
import android.os.PowerManager
import android.os.Process
import android.provider.Telephony
import androidx.core.app.NotificationManagerCompat
import androidx.core.content.ContextCompat
import androidx.core.content.PackageManagerCompat
import androidx.core.content.UnusedAppRestrictionsConstants
import java.util.concurrent.ExecutionException

/** Reads what the phone says of each step. Its calls reach system services: call it off the main thread. */
class Probe(private val context: Context) {
    private val packages = context.packageManager
    private val name = context.packageName

    fun read(makerBatteryByHand: ByHand, chatFeaturesByHand: ByHand) = Facts(
        texting = TEXTING.all(::allowed),
        phoneNumber = allowed(Manifest.permission.READ_PHONE_NUMBERS),
        notificationsAllowed = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
            allowed(Manifest.permission.POST_NOTIFICATIONS)
        } else {
            null
        },
        notificationsOn = NotificationManagerCompat.from(context).areNotificationsEnabled(),
        backgroundRestricted = Build.VERSION.SDK_INT >= Build.VERSION_CODES.P &&
            context.getSystemService(ActivityManager::class.java).isBackgroundRestricted,
        ignoringBatteryOptimizations = context.getSystemService(PowerManager::class.java).isIgnoringBatteryOptimizations(name),
        makerBattery = makerBattery() != null,
        makerBatteryByHand = makerBatteryByHand,
        unusedApps = unusedApps(),
        backgroundData = context.getSystemService(ConnectivityManager::class.java).restrictBackgroundStatus !=
            ConnectivityManager.RESTRICT_BACKGROUND_STATUS_ENABLED,
        restricted = restricted(),
        textingApp = textingApp() != null,
        chatFeaturesByHand = chatFeaturesByHand,
        installUpdates = packages.canRequestPackageInstalls(),
    )

    /** The phone maker's own battery screen, where it has one that opens. */
    fun makerBattery(): Intent? = MakerScreens.find(packages, Build.MANUFACTURER)

    /** The phone's default texting app, where it has one that opens. */
    fun textingApp(): Intent? = Telephony.Sms.getDefaultSmsPackage(context)?.let(packages::getLaunchIntentForPackage)

    private fun allowed(permission: String) =
        ContextCompat.checkSelfPermission(context, permission) == PackageManager.PERMISSION_GRANTED

    /** Android 11+ only, as Android's own setting; Play services' backport to older phones is left out. */
    private fun unusedApps(): UnusedApps {
        val status = try {
            PackageManagerCompat.getUnusedAppRestrictionsStatus(context).get()
        } catch (_: ExecutionException) {
            UnusedAppRestrictionsConstants.ERROR
        } catch (_: InterruptedException) {
            UnusedAppRestrictionsConstants.ERROR
        }
        return when (status) {
            UnusedAppRestrictionsConstants.API_30, UnusedAppRestrictionsConstants.API_31 -> UnusedApps.ON
            UnusedAppRestrictionsConstants.DISABLED ->
                if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) UnusedApps.OFF else UnusedApps.NOT_AVAILABLE
            else -> UnusedApps.NOT_AVAILABLE
        }
    }

    /**
     * Android 15+ restricts the SMS permissions, among other settings, for an app installed from a
     * file, as an APK downloaded in a browser is, until "Allow restricted settings" in its App
     * info. (Android 13 and 14 restricted only settings this app never asks for: accessibility and
     * notification access.) Android keeps where the app stands in the
     * `access_restricted_settings` app op: errored, restricted; ignored, restricted and the
     * person told so, which shows App info's menu item; allowed, lifted; default, decided by
     * where the app was installed from.
     */
    private fun restricted(): Boolean? {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.VANILLA_ICE_CREAM) return null
        val fromFile = try {
            packages.getInstallSourceInfo(name).packageSource in FILE_SOURCES
        } catch (_: PackageManager.NameNotFoundException) {
            false
        }
        val mode = try {
            context.getSystemService(AppOpsManager::class.java)
                .unsafeCheckOpNoThrow(RESTRICTED_SETTINGS_OP, Process.myUid(), name)
        } catch (_: IllegalArgumentException) {
            // A phone whose Android doesn't know the app op: none of its settings is restricted.
            return null
        }
        return when (mode) {
            AppOpsManager.MODE_ERRORED, AppOpsManager.MODE_IGNORED -> true
            else -> if (fromFile) mode != AppOpsManager.MODE_ALLOWED else null
        }
    }

    companion object {
        /** What texting needs: reading, receiving and sending texts, and receiving pictures. */
        val TEXTING = arrayOf(
            Manifest.permission.READ_SMS,
            Manifest.permission.RECEIVE_SMS,
            Manifest.permission.SEND_SMS,
            Manifest.permission.RECEIVE_MMS,
        )

        /** Android's `AppOpsManager.OPSTR_ACCESS_RESTRICTED_SETTINGS`, which it doesn't publish. */
        private const val RESTRICTED_SETTINGS_OP = "android:access_restricted_settings"

        /** The install sources Android 15+ restricts, as its EnhancedConfirmationService has them. */
        private val FILE_SOURCES = setOf(PackageInstaller.PACKAGE_SOURCE_LOCAL_FILE, PackageInstaller.PACKAGE_SOURCE_DOWNLOADED_FILE)
    }
}
