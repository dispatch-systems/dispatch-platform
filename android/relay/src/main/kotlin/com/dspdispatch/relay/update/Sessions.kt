package com.dspdispatch.relay.update

import java.io.File

/**
 * Android's sessions for installing this app's updates (`PhoneSessions`). Each call blocks: make
 * it off the main thread. Android refuses any of them with a [SecurityException] where the phone
 * doesn't let the app install, and writing the APK may fail with an [java.io.IOException].
 */
interface Sessions {
    /** Opens a session installing [apk] as this app, abandoning any an earlier process left; its ID. */
    fun create(apk: File): Int

    /** Writes [apk] into session [id] and commits it, for Android to tell [InstallReceiver] what came of it. */
    fun commit(id: Int, apk: File)

    /** Abandons session [id], while it's open. */
    fun abandon(id: Int)
}
