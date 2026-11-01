package com.dspdispatch.relay.setup

import android.content.ComponentName
import android.content.Intent
import android.content.pm.PackageManager

/** An action a maker documents for opening one of its screens: [action], answered by [pkg], with [extras]. */
data class Action(val pkg: String, val action: String, val extras: Map<String, Int> = emptyMap())

/**
 * One of a maker's controls over running in the background, by [name], which only the person can
 * say is set: the action the maker documents for it, if any, then the screens that may hold it on
 * its phones, best first, as `package/class`. A screen that sets one app's own policy is told
 * which ([forTheApp]): its package and label.
 */
data class Control(val name: String, val screens: List<String>, val action: Action? = null, val forTheApp: Boolean = false)

/** The app a screen [Control.forTheApp] is told to set: its package and the label it shows by. */
data class ThisApp(val pkg: String, val label: String)

/**
 * The controls phone makers add to keep apps from running in the background, beyond Android's own
 * battery optimization, by maker: as dontkillmyapp.com and the apps that open them have documented
 * them. Each control is set on its own; its screens are the same control on other models. Each
 * package is named in the manifest's `<queries>`, which Android 11+ needs to see it.
 */
object MakerScreens {
    private val xiaomi = listOf(
        Control("autostart", listOf("com.miui.securitycenter/com.miui.permcenter.autostart.AutoStartManagementActivity")),
        Control("battery_saver", listOf("com.miui.powerkeeper/com.miui.powerkeeper.ui.HiddenAppsConfigActivity"), forTheApp = true),
    )
    private val huawei = listOf(
        "com.huawei.systemmanager/com.huawei.systemmanager.startupmgr.ui.StartupNormalAppListActivity",
        "com.huawei.systemmanager/com.huawei.systemmanager.optimize.process.ProtectActivity",
        "com.huawei.systemmanager/com.huawei.systemmanager.appcontrol.activity.StartupAppControlActivity",
    )
    private val colorOsStartup = listOf(
        "com.coloros.safecenter/com.coloros.safecenter.permission.startup.StartupAppListActivity",
        "com.coloros.safecenter/com.coloros.safecenter.startupapp.StartupAppListActivity",
        "com.oppo.safe/com.oppo.safe.permission.startup.StartupAppListActivity",
    )
    private val colorOsBattery =
        Control("battery_saver", listOf("com.coloros.oppoguardelf/com.coloros.powermanager.fuelgaue.PowerUsageModelActivity"))
    private val vivo = listOf(
        Control(
            "autostart",
            listOf(
                "com.vivo.permissionmanager/com.vivo.permissionmanager.activity.BgStartUpManagerActivity",
                "com.iqoo.secure/com.iqoo.secure.ui.phoneoptimize.BgStartUpManager",
            ),
        ),
        Control("background_power", listOf("com.iqoo.secure/com.iqoo.secure.ui.phoneoptimize.AddWhiteListActivity")),
    )
    private val lenovo = listOf(Control("background", listOf("com.lenovo.security/com.lenovo.security.purebackground.PureBackgroundActivity")))
    private val transsion = listOf(Control("autostart", listOf("com.transsion.phonemaster/com.cyin.himgr.autostart.AutoStartActivity")))

    /** By `Build.MANUFACTURER`, lowercase. */
    private val byMaker: Map<String, List<Control>> = mapOf(
        "samsung" to listOf(
            Control(
                "never_sleeping",
                listOf(
                    "com.samsung.android.lool/com.samsung.android.sm.battery.ui.BatteryActivity",
                    "com.samsung.android.lool/com.samsung.android.sm.ui.battery.BatteryActivity",
                    "com.samsung.android.sm/com.samsung.android.sm.ui.battery.BatteryActivity",
                ),
                // Samsung's app-management API: its Never sleeping apps list.
                Action("com.samsung.android.lool", "com.samsung.android.sm.ACTION_OPEN_CHECKABLE_LISTACTIVITY", mapOf("activity_type" to 2)),
            ),
        ),
        "xiaomi" to xiaomi,
        "redmi" to xiaomi,
        "poco" to xiaomi,
        "huawei" to listOf(Control("app_launch", huawei)),
        "honor" to listOf(
            Control("app_launch", listOf("com.hihonor.systemmanager/com.hihonor.systemmanager.startupmgr.ui.StartupNormalAppListActivity") + huawei),
        ),
        "oppo" to listOf(Control("autostart", colorOsStartup), colorOsBattery),
        "realme" to listOf(Control("autostart", colorOsStartup), colorOsBattery),
        "oneplus" to listOf(
            Control("autostart", listOf("com.oneplus.security/com.oneplus.security.chainlaunch.view.ChainLaunchAppListActivity") + colorOsStartup),
            colorOsBattery,
        ),
        "vivo" to vivo,
        "iqoo" to vivo,
        "asus" to listOf(
            Control(
                "autostart",
                listOf(
                    "com.asus.mobilemanager/com.asus.mobilemanager.autostart.AutoStartActivity",
                    "com.asus.mobilemanager/com.asus.mobilemanager.entry.FunctionActivity",
                ),
            ),
            Control("power_saver", listOf("com.asus.mobilemanager/com.asus.mobilemanager.powersaver.PowerSaverSettings")),
        ),
        "lenovo" to lenovo,
        "motorola" to lenovo,
        "tecno" to transsion,
        "infinix" to transsion,
        "itel" to transsion,
        "meizu" to listOf(Control("background", listOf("com.meizu.safe/com.meizu.safe.permission.SmartBGActivity"))),
        "letv" to listOf(Control("autostart", listOf("com.letv.android.letvsafe/com.letv.android.letvsafe.AutobootManageActivity"))),
        "htc" to listOf(Control("power", listOf("com.htc.pitroad/com.htc.pitroad.landingpage.activity.LandingPageActivity"))),
        "hmd global" to listOf(
            Control("power_saver", listOf("com.evenwell.powersaving.g3/com.evenwell.powersaving.g3.exception.PowerSaverExceptionActivity")),
        ),
    )

    /** The controls [manufacturer]'s phones may have, in the order Set up offers them. */
    fun controls(manufacturer: String): List<Control> = byMaker[manufacturer.trim().lowercase()].orEmpty()

    /**
     * The controls of [manufacturer]'s this phone has, by name in order, each with the first of
     * its screens that the phone has and lets the app open.
     */
    fun find(packages: PackageManager, manufacturer: String, app: ThisApp): Map<String, Intent> = controls(manufacturer)
        .mapNotNull { control -> control.intents(app).firstOrNull { it.resolveActivityInfo(packages, 0)?.exported == true }?.let { control.name to it } }
        .toMap()

    private fun Control.intents(app: ThisApp): List<Intent> = listOfNotNull(action?.intent()) +
        screens.mapNotNull(ComponentName::unflattenFromString).map { screen ->
            Intent().setComponent(screen).apply {
                if (forTheApp) putExtra(PACKAGE_NAME, app.pkg).putExtra(PACKAGE_LABEL, app.label)
            }
        }

    // As Xiaomi's per-app battery saver takes the app it sets.
    private const val PACKAGE_NAME = "package_name"
    private const val PACKAGE_LABEL = "package_label"

    private fun Action.intent() = extras.entries.fold(Intent(action).setPackage(pkg)) { intent, (key, value) -> intent.putExtra(key, value) }
}
