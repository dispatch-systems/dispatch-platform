package com.dspdispatch.relay.setup

import android.content.ComponentName
import android.content.Intent
import android.content.pm.PackageManager

/**
 * The screens phone makers add to keep apps from running in the background, beyond Android's own
 * battery optimization, by maker, best first: as dontkillmyapp.com and the apps that open them
 * have documented them. Each package is named in the manifest's `<queries>`, which Android 11+
 * needs to see it.
 */
object MakerScreens {
    private val xiaomi = listOf(
        "com.miui.securitycenter/com.miui.permcenter.autostart.AutoStartManagementActivity",
        "com.miui.powerkeeper/com.miui.powerkeeper.ui.HiddenAppsConfigActivity",
    )
    private val huawei = listOf(
        "com.huawei.systemmanager/com.huawei.systemmanager.startupmgr.ui.StartupNormalAppListActivity",
        "com.huawei.systemmanager/com.huawei.systemmanager.optimize.process.ProtectActivity",
        "com.huawei.systemmanager/com.huawei.systemmanager.appcontrol.activity.StartupAppControlActivity",
    )
    private val colorOs = listOf(
        "com.coloros.safecenter/com.coloros.safecenter.permission.startup.StartupAppListActivity",
        "com.coloros.safecenter/com.coloros.safecenter.startupapp.StartupAppListActivity",
        "com.oppo.safe/com.oppo.safe.permission.startup.StartupAppListActivity",
        "com.coloros.oppoguardelf/com.coloros.powermanager.fuelgaue.PowerUsageModelActivity",
    )
    private val vivo = listOf(
        "com.vivo.permissionmanager/com.vivo.permissionmanager.activity.BgStartUpManagerActivity",
        "com.iqoo.secure/com.iqoo.secure.ui.phoneoptimize.AddWhiteListActivity",
        "com.iqoo.secure/com.iqoo.secure.ui.phoneoptimize.BgStartUpManager",
    )
    private val lenovo = listOf("com.lenovo.security/com.lenovo.security.purebackground.PureBackgroundActivity")
    private val transsion = listOf("com.transsion.phonemaster/com.cyin.himgr.autostart.AutoStartActivity")

    /** By `Build.MANUFACTURER`, lowercase. */
    private val byMaker: Map<String, List<String>> = mapOf(
        "samsung" to listOf(
            "com.samsung.android.lool/com.samsung.android.sm.battery.ui.BatteryActivity",
            "com.samsung.android.lool/com.samsung.android.sm.ui.battery.BatteryActivity",
            "com.samsung.android.sm/com.samsung.android.sm.ui.battery.BatteryActivity",
        ),
        "xiaomi" to xiaomi,
        "redmi" to xiaomi,
        "poco" to xiaomi,
        "huawei" to huawei,
        "honor" to listOf(
            "com.hihonor.systemmanager/com.hihonor.systemmanager.startupmgr.ui.StartupNormalAppListActivity",
        ) + huawei,
        "oppo" to colorOs,
        "realme" to colorOs,
        "oneplus" to listOf("com.oneplus.security/com.oneplus.security.chainlaunch.view.ChainLaunchAppListActivity") + colorOs,
        "vivo" to vivo,
        "iqoo" to vivo,
        "asus" to listOf(
            "com.asus.mobilemanager/com.asus.mobilemanager.autostart.AutoStartActivity",
            "com.asus.mobilemanager/com.asus.mobilemanager.powersaver.PowerSaverSettings",
            "com.asus.mobilemanager/com.asus.mobilemanager.entry.FunctionActivity",
        ),
        "lenovo" to lenovo,
        "motorola" to lenovo,
        "tecno" to transsion,
        "infinix" to transsion,
        "itel" to transsion,
        "meizu" to listOf("com.meizu.safe/com.meizu.safe.permission.SmartBGActivity"),
        "letv" to listOf("com.letv.android.letvsafe/com.letv.android.letvsafe.AutobootManageActivity"),
        "htc" to listOf("com.htc.pitroad/com.htc.pitroad.landingpage.activity.LandingPageActivity"),
        "hmd global" to listOf("com.evenwell.powersaving.g3/com.evenwell.powersaving.g3.exception.PowerSaverExceptionActivity"),
    )

    /** The screens [manufacturer]'s phones may have, best first, as `package/class`. */
    fun candidates(manufacturer: String): List<String> = byMaker[manufacturer.trim().lowercase()].orEmpty()

    /** The first of [manufacturer]'s screens this phone has and lets the app open, if any. */
    fun find(packages: PackageManager, manufacturer: String): Intent? = candidates(manufacturer)
        .mapNotNull(ComponentName::unflattenFromString)
        .map { Intent().setComponent(it) }
        .firstOrNull { it.resolveActivityInfo(packages, 0)?.exported == true }
}
