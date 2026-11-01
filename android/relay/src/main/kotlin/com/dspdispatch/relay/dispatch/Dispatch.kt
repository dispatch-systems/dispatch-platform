package com.dspdispatch.relay.dispatch

import java.io.IOException
import java.io.InputStream
import java.util.concurrent.TimeUnit
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.suspendCancellableCoroutine
import kotlinx.coroutines.withContext
import kotlinx.serialization.KSerializer
import kotlinx.serialization.serializer
import okhttp3.Call
import okhttp3.Callback
import okhttp3.HttpUrl.Companion.toHttpUrl
import okhttp3.MediaType
import okhttp3.MediaType.Companion.toMediaType
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.RequestBody
import okhttp3.RequestBody.Companion.toRequestBody
import okhttp3.Response
import okio.BufferedSink
import okio.source
import kotlin.coroutines.resume
import kotlin.coroutines.resumeWithException

/**
 * The calls the app makes to its DSP, at `<site>/api/communications/phone/…`, as the app's [own]
 * version. Each answers what Dispatch did, or throws a [Refusal] for an answer refusing it, or an
 * [IOException] for none.
 */
class Dispatch(http: OkHttpClient, private val own: AppVersion) {
    // Never a cookie: Dispatch refuses a call carrying one, as a browser's would.
    private val http = http.newBuilder().cookieJar(okhttp3.CookieJar.NO_COOKIES).build()

    // The outbox waits up to 20 seconds (`WAIT`) before answering with nothing.
    private val waiting = this.http.newBuilder().readTimeout(OUTBOX_READ_SECONDS, TimeUnit.SECONDS).build()

    /** Trades a pairing code for this phone's key. Never made again by itself: a code works once. */
    suspend fun pair(site: String, request: PairRequest): PairAnswer =
        http.call(post(site, "pair", wire.encodeToString(request)))

    /**
     * Waits on the outbox for what Dispatch has for this phone to send; checks the phone in, saying
     * the app's own version.
     */
    suspend fun outbox(site: String, key: String): Outbox {
        val url = address(site, "outbox").toHttpUrl().newBuilder()
            .addQueryParameter("version", own.version)
            .addQueryParameter("versionCode", own.versionCode.toString())
            .build()
        return waiting.call(Request.Builder().url(url).header(AUTHORIZATION, bearer(key)).build())
    }

    /** The app's latest version as the DSP hands it out, which anyone at its address may ask. */
    suspend fun app(site: String): AppDetails = http.call(Request.Builder().url(address(site, "app")).build())

    /** Downloads that version's APK, handing its bytes to [save] as they arrive; answers what it does. */
    suspend fun <T> apk(site: String, save: (InputStream) -> T): T = withContext(Dispatchers.IO) {
        http.newCall(Request.Builder().url(address(site, "app/download")).build()).await().use { response ->
            if (!response.isSuccessful) throw refusal(response, response.body.string())
            response.body.byteStream().use(save)
        }
    }

    /** Reports texts the phone received or sent, answering Dispatch's IDs of them and their files. */
    suspend fun texts(site: String, key: String, report: TextsReport): TextsAnswer =
        http.call(post(site, "texts", wire.encodeToString(report), key))

    /**
     * Sends the [size] bytes [open] streams as those of the reported file Dispatch knows as [id].
     */
    suspend fun file(site: String, key: String, id: String, size: Long, open: () -> InputStream) {
        http.call<Done>(post(site, "files/$id", Upload(size, open), key))
    }

    private fun post(site: String, path: String, json: String, key: String? = null) =
        post(site, path, json.toRequestBody(JSON), key)

    // Dispatch takes anything but a read only from its own address's pages: the DSP's address
    // as the call's origin, as a page of it would send.
    private fun post(site: String, path: String, body: RequestBody, key: String?) =
        Request.Builder().url(address(site, path)).post(body).header(ORIGIN, site.trimEnd('/')).apply {
            if (key != null) header(AUTHORIZATION, bearer(key))
        }.build()

    /** A file's bytes, streamed as they're sent, never held whole: a picture may be 20 MiB. */
    private class Upload(private val size: Long, private val open: () -> InputStream) : RequestBody() {
        override fun contentType(): MediaType = BYTES

        override fun contentLength() = size

        override fun writeTo(sink: BufferedSink) {
            open().source().use(sink::writeAll)
        }
    }

    // Reading the answer's body blocks on the network too: never on the caller's thread.
    private suspend inline fun <reified T> OkHttpClient.call(request: Request): T = withContext(Dispatchers.IO) {
        newCall(request).await().use { response -> read(response, serializer<T>()) }
    }

    private fun <T> read(response: Response, serializer: KSerializer<T>): T {
        val body = response.body.string()
        if (!response.isSuccessful) throw refusal(response, body)
        return wire.decodeFromString(serializer, body)
    }

    private fun refusal(response: Response, body: String): Refusal {
        val code = runCatching { wire.decodeFromString<RefusalAnswer>(body).error }.getOrNull()
        return Refusal(response.code, code ?: "http_${response.code}")
    }

    companion object {
        private const val PATH = "api/communications/phone"
        private const val AUTHORIZATION = "Authorization"
        private const val ORIGIN = "Origin"
        private const val OUTBOX_READ_SECONDS = 40L
        private val JSON = "application/json".toMediaType()
        private val BYTES = "application/octet-stream".toMediaType()

        private fun address(site: String, path: String) = "${site.trimEnd('/')}/$PATH/$path"

        private fun bearer(key: String) = "Bearer $key"
    }
}

/** Makes the call, cancelling it when the coroutine waiting on it is. */
internal suspend fun Call.await(): Response = suspendCancellableCoroutine { waiting ->
    enqueue(object : Callback {
        override fun onResponse(call: Call, response: Response) = waiting.resume(response) { _, _, _ -> response.close() }

        override fun onFailure(call: Call, e: IOException) = waiting.resumeWithException(e)
    })
    waiting.invokeOnCancellation { cancel() }
}
