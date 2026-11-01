package com.dspdispatch.relay.update

import android.annotation.SuppressLint
import android.content.Context
import android.content.pm.PackageInfo
import android.content.pm.PackageManager
import android.content.pm.Signature
import android.os.Build
import java.io.File

/**
 * Whether an APK is this app: the same package name, signed with the same certificate as the app
 * installed. The key never changes, so the certificates must match as they are.
 */
class PackageCheck(private val context: Context) : ApkCheck {
    override fun isThisApp(apk: File): Boolean {
        val packages = context.packageManager
        val archive = archive(packages, apk) ?: return false
        if (archive.packageName != context.packageName) return false
        val signers = signers(archive)
        return signers.isNotEmpty() && signers == signers(installed(packages))
    }

    private fun archive(packages: PackageManager, apk: File): PackageInfo? =
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
            packages.getPackageArchiveInfo(apk.path, PackageManager.PackageInfoFlags.of(SIGNERS.toLong()))
        } else {
            // Deprecated from Android 13, where the flags above replace it.
            @Suppress("DEPRECATION")
            packages.getPackageArchiveInfo(apk.path, SIGNERS)
        }

    private fun installed(packages: PackageManager): PackageInfo =
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
            packages.getPackageInfo(context.packageName, PackageManager.PackageInfoFlags.of(SIGNERS.toLong()))
        } else {
            // Deprecated from Android 13, where the flags above replace it.
            @Suppress("DEPRECATION")
            packages.getPackageInfo(context.packageName, SIGNERS)
        }

    private fun signers(info: PackageInfo): Set<Signature> =
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.P) {
            val signing = info.signingInfo ?: return emptySet()
            (if (signing.hasMultipleSigners()) signing.apkContentsSigners else signing.signingCertificateHistory).toSet()
        } else {
            // Deprecated from Android 9, where signingInfo replaces it.
            @Suppress("DEPRECATION")
            info.signatures.orEmpty().toSet()
        }

    private companion object {
        /**
         * The signers: Android 9+'s signing certificates, or before it the signatures, which lint
         * warns can be spoofed on Android 4.4 and older; the app needs Android 8.
         */
        @SuppressLint("PackageManagerGetSignatures")
        @Suppress("DEPRECATION")
        val SIGNERS = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.P) PackageManager.GET_SIGNING_CERTIFICATES else PackageManager.GET_SIGNATURES
    }
}
