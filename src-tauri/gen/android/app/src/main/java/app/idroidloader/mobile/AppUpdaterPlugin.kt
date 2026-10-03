package app.idroidloader.mobile

import android.app.Activity
import android.content.Intent
import android.content.pm.PackageInfo
import android.content.pm.PackageManager
import android.net.Uri
import android.os.Build
import android.provider.Settings
import androidx.core.content.FileProvider
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin
import org.json.JSONObject
import java.io.File
import java.net.URL
import java.security.MessageDigest
import java.util.concurrent.Executors
import javax.net.ssl.HttpsURLConnection

@InvokeArg
class AppUpdateArgs {
    lateinit var channel: String
    var versionCode: Long = 0
}

@TauriPlugin
class AppUpdaterPlugin(private val host: Activity) : Plugin(host) {
    private val worker = Executors.newSingleThreadExecutor()
    // The frontend receives only version information. Download URLs/hashes stay native.
    private val candidates = mutableMapOf<String, UpdateCandidate>()
    private val installedCode: Long get() = versionCode(host.packageManager.getPackageInfo(host.packageName, 0))

    private fun execute(invoke: Invoke, operation: () -> Unit) {
        worker.execute {
            try {
                operation()
            } catch (error: Exception) {
                val reason = error.message?.takeIf { it in setOf(
                    "invalid_metadata", "rate_limited", "download_changed", "wrong_package",
                    "signature_mismatch", "stale_update", "http_error", "installer_unavailable",
                ) } ?: "network_error"
                invoke.reject(reason)
            }
        }
    }

    @Command
    fun info(invoke: Invoke) {
        invoke.resolve(JSObject().apply {
            put("version", BuildConfig.VERSION_NAME)
            put("versionCode", installedCode)
            put("channel", BuildConfig.UPDATE_CHANNEL)
        })
    }

    @Command
    fun check(invoke: Invoke) = execute(invoke) {
        val channel = invoke.parseArgs(AppUpdateArgs::class.java).channel
        require(channel == "release" || channel == "nightly") { "invalid_metadata" }
        candidates.remove(channel)
        val endpoint = if (channel == "nightly") "tags/nightly" else "latest"
        val release = readJson("https://api.github.com/repos/${UpdateCandidate.REPOSITORY}/releases/$endpoint", missingAllowed = true)
        val result = JSObject().apply { put("status", "unavailable") }
        if (release != null && !release.optBoolean("draft") &&
            (channel == "nightly" || !release.optBoolean("prerelease"))) {
            val assets = release.getJSONArray("assets")
            val metadata = (0 until assets.length()).map { assets.getJSONObject(it) }
                .firstOrNull { it.optString("name") == "android-update.json" }
            val apk = (0 until assets.length()).map { assets.getJSONObject(it) }
                .firstOrNull { it.optString("name") == "iDroidLoader-arm64.apk" }
            if (metadata != null && apk != null) {
                val metadataUrl = metadata.getString("browser_download_url")
                require(UpdateCandidate.isReleaseAsset(metadataUrl, "android-update.json")) { "invalid_metadata" }
                val manifest = requireNotNull(readJson(metadataUrl))
                require(manifest.getInt("schemaVersion") == 1 && manifest.getString("channel") == channel &&
                    manifest.getString("packageName") == host.packageName && manifest.getString("abi") == "arm64-v8a") { "invalid_metadata" }
                val candidate = UpdateCandidate(channel, manifest.getString("version"), manifest.getLong("versionCode"),
                    manifest.getString("sha256"), manifest.getLong("size"), apk.getString("browser_download_url"))
                require(candidate.size == apk.getLong("size")) { "invalid_metadata" }
                if (candidate.isNewerThan(installedCode)) {
                    candidates[channel] = candidate
                    result.put("status", "available")
                    result.put("version", candidate.version)
                    result.put("versionCode", candidate.versionCode)
                } else {
                    result.put("status", "up_to_date")
                }
            }
        }
        invoke.resolve(result)
    }

    @Command
    fun install(invoke: Invoke) = execute(invoke) {
        val args = invoke.parseArgs(AppUpdateArgs::class.java)
        val candidate = candidates[args.channel]
        require(candidate != null && candidate.versionCode == args.versionCode && candidate.isNewerThan(installedCode)) { "stale_update" }
        if (!host.packageManager.canRequestPackageInstalls()) {
            host.runOnUiThread {
                try {
                    host.startActivity(Intent(Settings.ACTION_MANAGE_UNKNOWN_APP_SOURCES, Uri.parse("package:${host.packageName}")))
                    invoke.resolve(JSObject().apply { put("status", "permission_required") })
                } catch (_: Exception) { invoke.reject("installer_unavailable") }
            }
            return@execute
        }
        val directory = File(host.cacheDir, "updates").apply { mkdirs() }
        val apk = File(directory, "update.apk")
        try {
            download(candidate, apk)
            verifyPackage(apk, candidate)
            host.runOnUiThread {
                try {
                    val uri = FileProvider.getUriForFile(host, "${host.packageName}.fileprovider", apk)
                    host.startActivity(Intent(Intent.ACTION_VIEW).apply {
                        setDataAndType(uri, "application/vnd.android.package-archive")
                        addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
                    })
                    invoke.resolve(JSObject().apply { put("status", "installer_opened") })
                } catch (_: Exception) { invoke.reject("installer_unavailable") }
            }
        } catch (error: Exception) {
            apk.delete()
            throw error
        }
    }

