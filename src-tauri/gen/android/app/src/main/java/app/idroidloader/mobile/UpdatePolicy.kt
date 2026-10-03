package app.idroidloader.mobile

import java.net.URI

/** Shared by both channels; Android only accepts builds newer than the installed version code. */
internal data class UpdateCandidate(
    val channel: String,
    val version: String,
    val versionCode: Long,
    val sha256: String,
    val size: Long,
    val url: String,
) {
    init {
        require(channel == "release" || channel == "nightly") { "invalid_metadata" }
        require(version.isNotBlank() && version.length <= 100) { "invalid_metadata" }
        require(versionCode in 1..2_100_000_000L) { "invalid_metadata" }
        require(sha256.matches(Regex("[a-f0-9]{64}"))) { "invalid_metadata" }
        require(size in 1..MAX_APK_BYTES) { "invalid_metadata" }
        require(isReleaseAsset(url, "iDroidLoader-arm64.apk")) { "invalid_metadata" }
    }

    fun isNewerThan(installed: Long) = versionCode > installed

    companion object {
        const val REPOSITORY = "AvianJay/idroidloader"
        const val MAX_APK_BYTES = 512L * 1024 * 1024

        fun isReleaseAsset(url: String, name: String): Boolean = runCatching {
            val uri = URI(url)
            uri.scheme == "https" && uri.host == "github.com" && uri.port == -1 &&
                uri.rawUserInfo == null && uri.rawQuery == null && uri.rawFragment == null &&
                uri.rawPath.matches(Regex("/$REPOSITORY/releases/download/[^/]+/${Regex.escape(name)}")) &&
                !uri.rawPath.contains("%") && !uri.rawPath.contains("/../")
        }.getOrDefault(false)
    }
}
