package ua.powerwatch.app

import android.app.Activity
import android.content.Intent
import android.content.pm.PackageManager
import android.net.Uri
import android.os.Build
import android.provider.Settings
import androidx.core.content.FileProvider
import java.io.File
import java.security.MessageDigest

/** Android remains the installation authority; this code never installs silently. */
@androidx.annotation.Keep
object UpdateInstaller {
    fun install(activity: Activity, args: InstallUpdateArgs): String {
        val file = validate(activity, args)
        val manager = activity.packageManager
        if (!manager.canRequestPackageInstalls()) {
            activity.startActivity(Intent(Settings.ACTION_MANAGE_UNKNOWN_APP_SOURCES, Uri.parse("package:${activity.packageName}")))
            return "permission_required"
        }
        val uri = FileProvider.getUriForFile(activity, "${activity.packageName}.fileprovider", file)
        activity.startActivity(Intent(Intent.ACTION_VIEW).setDataAndType(uri, "application/vnd.android.package-archive")
            .addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION))
        return "installer_opened"
    }
    @Suppress("DEPRECATION")
    fun validate(activity: Activity, args: InstallUpdateArgs): File {
        val file = File(args.path).canonicalFile
        val directory = File(activity.cacheDir, "updates").canonicalFile
        require(file.parentFile == directory && file.isFile && file.extension == "apk")
        val hash = MessageDigest.getInstance("SHA-256")
        file.inputStream().use { input ->
            val buffer = ByteArray(65536)
            while (true) {
                val size = input.read(buffer)
                if (size < 0) break
                hash.update(buffer, 0, size)
            }
        }
        require(hash.digest().joinToString("") { "%02x".format(it) } == args.sha256.lowercase())
        val manager = activity.packageManager
        // Android 9 only collects archive certificates when GET_SIGNATURES is
        // also requested. Both paths still use Android's APK signature parser.
        val flags = PackageManager.GET_SIGNING_CERTIFICATES or
            (if (Build.VERSION.SDK_INT == 28) PackageManager.GET_SIGNATURES else 0)
        val archive = manager.getPackageArchiveInfo(file.path, flags)
            ?: throw IllegalArgumentException("Invalid APK")
        val current = manager.getPackageInfo(activity.packageName, flags)
        require(archive.packageName == activity.packageName && archive.longVersionCode == args.versionCode && archive.longVersionCode > current.longVersionCode)
        val expected = (current.signingInfo?.apkContentsSigners ?: if (Build.VERSION.SDK_INT == 28) current.signatures else null)?.map { it.toCharsString() }?.toSet()
        val actual = (archive.signingInfo?.apkContentsSigners ?: if (Build.VERSION.SDK_INT == 28) archive.signatures else null)?.map { it.toCharsString() }?.toSet()
        require(!expected.isNullOrEmpty() && actual == expected)
        return file
    }
}
