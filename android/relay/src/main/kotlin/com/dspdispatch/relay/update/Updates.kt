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
import kotlinx.coroutines.cancelAndJoin
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
    /** Whether Android lets the app install its updates, as Set up's Install updates step has it. */
    fun allowed(): Boolean

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

/** The notification asking the person to confirm an update Android won't install without them (`UpdateNotice`). */
interface Notice {
    /** The `versionCode` of the version last asked about, while its notification may be showing; kept across restarts. */
    var showing: Int?

    /** Asks the person to confirm installing [app], replacing any notification showing, where the app may notify. */
    fun show(app: AppVersion)

    /** Takes away the notification showing, if there is one. */
    fun cancel()
}

/**
 * Updating the app, at [own] version, from its DSP, on its own: each newer version Dispatch tells
 * of is downloaded to [apk], checked against the DSP's details and that it's this app, signed as it
 * is, and, once no text is mid-send ([quiet]), installed by Android with no one confirming, which
 * replaces the app and restarts it. Where Android asks the person to confirm it anyway, its
 * [notice] asks them, until it's installed or the DSP no longer hands it out. A try that fails is
 * tried again after [RETRY_MS]; one at an APK that isn't this app only once the DSP hands out
 * another. Its calls come on the main thread; a try runs in [scope].
 */
class Updates(
    private val dispatch: Dispatch,
    private val own: AppVersion,
    private val apk: File,
    private val check: ApkCheck,
    private val installer: Installer,
    private val notice: Notice,
    private val quiet: suspend () -> Unit,
    private val scope: CoroutineScope,
    private val io: CoroutineContext = Dispatchers.IO,
    private val now: () -> Long = System::currentTimeMillis,
) {
    private val state = MutableStateFlow<Update>(Update.None)
    val update: StateFlow<Update> = state.asStateFlow()

    /** The newer version the DSP last told of, in an outbox answer or a version's details. */
    private var offered: AppVersion? = null
    private var installing: Job? = null

    /** The version whose last try didn't install, and when to try it again: never, for one that isn't this app. */
    private var held: Held? = null

    private data class Held(val versionCode: Int, val until: Long?)

    /** The version the try in progress is at. */
    private val trying get() = state.value.app

    // An APK left by an update that installed, which ended the process before it was deleted.
    private val cleared = scope.launch(io) { apk.delete() }

    /**
     * Dispatch's latest version, [app] as the outbox of [site], the DSP the phone is paired with,
     * answered it: tried at once while newer than this app and Android lets the app install, unless
     * its last try is held. A confirmation still waiting on a version the DSP no longer hands out is
     * dropped for the one it does.
     */
    fun heard(site: String, app: AppVersion?) {
        val newer = app?.takeIf { it.versionCode > own.versionCode }
        val changed = newer != offered
        offer(newer)
        if (installing?.isActive == true) {
            if (!changed || state.value !is Update.Confirming) return
            installing?.cancel()
        }
        if (changed) state.value = newer?.let { Update.Available(it) } ?: Update.None
        if (newer != null && installer.allowed() && due(newer)) start(site)
    }

    /**
     * The person asked to update, from its notification: shows Android's confirmation again while
     * one waits, or tries the version offered from [site] at once, held or not, unless it isn't this
     * app.
     */
    fun install(site: String) {
        if (state.value is Update.Confirming) return installer.askAgain()
        val app = offered ?: return
        if (installing?.isActive == true || held == Held(app.versionCode, null)) return
        start(site)
    }

    private fun due(app: AppVersion): Boolean {
        val last = held ?: return true
        return last.versionCode != app.versionCode || (last.until != null && now() >= last.until)
    }

    /**
     * Offers [newer], or nothing, taking away the notification of any other version: one withdrawn,
     * or the one this app was updated to.
     */
    private fun offer(newer: AppVersion?) {
        offered = newer
        val showing = notice.showing ?: return
        if (showing == newer?.versionCode) return
        notice.cancel()
        notice.showing = null
    }

    /** Tries the version offered from [site], after any try still ending, a confirmation dropped. */
    private fun start(site: String) {
        val previous = installing
        installing = scope.launch {
            previous?.cancelAndJoin()
            offered?.let { state.value = Update.Downloading(it) }
            state.value = try {
                attempt(site)
            } catch (_: Refusal) {
                failed(trying, Problem.DOWNLOAD)
            } catch (_: IOException) {
                failed(trying, Problem.DOWNLOAD)
            } catch (_: SerializationException) {
                failed(trying, Problem.DOWNLOAD)
            } catch (_: Mismatch) {
                failed(trying, Problem.MISMATCH)
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
        offer(app.takeIf { it.versionCode > own.versionCode })
        if (offered == null) return Update.None
        // From here the try is at the version the details name, which may not be the one heard of.
        state.value = Update.Downloading(app)
        withContext(io) { dispatch.apk(site) { input -> save(input, apk, details) } }
        if (!withContext(io) { check.isThisApp(apk) }) return failed(app, Problem.NOT_THIS_APP)
        // Installing ends the process: never while a text is mid-send.
        quiet()
        state.value = Update.Installing(app)
        val outcome = installer.install(apk) {
            state.value = Update.Confirming(app)
            notice.show(app)
            notice.showing = app.versionCode
        }
        return when (outcome) {
            // Android replaces the app, ending this process; the new one starts the relay again.
            Outcome.INSTALLED -> Update.Installing(app)
            Outcome.REFUSED -> failed(app, null)
            Outcome.FAILED -> failed(app, Problem.INSTALL)
        }
    }

    /**
     * Where a try at [app] that didn't install, for [problem] or refused by the person, leaves
     * updating: held, then offering the version the DSP last told of, older or newer, with the
     * try's [problem] only if it's [app]; or nothing, if none.
     */
    private fun failed(app: AppVersion?, problem: Problem?): Update {
        if (app != null) held = Held(app.versionCode, if (problem == Problem.NOT_THIS_APP) null else now() + RETRY_MS)
        val now = offered ?: return Update.None
        return Update.Available(now, problem.takeIf { now == app })
    }

    companion object {
        /** How long a try that didn't install waits before the next. */
        const val RETRY_MS = 60 * 60 * 1000L
    }
}