    private fun download(candidate: UpdateCandidate, destination: File) {
        val connection = connect(candidate.url)
        try {
            require(connection.responseCode == 200) { "http_error" }
            val digest = MessageDigest.getInstance("SHA-256")
            var size = 0L
            connection.inputStream.use { input ->
                destination.outputStream().use { output ->
                    val buffer = ByteArray(64 * 1024)
                    while (true) {
                        val count = input.read(buffer)
                        if (count < 0) break
                        size += count
                        require(size <= candidate.size) { "download_changed" }
                        digest.update(buffer, 0, count)
                        output.write(buffer, 0, count)
                    }
                }
            }
            val hash = digest.digest().joinToString("") { "%02x".format(it) }
            require(size == candidate.size && hash == candidate.sha256) { "download_changed" }
        } finally { connection.disconnect() }
    }

    @Suppress("DEPRECATION")
    private fun verifyPackage(apk: File, candidate: UpdateCandidate) {
        val manager = host.packageManager
        val flags = if (Build.VERSION.SDK_INT >= 28) PackageManager.GET_SIGNING_CERTIFICATES else PackageManager.GET_SIGNATURES
        val archive = manager.getPackageArchiveInfo(apk.absolutePath, flags)
        require(archive != null && archive.packageName == host.packageName &&
            versionCode(archive) == candidate.versionCode && archive.versionName == candidate.version) { "wrong_package" }
        val installed = manager.getPackageInfo(host.packageName, flags)
        fun signers(info: PackageInfo): Set<String> {
            val signatures = if (Build.VERSION.SDK_INT >= 28) info.signingInfo?.apkContentsSigners else info.signatures
            return signatures?.map { signature ->
                MessageDigest.getInstance("SHA-256").digest(signature.toByteArray()).joinToString("") { "%02x".format(it) }
            }?.toSet() ?: emptySet()
        }
        val expected = signers(installed)
        require(expected.isNotEmpty() && expected == signers(archive)) { "signature_mismatch" }
    }

    private fun readJson(url: String, missingAllowed: Boolean = false): JSONObject? {
        val connection = connect(url)
        try {
            if (missingAllowed && connection.responseCode == 404) return null
            require(connection.responseCode != 403 && connection.responseCode != 429) { "rate_limited" }
            require(connection.responseCode == 200) { "http_error" }
            val bytes = connection.inputStream.use { it.readBytesBounded(1024 * 1024) }
            return JSONObject(bytes.toString(Charsets.UTF_8))
        } finally { connection.disconnect() }
    }

    private fun java.io.InputStream.readBytesBounded(limit: Int): ByteArray {
        val output = java.io.ByteArrayOutputStream()
        val buffer = ByteArray(8192)
        while (true) {
            val count = read(buffer)
            if (count < 0) break
            require(output.size() + count <= limit) { "invalid_metadata" }
            output.write(buffer, 0, count)
        }
        return output.toByteArray()
    }

    private fun connect(address: String): HttpsURLConnection {
        var url = URL(address)
        repeat(6) {
            require(url.protocol == "https" && url.userInfo == null) { "invalid_metadata" }
            val connection = url.openConnection() as HttpsURLConnection
            connection.connectTimeout = 15_000
            connection.readTimeout = 30_000
            connection.instanceFollowRedirects = false
            connection.useCaches = false
            connection.setRequestProperty("User-Agent", "iDroidLoader/${BuildConfig.VERSION_NAME}")
            connection.setRequestProperty("Accept", "application/vnd.github+json")
            if (connection.responseCode in setOf(301, 302, 303, 307, 308)) {
                val location = connection.getHeaderField("Location")
                connection.disconnect()
                require(!location.isNullOrBlank()) { "http_error" }
                url = URL(url, location)
            } else return connection
        }
        error("http_error")
    }

    @Suppress("DEPRECATION")
    private fun versionCode(info: PackageInfo): Long =
        if (Build.VERSION.SDK_INT >= 28) info.longVersionCode else info.versionCode.toLong()
}
