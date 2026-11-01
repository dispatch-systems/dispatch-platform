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
import androidx.lifecycle.lifecycleScope
import com.dspdispatch.relay.dispatch.Refusal
import com.dspdispatch.relay.pairing.PairingLink
import com.dspdispatch.relay.pairing.Standing
import com.dspdispatch.relay.ui.CameraScreen
import com.dspdispatch.relay.ui.ConnectedScreen
import com.dspdispatch.relay.ui.RemovedScreen
import com.dspdispatch.relay.ui.ScanScreen
import com.dspdispatch.relay.ui.StartScreen
import java.io.IOException
import kotlinx.coroutines.launch
import kotlinx.serialization.SerializationException

/**
 * The app's one screen at a time: pairing while unpaired or removed, then the relay's state. A
 * pairing link opens it too, as the phone's own camera app offers one from the code's QR.
 */
class MainActivity : ComponentActivity() {
    private var scanning by mutableStateOf(false)
    private var pairing by mutableStateOf(false)
    private var problem by mutableStateOf<Int?>(null)
    private var cameraAllowed by mutableStateOf(false)
    private var waiting: PairingLink? = null

    // The phone's own number goes to its DSP as it pairs, when Android allows reading it.
    private val numberThenPair = registerForActivityResult(ActivityResultContracts.RequestPermission()) {
        waiting?.let(::pair)
        waiting = null
    }
    private val camera = registerForActivityResult(ActivityResultContracts.RequestMultiplePermissions()) {
        cameraAllowed = allowed(Manifest.permission.CAMERA)
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        opened(intent)
        if (relay.pairings.standing.value is Standing.Pairing) RelayService.start(this)
        setContent {
            val standing by relay.pairings.standing.collectAsStateWithLifecycle()
            val link by relay.connection.link.collectAsStateWithLifecycle()
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

    override fun onResume() {
        super.onResume()
        cameraAllowed = allowed(Manifest.permission.CAMERA)
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        opened(intent)
    }

    private fun opened(intent: Intent) {
        intent.data?.toString()?.let(::read)
    }

    private fun scan() {
        problem = null
        scanning = true
        if (!cameraAllowed) askForCamera()
    }

    private fun askForCamera() = camera.launch(arrayOf(Manifest.permission.CAMERA, Manifest.permission.READ_PHONE_NUMBERS))

    /** What a QR code or a link held: pairs with the DSP it names, if this build pairs with it. */
    private fun read(text: String) {
        if (pairing || relay.pairings.standing.value is Standing.Pairing) return
        scanning = false
        val link = PairingLink.parse(text)
        problem = when {
            link == null -> R.string.pairing_not_a_code
            !Server.THIS_BUILD.accepts(link.site) -> R.string.pairing_other_server
            else -> null
        }
        if (link == null || problem != null) return
        if (allowed(Manifest.permission.READ_PHONE_NUMBERS)) {
            pair(link)
        } else {
            waiting = link
            numberThenPair.launch(Manifest.permission.READ_PHONE_NUMBERS)
        }
    }

    private fun pair(link: PairingLink) {
        pairing = true
        lifecycleScope.launch {
            problem = try {
                relay.pair(link)
                null
            } catch (refusal: Refusal) {
                if (refusal.code == Refusal.PAIRING_INVALID) R.string.pairing_invalid else R.string.pairing_refused
            } catch (_: IOException) {
                R.string.pairing_unreachable
            } catch (_: SerializationException) {
                R.string.pairing_unreachable
            }
            pairing = false
        }
    }

    private fun allowed(permission: String) =
        ContextCompat.checkSelfPermission(this, permission) == PackageManager.PERMISSION_GRANTED
}
