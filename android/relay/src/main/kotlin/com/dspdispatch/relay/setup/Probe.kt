package com.dspdispatch.relay.setup

import android.Manifest
import android.app.ActivityManager
import android.content.Context
import android.content.Intent
import android.content.pm.PackageInstaller
import android.content.pm.PackageManager
import android.net.ConnectivityManager
import android.os.Build
import android.os.PowerManager
import android.provider.Telephony
import androidx.core.app.NotificationManagerCompat
import androidx.core.content.ContextCompat
import androidx.core.content.PackageManagerCompat
import androidx.core.content.UnusedAppRestrictionsConstants
import com.dspdispatch.relay.Notifications
import java.util.concurrent.ExecutionException

/** Reads what the phone says of each step. Its calls reach system services: call it off the main thread. */
class Probe(private val context: Context) {
    private val packages = context.packageManager
    private val name = context.packageName

    /**
     * What the phone says of each step now, with what the person has said of those only they can:
     * of each maker's control by its name, and of chat features.
     */
    fun read(makerBatteryByHand: (String) -> ByHand, chatFeaturesByHand: ByHand): Facts {
        val texting = TEXTING.all(::allowed)
        return Facts(
            texting = texting,
            phoneNumber = allowed(Manifest.permission.READ_PHONE_NUMBERS),
            notificationsAllowed = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
                allowed(Manifest.permission.POST_NOTIFICATIONS)
            } else {
                null
            },
            notificationsOn = NotificationManagerCompat.from(context).areNotificationsEnabled(),
            channelsOn = Notifications.blocked(context).isEmpty(),
            backgroundRestricted = Build.VERSION.SDK_INT >= Build.VERSION_CODES.P &&
                context.getSystemService(ActivityManager::class.java).isBackgroundRestricted,
            ignoringBatteryOptimizations = context.getSystemService(PowerManager::class.java).isIgnoringBatteryOptimizations(name),
            makerBattery = makerBattery().keys.map(makerBatteryByHand),
            unusedApps = unusedApps(),
            backgroundData = context.getSystemService(ConnectivityManager::class.java).restrictBackgroundStatus !=
                ConnectivityManager.RESTRICT_BACKGROUND_STATUS_ENABLED,
            restricted = restricted(texting),
            textingApp = textingApp() != null,
            chatFeaturesByHand = chatFeaturesByHand,
            installUpdates = packages.canRequestPackageInstalls(),
        )
    }

    /** The controls the phone's maker adds that open here, by name in order, each with its screen. */
    fun makerBattery(): Map<String, Intent> = MakerScreens.find(packages, Build.MANUFACTURER)

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
     * info, which shows once the person has tried to give one. (Android 13 and 14 restricted only
     * settings this app never asks for: accessibility and notification access.) Android lets no
     * app read whether its restriction was lifted, so it stands until texting is allowed, the
     * only restricted setting the app needs.
     */
    private fun restricted(texting: Boolean): Boolean? {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.VANILLA_ICE_CREAM) return null
        // The install sources Android restricts, as its EnhancedConfirmationService has them.
        val fromFile = try {
            packages.getInstallSourceInfo(name).packageSource.let {
                it == PackageInstaller.PACKAGE_SOURCE_LOCAL_FILE || it == PackageInstaller.PACKAGE_SOURCE_DOWNLOADED_FILE
            }
        } catch (_: PackageManager.NameNotFoundException) {
            false
        }
        return if (fromFile) !texting else null
    }

    companion object {
        /** What texting needs: reading, receiving and sending texts, and receiving pictures. */
        val TEXTING = arrayOf(
            Manifest.permission.READ_SMS,
            Manifest.permission.RECEIVE_SMS,
            Manifest.permission.SEND_SMS,
            Manifest.permission.RECEIVE_MMS,
        )
    }
}
