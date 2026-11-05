package com.dspdispatch.relay.update

import com.dspdispatch.relay.connection.Unexpected
import com.dspdispatch.relay.dispatch.AppDetails
import com.dspdispatch.relay.dispatch.AppVersion
import com.dspdispatch.relay.dispatch.Dispatch
import com.dspdispatch.relay.dispatch.Refusal
import java.io.File
import java.io.IOException
import kotlin.coroutines.CoroutineContext
import kotlin.coroutines.cancellation.CancellationException
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

    /** Asks the person to confirm installing [app], replacing any notification showing, where the app may notify; whether it did. */
    fun show(app: AppVersion): Boolean

    /** Takes away the notification showing, if there is one. */
    fun cancel()
}

/**
 * A version whose last try didn't install, after [tries] in a row, and when to try it again: never,
 * for an APK that isn't this app.
 */
data class Hold(val versionCode: Int, val until: Long?, val tries: Int = 1)

/** The update held back from trying again (`StoredHold`): kept across restarts. */
interface Holds {
    var hold: Hold?
}

/**
 * Updating the app, at [own] version, from its DSP, on its own: each newer version Dispatch tells
 * of is downloaded to [apk], checked against the DSP's details and that it's this app, signed as it
 * is, and, once no text is mid-send, with sending held back ([hold], until [release]), installed by
 * Android with no one confirming, which replaces the app and restarts it. Where Android asks the
 * person to confirm it anyway, sending goes on and its [notice] asks them, until it's installed or
 * the DSP no longer hands it out. A try that fails is held ([holds]) longer each time at its
 * version ([RETRIES_MS]), its download kept for the next once whole; one at an APK that isn't this
 * app until the DSP hands out another. An error nothing expected is told to [unexpected], and held
 * as one Android failed. Its calls come on the main thread; a try runs in [scope].
 */
