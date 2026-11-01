package com.dspdispatch.relay.update

import androidx.annotation.StringRes
import com.dspdispatch.relay.R
import com.dspdispatch.relay.dispatch.AppVersion

/** Where updating the app stands: nothing newer, a newer version offered, or installing it. */
sealed interface Update {
    /** The newer version, while there's one. */
    val app: AppVersion?

    data object None : Update {
        override val app = null
    }

    /** A newer version, [app], to install; with the last try's [problem], if it failed. */
    data class Available(override val app: AppVersion, val problem: Problem? = null) : Update

    /** Downloading [app], [fraction] of it so far, from 0 to 1. */
    data class Downloading(override val app: AppVersion, val fraction: Float) : Update

    /** Android is installing [app]. */
    data class Installing(override val app: AppVersion) : Update

    /** Android is asking the person to confirm installing [app], which Install shows again. */
    data class Confirming(override val app: AppVersion) : Update
}

/** Why the last try at updating failed, as the running screen says it. */
enum class Problem(@param:StringRes val label: Int) {
    /** The DSP couldn't be reached, or refused the download. */
    DOWNLOAD(R.string.update_download_failed),

    /** The download wasn't the size or SHA-256 the DSP gave. */
    MISMATCH(R.string.update_mismatch),

    /** The APK isn't this app, or isn't signed with this app's key. */
    NOT_THIS_APP(R.string.update_not_this_app),

    /** Android didn't install it. */
    INSTALL(R.string.update_install_failed),
}
