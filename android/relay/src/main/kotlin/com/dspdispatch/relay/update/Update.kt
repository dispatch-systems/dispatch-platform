package com.dspdispatch.relay.update

import com.dspdispatch.relay.dispatch.AppVersion

/** Where updating the app stands: nothing newer, a newer version offered, or installing it. */
sealed interface Update {
    /** The newer version, while there's one. */
    val app: AppVersion?

    data object None : Update {
        override val app = null
    }

    /** A newer version, [app], to install. */
    data class Available(override val app: AppVersion) : Update

    /** Downloading [app]. */
    data class Downloading(override val app: AppVersion) : Update

    /** Android is installing [app]. */
    data class Installing(override val app: AppVersion) : Update

    /** Android is asking the person to confirm installing [app], which the update's notification shows again. */
    data class Confirming(override val app: AppVersion) : Update
}

/** Why the last try at updating failed. */
enum class Problem {
    /** The DSP couldn't be reached, or refused the download. */
    DOWNLOAD,

    /** The download wasn't the size or SHA-256 the DSP gave. */
    MISMATCH,

    /** The APK isn't this app, or isn't signed with this app's key. */
    NOT_THIS_APP,

    /** Android didn't install it. */
    INSTALL,
}
