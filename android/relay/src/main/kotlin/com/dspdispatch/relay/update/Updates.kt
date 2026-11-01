package com.dspdispatch.relay.update

import com.dspdispatch.relay.dispatch.AppVersion
import com.dspdispatch.relay.dispatch.Dispatch
import com.dspdispatch.relay.dispatch.Refusal
import java.io.File
import java.io.IOException
import kotlin.coroutines.CoroutineContext
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import kotlinx.serialization.SerializationException

/** What came of installing an APK. */
enum class Outcome { INSTALLED, REFUSED, FAILED }

/** Installs an APK as an update of this app (`SessionInstaller`). */
interface Installer {
    /** Installs [apk], calling [confirming] once Android asks the person to confirm it; what came of it. */
    suspend fun install(apk: File, confirming: () -> Unit): Outcome

    /** Shows Android's confirmation again, while one waits. */
    fun askAgain()
}

/** Whether an APK is this app, signed with this app's key (`PackageCheck`). */
fun interface ApkCheck {
    /** Whether [apk] is this app, signed as it is. Blocks: call it off the main thread. */
    fun isThisApp(apk: File): Boolean
}

/** The notification that a newer version is offered (`UpdateNotice`). */
interface Notice {
    fun show(app: AppVersion)

    fun cancel()
}

/**
 * Updating the app, at [own] version, from its DSP: offers each newer version Dispatch tells of,
 * and on Install downloads it to [apk], checks it against the DSP's details and that it's this
 * app, signed as it is, then has Android install it, which replaces the app and restarts it.
 * Its calls come on the main thread; a try outlives the screen that started it, in [scope].
 */
class Updates(
    private val dispatch: Dispatch,
    private val own: AppVersion,
    private val apk: File,
    private val check: ApkCheck,
    private val installer: Installer,
    private val notice: Notice,
    private val scope: CoroutineScope,
    private val io: CoroutineContext = Dispatchers.IO,
) {
    private val state = MutableStateFlow<Update>(Update.None)
    val update: StateFlow<Update> = state.asStateFlow()

    /** The newer version Dispatch last told of, notified once. */
    private var offered: AppVersion? = null
    private var installing: Job? = null

    // An APK left by an update that installed, which ended the process before it was deleted.
    private val cleared = scope.launch(io) { apk.delete() }

    /** Dispatch's latest version, [app] as the outbox answered it: offered while newer than this app. */
    fun heard(app: AppVersion?) {
        val newer = app?.takeIf { it.versionCode > own.versionCode }
        if (newer == offered) return
        offered = newer
        if (newer == null) notice.cancel() else notice.show(newer)
        if (installing?.isActive != true) state.value = newer?.let { Update.Available(it) } ?: Update.None
    }

    /**
     * Installs the version offered from [site], the DSP the phone is paired with; or, while Android
     * asks to confirm one, shows its confirmation again.
     */
    fun install(site: String) {
        val now = state.value
        if (now is Update.Confirming) return installer.askAgain()
        if (now !is Update.Available || installing?.isActive == true) return
        installing = scope.launch {
            state.value = Update.Downloading(now.app, 0f)
            state.value = try {
                attempt(site)
            } catch (_: Refusal) {
                after(now.app, Problem.DOWNLOAD)
            } catch (_: IOException) {
                after(now.app, Problem.DOWNLOAD)
            } catch (_: SerializationException) {
                after(now.app, Problem.DOWNLOAD)
            } catch (_: Mismatch) {
                after(now.app, Problem.MISMATCH)
            } finally {
                withContext(NonCancellable + io) { apk.delete() }
            }
        }
    }

    /** Downloads, checks and installs the version the DSP hands out now; where that leaves updating. */
    private suspend fun attempt(site: String): Update {
        cleared.join()
        val details = dispatch.app(site)
        val app = details.app
        if (app.versionCode <= own.versionCode) {
            offered = null
            notice.cancel()
            return Update.None
        }
        var percent = 0
        withContext(io) {
            dispatch.apk(site) { input ->
                save(input, apk, details) { saved ->
                    val now = (saved * PERCENT / details.size).toInt()
                    if (now != percent) {
                        percent = now
                        state.value = Update.Downloading(app, now / PERCENT.toFloat())
                    }
                }
            }
        }
        if (!withContext(io) { check.isThisApp(apk) }) return after(app, Problem.NOT_THIS_APP)
        state.value = Update.Installing(app)
        return when (installer.install(apk) { state.value = Update.Confirming(app) }) {
            // Android replaces the app, ending this process; the new one starts the relay again.
            Outcome.INSTALLED -> Update.Installing(app)
            Outcome.REFUSED -> after(app, null)
            Outcome.FAILED -> after(app, Problem.INSTALL)
        }
    }

    /**
     * Where a try at [app] that didn't install leaves updating: offering it again, with its
     * [problem], unless Dispatch has since told of a newer version, or of none.
     */
    private fun after(app: AppVersion, problem: Problem?): Update {
        val newest = offered ?: return Update.None
        return if (newest.versionCode > app.versionCode) Update.Available(newest) else Update.Available(app, problem)
    }

    private companion object {
        const val PERCENT = 100L
    }
}
