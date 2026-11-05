package com.dspdispatch.relay.update

import com.dspdispatch.relay.dispatch.AppVersion
import com.dspdispatch.relay.dispatch.Dispatch
import java.io.File
import java.security.MessageDigest
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.runTest
import mockwebserver3.Dispatcher
import mockwebserver3.MockResponse
import mockwebserver3.MockWebServer
import mockwebserver3.RecordedRequest
import okhttp3.OkHttpClient
import org.junit.After
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
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
    private val holds = FakeHolds()
    private var quiet = CompletableDeferred(Unit)
    private val quietAsked = CompletableDeferred<Unit>()
    private var released = 0
    private var thisApp = true
    private var clock = 0L
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
    fun installsANewerVersionOnItsOwnFromThePairedDsp() = runTest {
        val updates = updates()
        serve(bytes)
        updates.heard(site, newer)
        assertArrayEquals(bytes, installer.installed.await())
        assertEquals(Update.Installing(newer), updates.update.first { it is Update.Installing })
        assertEquals("/api/communications/phone/app", server.takeRequest().url.encodedPath)
        val download = server.takeRequest()
        assertEquals("/api/communications/phone/app/download", download.url.encodedPath)
        assertNull(download.headers["Authorization"])
        assertEquals(emptyList<AppVersion>(), notice.shown)
    }

    @Test
    fun downloadsNothingWhileAndroidDoesNotLetTheAppInstall() = runTest {
        installer.allowed = false
        val updates = updates()
        updates.heard(site, newer)
        assertEquals(Update.Available(newer), updates.update.value)
        assertEquals(0, server.requestCount)
        installer.allowed = true
        serve(bytes)
        updates.heard(site, newer)
        assertArrayEquals(bytes, installer.installed.await())
    }

    @Test
    fun installsOnlyOnceNoTextIsMidSendHoldingSendingBackUntilAndroidAsks() = runTest {
        quiet = CompletableDeferred()
        installer.confirm = true
        val updates = updates()
        serve(bytes)
        updates.heard(site, newer)
        quietAsked.await()
        assertFalse(installer.installed.isCompleted)
        quiet.complete(Unit)
        assertArrayEquals(bytes, installer.installed.await())
        updates.update.first { it is Update.Confirming }
        assertEquals(1, released)
    }

    @Test
    fun asksAfreshForEachConfirmationThroughARestart() = runTest {
        installer.confirm = true
        notice.showing = newer.versionCode
        val updates = updates()
        serve(bytes)
        updates.heard(site, newer)
        updates.update.first { it is Update.Confirming }
        assertEquals(listOf(newer), notice.shown)
    }

    @Test
    fun asksAgainOnceTheAppMayNotify() = runTest {
        installer.confirm = true
        notice.allowed = false
        val updates = updates()
        serve(bytes)
        updates.heard(site, newer)
        updates.update.first { it is Update.Confirming }
        assertNull(notice.showing)
        updates.heard(site, newer)
        assertEquals(emptyList<AppVersion>(), notice.shown)
        notice.allowed = true
        updates.heard(site, newer)
        updates.heard(site, newer)
        assertEquals(listOf(newer), notice.shown)
    }

    @Test
    fun asksThePersonOnlyOnceAndroidWillNotInstallWithoutThem() = runTest {
        installer.confirm = true
        val updates = updates()
        serve(bytes)
        updates.heard(site, newer)
        assertEquals(Update.Confirming(newer), updates.update.first { it is Update.Confirming })
        assertEquals(listOf(newer), notice.shown)
        assertEquals(newer.versionCode, notice.showing)
        updates.heard(site, newer)
        updates.install(site)
        assertEquals(1, installer.asks)
        assertEquals(1, installer.installs.size)
        assertEquals(listOf(newer), notice.shown)
    }

    @Test
    fun dropsAWaitingConfirmationForTheVersionTheDspHandsOutNow() = runTest {
        installer.confirm = true
        val updates = updates()
        serve(bytes, app = newest)
        updates.heard(site, newest)
        updates.update.first { it is Update.Confirming }
        serve(bytes, app = newer)
        updates.heard(site, newer)
        assertEquals(Update.Confirming(newer), updates.update.first { it == Update.Confirming(newer) })
        assertTrue(installer.dropped.isCompleted)
        assertEquals(listOf(newest, newer), notice.shown)
        assertEquals(1, notice.cancelled)
    }

    @Test
    fun takesAwayTheNotificationOfAVersionWithdrawnOrUpdatedTo() = runTest {
        installer.confirm = true
        val updates = updates()
        serve(bytes)
        updates.heard(site, newer)
        updates.update.first { it is Update.Confirming }
        updates.heard(site, own)
        assertEquals(Update.None, updates.update.value)
        installer.dropped.await()
        assertNull(notice.visible)
        // After the update, the version it asked about is the app's own.
        notice.show(own)
        notice.showing = own.versionCode
        updates().heard(site, own)
        assertNull(notice.visible)
        assertNull(notice.showing)
    }

    @Test
    fun triesAnUpdateThatDidNotInstallAgainLaterEachTimeFromTheApkKept() = runTest {
        installer.outcome = Outcome.FAILED
        server.dispatcher = object : Dispatcher() {
            override fun dispatch(request: RecordedRequest) =
                if (request.url.encodedPath.endsWith("/download")) download(bytes) else details(bytes)
        }
        var updates = updates()
        val waits = Updates.RETRIES_MS + Updates.RETRIES_MS.last()
        for ((tried, wait) in waits.withIndex()) {
            updates.heard(site, newer)
            installer.tries.receive()
            assertEquals(Update.Available(newer, Problem.INSTALL), updates.update.first { it is Update.Available })
            assertEquals(Hold(newer.versionCode, clock + wait, tried + 1), holds.hold)
            clock += wait - 1
            updates.heard(site, newer)
            // Through a restart too.
            updates = updates()
            updates.heard(site, newer)
            clock += 1
        }
        assertEquals(List(waits.size) { bytes.toList() }, installer.installs.map(ByteArray::toList))
        assertEquals("one download, then the details for each try", 1 + waits.size, server.requestCount)
        assertEquals(waits.size, released)
    }

    @Test
    fun keepsTheApkThroughATryWhoseDetailsCouldNotBeHad() = runTest {
        installer.outcome = Outcome.FAILED
        var reachable = true
        val downloads = mutableListOf<String>()
        server.dispatcher = object : Dispatcher() {
            override fun dispatch(request: RecordedRequest) = when {
                !reachable -> json(503, """{"error":"platform_busy"}""")
                request.url.encodedPath.endsWith("/download") -> download(bytes).also { downloads += "apk" }
                else -> details(bytes)
            }
        }
        val updates = updates()
        updates.heard(site, newer)
        installer.tries.receive()
        updates.update.first { it == Update.Available(newer, Problem.INSTALL) }
        reachable = false
        clock += Updates.RETRIES_MS[0]
        updates.heard(site, newer)
        updates.update.first { it == Update.Available(newer, Problem.DOWNLOAD) }
        reachable = true
        installer.outcome = Outcome.INSTALLED
        clock += Updates.RETRIES_MS[1]
        updates.heard(site, newer)
        installer.tries.receive()
        assertEquals(listOf(bytes.toList(), bytes.toList()), installer.installs.map(ByteArray::toList))
        assertEquals(listOf("apk"), downloads)
    }

    @Test
    fun triesAgainAtOnceWhenThePersonAsksAfterRefusing() = runTest {
        installer.outcome = Outcome.REFUSED
        val updates = updates()
        serve(bytes)
        updates.heard(site, newer)
        installer.installed.await()
        updates.update.first { it == Update.Available(newer) }
        updates.heard(site, newer)
        assertEquals(1, installer.installs.size)
        installer.outcome = Outcome.INSTALLED
        serve(bytes)
        updates.install(site)
        assertEquals(Update.Installing(newer), updates.update.first { it is Update.Installing })
        assertEquals(2, installer.installs.size)
    }

    @Test
    fun triesAnApkThatIsNotThisAppAgainOnlyForAnotherVersion() = runTest {
        thisApp = false
        val updates = updates()
        serve(bytes)
        updates.heard(site, newer)
        assertEquals(Update.Available(newer, Problem.NOT_THIS_APP), updates.update.first { it is Update.Available && it.problem != null })
        assertTrue(installer.installs.isEmpty())
        clock += 2 * Updates.RETRIES_MS.last()
        updates.heard(site, newer)
        updates.install(site)
        updates().heard(site, newer)
        assertEquals(2, server.requestCount)
        thisApp = true
        serve(bytes, app = newest)
        updates.heard(site, newest)
        assertArrayEquals(bytes, installer.installed.await())
    }

    @Test
    fun installsNothingThatIsNotTheDownloadDescribed() = runTest {
        val updates = updates()
        serve("another apk!!".toByteArray(), sha256 = sha256(bytes))
        updates.heard(site, newer)
        assertEquals(Update.Available(newer, Problem.MISMATCH), updates.update.first { it is Update.Available && it.problem != null })
        assertTrue(installer.installs.isEmpty())
        assertFalse(apk.exists())
    }

    @Test
    fun saysTheDownloadFailedWhenTheDspRefusesIt() = runTest {
        val updates = updates()
        server.enqueue(json(404, """{"error":"phones_app_not_found","message":"…"}"""))
        updates.heard(site, newer)
        assertEquals(Update.Available(newer, Problem.DOWNLOAD), updates.update.first { it is Update.Available && it.problem != null })
    }

    @Test
    fun installsTheVersionTheDetailsNameOverAnEarlierAnswer() = runTest {
        installer.outcome = Outcome.FAILED
        val updates = updates()
        serve(bytes, app = newer)
        updates.heard(site, newest)
        assertEquals(Update.Available(newer, Problem.INSTALL), updates.update.first { it is Update.Available && it.problem != null })
    }

    @Test
    fun saysTheDownloadOfTheVersionTheDetailsNameFailed() = runTest {
        val updates = updates()
        server.enqueue(json(200, """{"version":"1.0.5","versionCode":5,"size":13,"sha256":"${sha256(bytes)}"}"""))
        server.enqueue(json(404, """{"error":"phones_app_not_found","message":"…"}"""))
        updates.heard(site, newest)
        assertEquals(Update.Available(newer, Problem.DOWNLOAD), updates.update.first { it is Update.Available && it.problem != null })
        assertEquals(Hold(newer.versionCode, Updates.RETRIES_MS.first()), holds.hold)
    }

    @Test
    fun saysTheVersionTheDetailsNameIsNotTheOneDescribed() = runTest {
        val updates = updates()
        serve("another apk!!".toByteArray(), sha256 = sha256(bytes), app = newer)
        updates.heard(site, newest)
        assertEquals(Update.Available(newer, Problem.MISMATCH), updates.update.first { it is Update.Available && it.problem != null })
        assertEquals(Hold(newer.versionCode, Updates.RETRIES_MS.first()), holds.hold)
    }

    @Test
    fun installsNothingOnceTheDspNoLongerHandsOutANewerVersion() = runTest {
        val updates = updates()
        server.enqueue(json(200, """{"version":"1.0.4","versionCode":4,"size":13,"sha256":"${sha256(bytes)}"}"""))
        updates.heard(site, newer)
        assertEquals(Update.None, updates.update.first { it == Update.None })
        assertEquals(1, server.requestCount)
        assertTrue(installer.installs.isEmpty())
    }

    private fun TestScope.updates() = Updates(
        Dispatch(OkHttpClient(), own),
        own,
        apk,
        { thisApp },
        installer,
        notice,
        holds,
        {
            quietAsked.complete(Unit)
            quiet.await()
        },
        { released++ },
        backgroundScope,
        { _, error -> throw AssertionError("Unexpected", error) },
        now = { clock },
    )

    private fun serve(apk: ByteArray, sha256: String = sha256(apk), app: AppVersion = newer) {
        server.enqueue(details(apk, sha256, app))
        server.enqueue(download(apk))
    }

    private fun details(apk: ByteArray, sha256: String = sha256(apk), app: AppVersion = newer) =
        json(200, """{"version":"${app.version}","versionCode":${app.versionCode},"size":${apk.size},"sha256":"$sha256"}""")

    private fun download(apk: ByteArray) =
        MockResponse.Builder().code(200).addHeader("Content-Type", "application/vnd.android.package-archive").body(okio.Buffer().write(apk)).build()

    private fun json(code: Int, body: String) =
        MockResponse.Builder().code(code).addHeader("Content-Type", "application/json").body(body).build()

    private fun sha256(bytes: ByteArray) = MessageDigest.getInstance("SHA-256").digest(bytes).joinToString("") { "%02x".format(it) }

    /** Android's installer: each APK handed to it, the first [installed]; while it asks the person, a try dropped is [dropped]. */
    private class FakeInstaller : Installer {
        var allowed = true
        var outcome = Outcome.INSTALLED
        var confirm = false
        var asks = 0
        val installs = mutableListOf<ByteArray>()
        val installed = CompletableDeferred<ByteArray>()
        val tries = Channel<Unit>(Channel.UNLIMITED)
        val dropped = CompletableDeferred<Unit>()

        override fun allowed() = allowed

        override suspend fun install(apk: File, confirming: () -> Unit): Outcome {
            val bytes = apk.readBytes()
            installs += bytes
            installed.complete(bytes)
            tries.trySend(Unit)
            if (!confirm) return outcome
            confirming()
            try {
                return CompletableDeferred<Outcome>().await()
            } finally {
                dropped.complete(Unit)
            }
        }

        override fun askAgain() {
            asks++
        }
    }

    /** The phone's notifications, and the app's storage, which outlive each [Updates] made. */
    private class FakeNotice : Notice {
        var allowed = true
        val shown = mutableListOf<AppVersion>()
        var visible: AppVersion? = null
        var cancelled = 0
        override var showing: Int? = null

        override fun show(app: AppVersion): Boolean {
            if (!allowed) return false
            shown += app
            visible = app
            return true
        }

        override fun cancel() {
            visible = null
            cancelled++
        }
    }

    /** The app's storage of the update held back, which outlives each [Updates] made. */
    private class FakeHolds : Holds {
        override var hold: Hold? = null
    }
}
