package com.dspdispatch.relay.update

import com.dspdispatch.relay.dispatch.AppVersion
import com.dspdispatch.relay.dispatch.Dispatch
import java.io.File
import java.security.MessageDigest
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.runTest
import mockwebserver3.MockResponse
import mockwebserver3.MockWebServer
import okhttp3.OkHttpClient
import org.junit.After
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Before
import org.junit.Test

class UpdatesTest {
    private val server = MockWebServer()
    private val own = AppVersion("1.0.4", 4)
    private val newer = AppVersion("1.0.5", 5)
    private val newest = AppVersion("1.0.6", 6)
    private val bytes = "the 1.0.5 apk".toByteArray()
    private val apk = File.createTempFile("update", ".apk")
    private val installer = FakeInstaller()
    private val notice = FakeNotice()
    private var thisApp = true
    private lateinit var site: String

    @Before
    fun start() {
        server.start()
        site = server.url("/").toString()
    }

    @After
    fun stop() {
        server.close()
        apk.delete()
    }

    @Test
    fun offersANewerVersionAndNotifiesOnce() = runTest {
        val updates = updates()
        updates.heard(newer)
        updates.heard(newer)
        assertEquals(Update.Available(newer), updates.update.value)
        assertEquals(listOf(newer), notice.shown)
        updates.heard(own)
        assertEquals(Update.None, updates.update.value)
        assertEquals(1, notice.cancelled)
        updates.heard(null)
        assertEquals(1, notice.cancelled)
    }

    @Test
    fun notifiesOfAVersionOnceThroughARestart() = runTest {
        updates().heard(newer)
        val restarted = updates()
        restarted.heard(newer)
        assertEquals(Update.Available(newer), restarted.update.value)
        assertEquals(listOf(newer), notice.shown)
        restarted.heard(newest)
        assertEquals(listOf(newer, newest), notice.shown)
    }

    @Test
    fun notifiesOnceTheAppMayNotifyAgain() = runTest {
        notice.allowed = false
        val updates = updates()
        updates.heard(newer)
        updates.heard(newer)
        assertEquals(Update.Available(newer), updates.update.value)
        assertEquals(emptyList<AppVersion>(), notice.shown)
        notice.allowed = true
        updates.heard(newer)
        updates.heard(newer)
        assertEquals(listOf(newer), notice.shown)
    }

    @Test
    fun takesAwayTheNotificationOfTheVersionItWasUpdatedTo() = runTest {
        val updates = updates()
        updates.heard(own)
        assertEquals(Update.None, updates.update.value)
        assertEquals(1, notice.cancelled)
        updates.heard(own)
        assertEquals(1, notice.cancelled)
    }

    @Test
    fun downloadsChecksAndInstallsFromThePairedDsp() = runTest {
        val updates = updates()
        updates.heard(newer)
        serve(bytes)
        updates.install(site)
        updates.update.first { it is Update.Downloading }
        assertEquals(Update.Installing(newer), updates.update.first { it !is Update.Downloading })
        assertArrayEquals(bytes, installer.installed.await())
        assertEquals("/api/communications/phone/app", server.takeRequest().url.encodedPath)
        val download = server.takeRequest()
        assertEquals("/api/communications/phone/app/download", download.url.encodedPath)
        assertNull(download.headers["Authorization"])
    }

    @Test
    fun leavesUpdateShowingWhenTheDownloadIsNotTheOneDescribed() = runTest {
        val updates = updates()
        updates.heard(newer)
        serve("another apk!!".toByteArray(), sha256 = sha256(bytes))
        updates.install(site)
        assertEquals(Update.Available(newer, Problem.MISMATCH), updates.tried())
        assertFalse(installer.installed.isCompleted)
        assertFalse(apk.exists())
    }

    @Test
    fun refusesAnApkThatIsNotThisApp() = runTest {
        thisApp = false
        val updates = updates()
        updates.heard(newer)
        serve(bytes)
        updates.install(site)
        assertEquals(Update.Available(newer, Problem.NOT_THIS_APP), updates.tried())
        assertFalse(installer.installed.isCompleted)
    }

    @Test
    fun saysTheDownloadFailedWhenTheDspRefusesIt() = runTest {
        val updates = updates()
        updates.heard(newer)
        server.enqueue(json(404, """{"error":"phones_app_not_found","message":"…"}"""))
        updates.install(site)
        assertEquals(Update.Available(newer, Problem.DOWNLOAD), updates.tried())
    }

