package com.dspdispatch.relay.update

import android.content.pm.PackageInstaller
import java.io.File
import java.io.IOException
import kotlinx.coroutines.test.runTest
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Test

class SessionInstallerTest {
    private val apk = File.createTempFile("update", ".apk")
    private val sessions = FakeSessions()
    private val installer = SessionInstaller(sessions)

    @After
    fun stop() {
        apk.delete()
    }

    @Test
    fun installsOnceAndroidSaysItDid() = runTest {
        sessions.answer = { installer.said(it, PackageInstaller.STATUS_SUCCESS, null) }
        assertEquals(Outcome.INSTALLED, installer.install(apk) {})
        assertEquals(listOf(SESSION), sessions.committed)
        assertEquals(emptyList<Int>(), sessions.abandoned)
    }

    @Test
    fun failsWhenAndroidRefusesToOpenASession() = runTest {
        sessions.refuseCreate = SecurityException("installing is restricted for this user")
        assertEquals(Outcome.FAILED, installer.install(apk) {})
        sessions.refuseCreate = IOException("no room for a session")
        assertEquals(Outcome.FAILED, installer.install(apk) {})
        assertEquals(emptyList<Int>(), sessions.committed)
        assertEquals(emptyList<Int>(), sessions.abandoned)
    }

    @Test
    fun failsAndAbandonsTheSessionWhenAndroidRefusesItsCommit() = runTest {
        sessions.refuseCommit = SecurityException("installing is restricted for this user")
        assertEquals(Outcome.FAILED, installer.install(apk) {})
        assertEquals(listOf(SESSION), sessions.abandoned)
    }

    private class FakeSessions : Sessions {
        var refuseCreate: Exception? = null
        var refuseCommit: Exception? = null
        var answer: (Int) -> Unit = {}
        val committed = mutableListOf<Int>()
        val abandoned = mutableListOf<Int>()

        override fun allowed() = true

        override fun create(apk: File): Int {
            refuseCreate?.let { throw it }
            return SESSION
        }

        override fun commit(id: Int, apk: File) {
            refuseCommit?.let { throw it }
            committed += id
            answer(id)
        }

        override fun abandon(id: Int) {
            abandoned += id
        }
    }

    private companion object {
        const val SESSION = 7
    }
}
