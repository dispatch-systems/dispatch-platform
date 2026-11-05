package com.dspdispatch.relay.pairing

import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyPermanentlyInvalidatedException
import android.security.keystore.KeyProperties
import android.util.Base64
import java.io.IOException
import java.security.GeneralSecurityException
import java.security.KeyStore
import java.security.UnrecoverableEntryException
import javax.crypto.AEADBadTagException
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec

/**
 * Keeps a secret encrypted by a key of Android's keystore, which never leaves it: what's written
 * to the app's storage is useless anywhere else.
 */
internal object Vault {
    private const val STORE = "AndroidKeyStore"
    private const val ALIAS = "pairing"
    private const val CIPHER = "AES/GCM/NoPadding"
    private const val TAG_BITS = 128
    private const val IV_BYTES = 12

    fun seal(secret: String): String {
        val cipher = Cipher.getInstance(CIPHER).apply { init(Cipher.ENCRYPT_MODE, key() ?: made()) }
        return Base64.encodeToString(cipher.iv + cipher.doFinal(secret.toByteArray()), Base64.NO_WRAP)
    }

    /**
     * The secret [seal] sealed, or null once it can never be read, as after the keystore lost its
     * key; [Unreachable] while the keystore can't be read just now, as it's still starting.
     */
    fun open(sealed: String): String? = try {
        val bytes = Base64.decode(sealed, Base64.NO_WRAP)
        val key = key() ?: return null
        val cipher = Cipher.getInstance(CIPHER).apply {
            init(Cipher.DECRYPT_MODE, key, GCMParameterSpec(TAG_BITS, bytes, 0, IV_BYTES))
        }
        String(cipher.doFinal(bytes, IV_BYTES, bytes.size - IV_BYTES))
    } catch (_: IllegalArgumentException) {
        // Not something sealed.
        null
    } catch (_: AEADBadTagException) {
        // Sealed by a key the keystore no longer has.
        null
    } catch (_: UnrecoverableEntryException) {
        null
    } catch (_: KeyPermanentlyInvalidatedException) {
        null
    } catch (failed: GeneralSecurityException) {
        throw Unreachable(failed)
    } catch (failed: IOException) {
        throw Unreachable(failed)
    } catch (failed: RuntimeException) {
        // As the keystore's own ProviderException: read as the app starts, it never ends the app.
        throw Unreachable(failed)
    }

    /** The keystore can't be read just now: what it sealed is still there, to read again. */
    class Unreachable(cause: Exception) : Exception(cause)

    /**
     * The keystore's key, if it has one. Never made as a secret is opened: that would replace one
     * the keystore didn't give a moment.
     */
    private fun key(): SecretKey? {
        val store = KeyStore.getInstance(STORE).apply { load(null) }
        return (store.getEntry(ALIAS, null) as? KeyStore.SecretKeyEntry)?.secretKey
    }

    private fun made(): SecretKey = KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, STORE).apply {
        init(
            KeyGenParameterSpec.Builder(ALIAS, KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT)
                .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
                .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
                .setKeySize(256)
                .build(),
        )
    }.generateKey()
}