    @Test
    fun leavesUpdateShowingWhenAndroidFailsOrThePersonRefuses() = runTest {
        val updates = updates()
        updates.heard(newer)
        serve(bytes)
        installer.outcome = Outcome.FAILED
        updates.install(site)
        assertEquals(Update.Available(newer, Problem.INSTALL), updates.tried())
        serve(bytes)
        installer.outcome = Outcome.REFUSED
        updates.install(site)
        assertEquals(Update.Available(newer), updates.tried())
    }

    @Test
    fun showsAndroidsConfirmationAgainWhileItWaits() = runTest {
        val updates = updates()
        updates.heard(newer)
        serve(bytes)
        installer.confirm = true
        updates.install(site)
        assertEquals(Update.Confirming(newer), updates.update.first { it is Update.Confirming })
        updates.install(site)
        assertEquals(1, installer.asks)
        installer.waiting.complete(Outcome.REFUSED)
        assertEquals(Update.Available(newer), updates.update.first { it is Update.Available })
    }

    @Test
    fun offersTheVersionRolledBackToWhenTheConfirmationIsCancelled() = rollBackWhileConfirming(Outcome.REFUSED)

    @Test
    fun offersTheVersionRolledBackToWithoutTheFailureOfTheOneWithdrawn() = rollBackWhileConfirming(Outcome.FAILED)

    @Test
    fun offersTheVersionTheDetailsNameOverAnEarlierAnswer() = runTest {
        val updates = updates()
        updates.heard(newest)
        serve(bytes, app = newer)
        installer.outcome = Outcome.FAILED
        updates.install(site)
        assertEquals(Update.Available(newer, Problem.INSTALL), updates.tried())
    }

    @Test
    fun installsNothingOnceTheDspNoLongerHandsOutANewerVersion() = runTest {
        val updates = updates()
        updates.heard(newer)
        server.enqueue(json(200, """{"version":"1.0.4","versionCode":4,"size":13,"sha256":"${sha256(bytes)}"}"""))
        updates.install(site)
        assertEquals(Update.None, updates.tried())
        assertEquals(listOf(newer), notice.shown)
        assertEquals(1, notice.cancelled)
        assertEquals(1, server.requestCount)
    }

    /** Installs 1.0.6, and while Android asks to confirm it, the DSP rolls back to 1.0.5; then the try ends in [outcome]. */
    private fun rollBackWhileConfirming(outcome: Outcome) = runTest {
        val updates = updates()
        updates.heard(newest)
        serve(bytes, app = newest)
        installer.confirm = true
        updates.install(site)
        updates.update.first { it is Update.Confirming }
        updates.heard(newer)
        assertEquals(Update.Confirming(newest), updates.update.value)
        installer.waiting.complete(outcome)
        assertEquals(Update.Available(newer), updates.update.first { it is Update.Available })
        updates.heard(newer)
        assertEquals(Update.Available(newer), updates.update.value)
    }

    /** Where a try Install started that didn't install leaves updating. */
    private suspend fun Updates.tried(): Update {
        update.first { it is Update.Downloading }
        return update.first { it is Update.Available || it == Update.None }
    }

    private fun TestScope.updates() = Updates(
        Dispatch(OkHttpClient(), own),
        own,
        apk,
        { thisApp },
        installer,
        notice,
        backgroundScope,
    )

    private fun serve(apk: ByteArray, sha256: String = sha256(apk), app: AppVersion = newer) {
        server.enqueue(json(200, """{"version":"${app.version}","versionCode":${app.versionCode},"size":${apk.size},"sha256":"$sha256"}"""))
        server.enqueue(MockResponse.Builder().code(200).addHeader("Content-Type", "application/vnd.android.package-archive").body(okio.Buffer().write(apk)).build())
    }

    private fun json(code: Int, body: String) =
        MockResponse.Builder().code(code).addHeader("Content-Type", "application/json").body(body).build()

    private fun sha256(bytes: ByteArray) = MessageDigest.getInstance("SHA-256").digest(bytes).joinToString("") { "%02x".format(it) }

    private class FakeInstaller : Installer {
        var outcome = Outcome.INSTALLED
        var confirm = false
        var asks = 0
        val installed = CompletableDeferred<ByteArray>()
        val waiting = CompletableDeferred<Outcome>()

        override suspend fun install(apk: File, confirming: () -> Unit): Outcome {
            installed.complete(apk.readBytes())
            if (!confirm) return outcome
            confirming()
            return waiting.await()
        }

        override fun askAgain() {
            asks++
        }
    }

    /** The phone's notifications, and the app's storage, which outlive each [Updates] made. */
    private class FakeNotice : Notice {
        var allowed = true
        val shown = mutableListOf<AppVersion>()
        var cancelled = 0
        override var notified = 0

        override fun show(app: AppVersion): Boolean {
            if (allowed) shown += app
            return allowed
        }

        override fun cancel() {
            cancelled++
        }
    }
}
