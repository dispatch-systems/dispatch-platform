package com.dspdispatch.relay

import android.Manifest
import android.content.Intent
import android.content.pm.PackageManager
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.core.content.ContextCompat
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.dspdispatch.relay.pairing.PairingLink
import com.dspdispatch.relay.pairing.Standing
import com.dspdispatch.relay.ui.CameraScreen
import com.dspdispatch.relay.ui.ConnectedScreen
import com.dspdispatch.relay.ui.RemovedScreen
import com.dspdispatch.relay.ui.ScanScreen
import com.dspdispatch.relay.ui.StartScreen

/**
 * The app's one screen at a time: pairing while unpaired or removed, then the relay's state. A
 * pairing link opens it too, as the phone's own camera app offers one from the code's QR.
 */
class MainActivity : ComponentActivity() {
    private var scanning by mutableStateOf(false)
    private var cameraAllowed by mutableStateOf(false)

    /** A link read while the app asks to read the phone's number, which goes with pairing. */
    private var waiting: PairingLink? = null

    private val numberThenPair = registerForActivityResult(ActivityResultContracts.RequestPermission()) {
        waiting?.let(relay::pair)
        waiting = null
    }
    private val camera = registerForActivityResult(ActivityResultContracts.RequestMultiplePermissions()) {
        cameraAllowed = allowed(Manifest.permission.CAMERA)
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        scanning = savedInstanceState?.getBoolean(SCANNING) ?: false
        waiting = savedInstanceState?.getString(WAITING)?.let(PairingLink::parse)
        if (savedInstanceState == null) opened(intent)
        setContent {
            val standing by relay.pairings.standing.collectAsStateWithLifecycle()
            val link by relay.connection.link.collectAsStateWithLifecycle()
            val attempt by relay.attempting.collectAsStateWithLifecycle()
            val pairing = attempt == Attempt.Pairing
            val problem = (attempt as? Attempt.Failed)?.problem
            val now = standing
            when {
                now is Standing.Pairing -> ConnectedScreen(now.paired, link)
                scanning && !cameraAllowed -> CameraScreen(::askForCamera) { scanning = false }
                scanning -> ScanScreen(::read) { scanning = false }
                now is Standing.Removed -> RemovedScreen(now.dsp, ::scan, pairing, problem)
                else -> StartScreen(::scan, pairing, problem)
            }
        }
    }

    override fun onStart() {
        super.onStart()
        // While shown, Android lets the app start the relay: whenever paired, make sure it runs.
        if (relay.pairings.standing.value is Standing.Pairing) RelayService.start(this)
    }

    override fun onResume() {
        super.onResume()
        cameraAllowed = allowed(Manifest.permission.CAMERA)
    }

    override fun onSaveInstanceState(outState: Bundle) {
        super.onSaveInstanceState(outState)
        outState.putBoolean(SCANNING, scanning)
        waiting?.let { outState.putString(WAITING, it.text) }
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        opened(intent)
    }

    private fun opened(intent: Intent) {
        intent.data?.toString()?.let(::read)
    }

    private fun scan() {
        relay.again()
        scanning = true
        if (!cameraAllowed) askForCamera()
    }

    private fun askForCamera() = camera.launch(arrayOf(Manifest.permission.CAMERA, Manifest.permission.READ_PHONE_NUMBERS))

    /** What a QR code or a link held: pairs with the DSP it names, if this build pairs with it. */
    private fun read(text: String) {
        if (relay.attempting.value == Attempt.Pairing || relay.pairings.standing.value is Standing.Pairing) return
        scanning = false
        val link = relay.read(text) ?: return
        if (allowed(Manifest.permission.READ_PHONE_NUMBERS)) {
            relay.pair(link)
        } else {
            waiting = link
            numberThenPair.launch(Manifest.permission.READ_PHONE_NUMBERS)
        }
    }

    private fun allowed(permission: String) =
        ContextCompat.checkSelfPermission(this, permission) == PackageManager.PERMISSION_GRANTED

    private companion object {
        const val SCANNING = "scanning"
        const val WAITING = "waiting"
    }
}
