package com.dspdispatch.relay.setup

import android.Manifest
import android.app.ActivityManager
import android.app.admin.DevicePolicyManager
import android.content.Context
import android.content.Intent
import android.content.pm.PackageInstaller
import android.content.pm.PackageManager
import android.net.ConnectivityManager
import android.os.Build
import android.os.PowerManager
import android.provider.Telephony
import androidx.annotation.RequiresApi
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
     * of each maker's control by its name, and of chat features; and whether Android has let the
     * person choose on texting ([textingUnrestricted]).
     */
    fun read(makerBatteryByHand: (String) -> ByHand, chatFeaturesByHand: ByHand, textingUnrestricted: Boolean): Facts {
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
            restricted = restricted(texting, textingUnrestricted),
            textingApp = textingApp() != null,
            chatFeaturesByHand = chatFeaturesByHand,
            installUpdates = packages.canRequestPackageInstalls(),
        )
    }

    /** The controls the phone's maker adds that open here, by name in order, each with its screen. */
    fun makerBattery(): Map<String, Intent> = MakerScreens.find(
        packages,
        Build.MANUFACTURER,
        ThisApp(name, context.applicationInfo.loadLabel(packages).toString()),
    )

    /** The phone's default texting app, where it has one that opens. */
    fun textingApp(): Intent? = Telephony.Sms.getDefaultSmsPackage(context)?.let(packages::getLaunchIntentForPackage)

    private fun allowed(permission: String) =
        ContextCompat.checkSelfPermission(context, permission) == PackageManager.PERMISSION_GRANTED

    /** Android 11+ only, as Android's own setting; Play services' backport to older phones is left out. */
    private fun unusedApps(): UnusedApps {
        if (managed()) return UnusedApps.NOT_AVAILABLE
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
     * Whether a device owner manages the phone, as an organization's device management does: Android
     * then exempts every app from its unused-app restrictions and locks the setting off, unless
     * the phone is a lender's, which no app but the owner may ask (Android's permission controller,
     * `isPackageHibernationExemptBySystem`).
     */
    private fun managed(): Boolean {
        val policy = context.getSystemService(DevicePolicyManager::class.java) ?: return false
        return policy.activeAdmins.orEmpty().any { policy.isDeviceOwnerApp(it.packageName) }
    }

    /**
     * Android 15+ restricts the SMS permissions, among other settings, for an app installed from a
     * file, as an APK downloaded in a browser is, until "Allow restricted settings" in its App
     * info, which shows once the person has tried to give one. (Android 13 and 14 restricted only
     * settings this app never asks for: accessibility and notification access.) Android lets no
     * app read whether it was lifted, so it's known lifted only once Android let the person choose
     * on texting ([unrestricted]), which it does only then; until then it may stand.
     */
    private fun restricted(texting: Boolean, unrestricted: Boolean): Boolean? =
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.VANILLA_ICE_CREAM) null else restriction(fromFile(), texting, unrestricted)

    /** Whether the app was installed from a file, an install source Android restricts. */
    @RequiresApi(Build.VERSION_CODES.VANILLA_ICE_CREAM)
    private fun fromFile() = try {
        packages.getInstallSourceInfo(name).packageSource.let {
            it == PackageInstaller.PACKAGE_SOURCE_LOCAL_FILE || it == PackageInstaller.PACKAGE_SOURCE_DOWNLOADED_FILE
        }
    } catch (_: PackageManager.NameNotFoundException) {
        false
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

/**
 * Whether Android may still restrict texting for the app: only where it was installed [fromFile],
 * and only until texting is allowed or Android let the person choose on it, which it does only
 * once it lifted the restriction ([unrestricted]); null where it never did. Without that record,
 * as on a phone updated from a build that didn't keep it, whether it was lifted isn't known.
 */
internal fun restriction(fromFile: Boolean, texting: Boolean, unrestricted: Boolean): Boolean? =
    if (fromFile) !texting && !unrestricted else null
