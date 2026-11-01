package com.dspdispatch.relay.update

import android.content.Context
import android.content.Intent
import android.content.pm.PackageInstaller
import android.content.pm.PackageManager
import android.os.Build
import com.dspdispatch.relay.update.Outcome.FAILED
import com.dspdispatch.relay.update.Outcome.INSTALLED
import com.dspdispatch.relay.update.Outcome.REFUSED
import java.io.File
import java.io.IOException
import java.util.concurrent.ConcurrentHashMap
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.withContext

/**
 * Installs an update of this app with Android's `PackageInstaller`, in a session of its own. On
 * Android 12+ it asks to install with no one confirming, which Android allows an app updating
 * itself; where Android asks anyway, as on older phones or the first update after installing from
 * a file, its confirmation waits in [confirm] for the app's screen to show.
 */
class SessionInstaller(private val context: Context) : Installer {
    private val sessions = context.packageManager.packageInstaller
    private val waiting = ConcurrentHashMap<Int, Waiting>()

    /** Android's confirmation of the session waiting on it, the last it asked for. */
    @Volatile
    private var pending: Intent? = null
    private val asking = MutableStateFlow<Intent?>(null)

    /** A confirmation for the app's screen to show, once, then say [asked]. */
    val confirm: StateFlow<Intent?> = asking.asStateFlow()

    private class Waiting(val outcome: CompletableDeferred<Outcome>, val confirming: () -> Unit)

    override suspend fun install(apk: File, confirming: () -> Unit): Outcome {
        val id = try {
            withContext(Dispatchers.IO) { sessions.createSession(params(apk)) }
        } catch (_: IOException) {
            return FAILED
        }
        val outcome = CompletableDeferred<Outcome>()
        waiting[id] = Waiting(outcome, confirming)
        try {
            withContext(Dispatchers.IO) { commit(id, apk) }
            return outcome.await()
        } catch (_: IOException) {
            return FAILED
        } catch (_: SecurityException) {
            // The session was abandoned meanwhile, or the app may no longer install.
            return FAILED
        } finally {
            waiting.remove(id)
            if (!outcome.isCompleted) withContext(NonCancellable) { abandon(id) }
        }
    }

    override fun askAgain() {
        asking.value = pending
    }

    /** The app's screen showed the confirmation [confirm] held. */
    fun asked() {
        asking.value = null
    }

    /**
     * What Android said of session [id], with its [status] and, while it asks the person, the
     * [confirmation] to show; from [InstallReceiver].
     */
    fun said(id: Int, status: Int, confirmation: Intent?) {
        val session = waiting[id] ?: return
        if (status == PackageInstaller.STATUS_PENDING_USER_ACTION) {
            pending = confirmation
            asking.value = confirmation
            session.confirming()
            return
        }
        pending = null
        asking.value = null
        session.outcome.complete(
            when (status) {
                PackageInstaller.STATUS_SUCCESS -> INSTALLED
                PackageInstaller.STATUS_FAILURE_ABORTED -> REFUSED
                else -> FAILED
            },
        )
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

    private fun commit(id: Int, apk: File) = sessions.openSession(id).use { session ->
        session.openWrite(APK_NAME, 0, apk.length()).use { output ->
            apk.inputStream().use { it.copyTo(output) }
            session.fsync(output)
        }
        session.commit(InstallReceiver.statusOf(context, id).intentSender)
    }

    private fun abandon(id: Int) {
        try {
            sessions.abandonSession(id)
        } catch (_: SecurityException) {
            // Already finished, so gone.
        }
    }

    private companion object {
        const val APK_NAME = "update.apk"
    }
}