class Updates(
    private val dispatch: Dispatch,
    private val own: AppVersion,
    private val apk: File,
    private val check: ApkCheck,
    private val installer: Installer,
    private val notice: Notice,
    private val holds: Holds,
    private val hold: suspend () -> Unit,
    private val release: () -> Unit,
    private val scope: CoroutineScope,
    private val unexpected: Unexpected,
    private val io: CoroutineContext = Dispatchers.IO,
    private val now: () -> Long = System::currentTimeMillis,
) {
    private val state = MutableStateFlow<Update>(Update.None)
    val update: StateFlow<Update> = state.asStateFlow()

    /** The newer version the DSP last told of, in an outbox answer or a version's details. */
    private var offered: AppVersion? = null
    private var installing: Job? = null

    /** The version the try in progress is at. */
    private val trying get() = state.value.app

    // An APK left by an update that installed, which ended the process before it was deleted: any
    // but one kept for the next try at a newer version.
    private val cleared = scope.launch(io) {
        if (holds.hold?.let { it.until != null && it.versionCode > own.versionCode } != true) apk.delete()
    }

    /**
     * Dispatch's latest version, [app] as the outbox of [site], the DSP the phone is paired with,
     * answered it: tried at once while newer than this app and Android lets the app install, unless
     * its last try is held. A confirmation still waiting on a version the DSP no longer hands out is
     * dropped for the one it does; one whose notification couldn't be posted is asked again.
     */
    fun heard(site: String, app: AppVersion?) {
        val newer = app?.takeIf { it.versionCode > own.versionCode }
        val changed = newer != offered
        offer(newer)
        if (installing?.isActive == true) {
            val confirming = state.value as? Update.Confirming ?: return
            if (!changed) return ask(confirming.app)
            installing?.cancel()
        }
        if (changed) state.value = newer?.let { Update.Available(it) } ?: Update.None
        if (newer != null && installer.allowed() && due(newer)) start(site)
    }

    /**
     * The person asked to update, from its notification: shows Android's confirmation again while
     * one waits, or tries the version offered from [site] at once, held or not, unless it's held for
     * good.
     */
    fun install(site: String) {
        if (state.value is Update.Confirming) return installer.askAgain()
        val app = offered ?: return
        val held = holds.hold?.let { it.versionCode == app.versionCode && it.until == null } == true
        if (installing?.isActive == true || held) return
        start(site)
    }

    private fun due(app: AppVersion): Boolean {
        val last = holds.hold ?: return true
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

    /**
     * Tries the version offered from [site], after any try still ending, a confirmation dropped. Its
     * APK is kept for the next try at its version, through any failure but its own: let go of once
     * it's not whole, not this app, installed, or of a version the DSP no longer hands out.
     */
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
            } catch (cancelled: CancellationException) {
                // Dropped for a version the DSP hands out now.
                letGo()
                throw cancelled
            } catch (error: Exception) {
                // Never ending the app, to fail the same way as it starts again.
                unexpected.happened("Updating the app", error)
                failed(trying, Problem.INSTALL)
            }
        }
    }

    /**
     * Downloads, checks and installs the version the DSP hands out now, from the APK kept by a try
     * before if it's that version's; where that leaves updating.
     */
    private suspend fun attempt(site: String): Update {
        cleared.join()
        val details = dispatch.app(site)
        val app = details.app
        offer(app.takeIf { it.versionCode > own.versionCode })
        if (offered == null) return Update.None.also { letGo() }
        // From here the try is at the version the details name, which may not be the one heard of.
        state.value = Update.Downloading(app)
        if (!withContext(io) { saved(apk, details) }) download(site, details)
        if (!withContext(io) { check.isThisApp(apk) }) return failed(app, Problem.NOT_THIS_APP).also { letGo() }
        // Installing ends the process: never while a text is mid-send, nor while one starts.
        var installed = false
        val outcome = try {
            hold()
            state.value = Update.Installing(app)
            installer.install(apk) {
                release()
                state.value = Update.Confirming(app)
                // Posted afresh for each confirmation: one posted before a restart is gone.
                notice.showing = app.versionCode.takeIf { notice.show(app) }
            }.also { installed = it == Outcome.INSTALLED }
        } finally {
            if (!installed) release()
        }
        if (installed) letGo()
        return when (outcome) {
            // Android replaces the app, ending this process; the new one starts the relay again.
            Outcome.INSTALLED -> Update.Installing(app)
            Outcome.REFUSED -> failed(app, null)
            Outcome.FAILED -> failed(app, Problem.INSTALL)
        }
    }

    /** Downloads the APK [details] describes from [site]: none of it kept, unless it's whole. */
    private suspend fun download(site: String, details: AppDetails) {
        try {
            withContext(io) { dispatch.apk(site) { input -> save(input, apk, details) } }
        } catch (failed: Throwable) {
            letGo()
            throw failed
        }
    }

    /** Lets go of the APK: not whole, not this app, installed, or of no version to try. */
    private suspend fun letGo() {
        withContext(NonCancellable + io) { apk.delete() }
    }

    /** Asks the person to confirm installing [app], unless its notification was posted already. */
    private fun ask(app: AppVersion) {
        if (notice.showing != app.versionCode && notice.show(app)) notice.showing = app.versionCode
    }

    /**
     * Where a try at [app] that didn't install, for [problem] or refused by the person, leaves
     * updating: held, then offering the version the DSP last told of, older or newer, with the
     * try's [problem] only if it's [app]; or nothing, if none.
     */
    private fun failed(app: AppVersion?, problem: Problem?): Update {
        if (app != null) holds.hold = held(app, problem)
        val now = offered ?: return Update.None
        return Update.Available(now, problem.takeIf { now == app })
    }

    /**
     * The hold after another try at [app] failed for [problem]: for good, for an APK that isn't
     * this app, as another download of the version wouldn't be either; else for longer than the try
     * before at it, a download that wasn't the one described too, as a network may spoil one.
     */
    private fun held(app: AppVersion, problem: Problem?): Hold {
        val tries = (holds.hold?.takeIf { it.versionCode == app.versionCode }?.tries ?: 0) + 1
        if (problem == Problem.NOT_THIS_APP) return Hold(app.versionCode, null, tries)
        return Hold(app.versionCode, now() + RETRIES_MS[minOf(tries, RETRIES_MS.size) - 1], tries)
    }

    companion object {
        /** How long each try in a row at a version that didn't install waits before the next: the last, each after. */
        val RETRIES_MS = listOf(1L, 3L, 12L, 24L).map { it * 60 * 60 * 1000 }
    }
}
