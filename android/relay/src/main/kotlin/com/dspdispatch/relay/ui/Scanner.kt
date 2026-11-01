package com.dspdispatch.relay.ui

import androidx.camera.core.CameraSelector
import androidx.camera.core.ImageAnalysis
import androidx.camera.core.ImageProxy
import androidx.camera.core.Preview
import androidx.camera.lifecycle.ProcessCameraProvider
import androidx.camera.view.PreviewView
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.viewinterop.AndroidView
import androidx.core.content.ContextCompat
import androidx.lifecycle.compose.LocalLifecycleOwner
import com.google.zxing.BarcodeFormat
import com.google.zxing.BinaryBitmap
import com.google.zxing.DecodeHintType
import com.google.zxing.MultiFormatReader
import com.google.zxing.NotFoundException
import com.google.zxing.PlanarYUVLuminanceSource
import com.google.zxing.common.HybridBinarizer
import java.util.concurrent.Executors

/** The back camera's picture, reading the first QR code it sees and handing its text to [read]. */
@Composable
fun Scanner(read: (String) -> Unit, modifier: Modifier = Modifier) {
    val context = LocalContext.current
    val owner = LocalLifecycleOwner.current
    val reading = rememberUpdatedState(read)
    val view = remember { PreviewView(context) }
    DisposableEffect(owner) {
        // Everything here runs on the main thread. Leaving the scanner can come before the camera
        // is ready: once closed, nothing binds, and a late read is dropped.
        var closed = false
        val analysis = Executors.newSingleThreadExecutor()
        val preview = Preview.Builder().build().also { it.surfaceProvider = view.surfaceProvider }
        val analyzer = ImageAnalysis.Builder()
            .setBackpressureStrategy(ImageAnalysis.STRATEGY_KEEP_ONLY_LATEST)
            .build()
            .also { it.setAnalyzer(analysis, QrReader { text -> view.post { if (!closed) reading.value(text) } }) }
        val future = ProcessCameraProvider.getInstance(context)
        future.addListener({
            if (!closed) future.get().bindToLifecycle(owner, CameraSelector.DEFAULT_BACK_CAMERA, preview, analyzer)
        }, ContextCompat.getMainExecutor(context))
        onDispose {
            closed = true
            if (future.isDone) future.get().unbind(preview, analyzer)
            analysis.shutdown()
        }
    }
    AndroidView({ view }, modifier)
}

/** Reads a QR code from each frame's brightness, as ZXing does without Google Play services. */
private class QrReader(private val found: (String) -> Unit) : ImageAnalysis.Analyzer {
    private val reader = MultiFormatReader().apply {
        setHints(mapOf(DecodeHintType.POSSIBLE_FORMATS to listOf(BarcodeFormat.QR_CODE)))
    }

    override fun analyze(image: ImageProxy) = image.use {
        val plane = it.planes[0]
        val bytes = ByteArray(plane.buffer.remaining()).also(plane.buffer::get)
        val source = PlanarYUVLuminanceSource(bytes, plane.rowStride, it.height, 0, 0, it.width, it.height, false)
        try {
            found(reader.decodeWithState(BinaryBitmap(HybridBinarizer(source))).text)
        } catch (_: NotFoundException) {
            // No code in this frame: the next may hold one.
        } finally {
            reader.reset()
        }
    }
}
