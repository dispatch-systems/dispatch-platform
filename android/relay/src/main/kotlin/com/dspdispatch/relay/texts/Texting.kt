package com.dspdispatch.relay.texts

import android.Manifest
import android.content.Context
import android.content.pm.PackageManager
import androidx.core.content.ContextCompat

/** Whether the phone lets the app read its texts and hear them arrive, the setup's Texting step. */
fun textingAllowed(context: Context): Boolean = listOf(Manifest.permission.READ_SMS, Manifest.permission.RECEIVE_SMS)
    .all { ContextCompat.checkSelfPermission(context, it) == PackageManager.PERMISSION_GRANTED }
