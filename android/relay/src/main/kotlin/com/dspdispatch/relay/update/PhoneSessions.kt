package com.dspdispatch.relay.update

import android.content.Context
import android.content.pm.PackageInstaller
import android.content.pm.PackageManager
import android.os.Build
import java.io.File

/** The phone's `PackageInstaller`, asking on Android 12+ to install with no one confirming. */
class PhoneSessions(private val context: Context) : Sessions {
    private val installer = context.packageManager.packageInstaller

    override fun create(apk: File): Int {
        // A session of an earlier process, which ended before Android answered it.
        installer.mySessions.forEach { abandon(it.sessionId) }
        return installer.createSession(params(apk))
    }

    override fun commit(id: Int, apk: File) = installer.openSession(id).use { session ->
        session.openWrite(APK_NAME, 0, apk.length()).use { output ->
            apk.inputStream().use { it.copyTo(output) }
            session.fsync(output)
        }
        session.commit(InstallReceiver.statusOf(context, id).intentSender)
    }

    override fun abandon(id: Int) {
        try {
            installer.abandonSession(id)
        } catch (_: SecurityException) {
            // Already finished, so gone.
        }
    }

    private fun params(apk: File) = PackageInstaller.SessionParams(PackageInstaller.SessionParams.MODE_FULL_INSTALL).apply {
        // Android refuses the session for any other app's APK, as PackageCheck does before it.
        setAppPackageName(context.packageName)
        setSize(apk.length())
        setInstallReason(PackageManager.INSTALL_REASON_USER)
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
            setRequireUserAction(PackageInstaller.SessionParams.USER_ACTION_NOT_REQUIRED)
        }
    }

    private companion object {
        const val APK_NAME = "update.apk"
    }
}
