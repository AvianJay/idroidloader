package app.idroidloader.mobile

import org.junit.Assert.*
import org.junit.Test

class UpdatePolicyTest {
    private val asset = "https://github.com/AvianJay/idroidloader/releases/download/nightly/iDroidLoader-arm64.apk"
    private fun candidate(channel: String = "nightly", code: Long = 100000123) =
        UpdateCandidate(channel, "2.3.4-nightly.123", code, "a".repeat(64), 42, asset)

    @Test fun channelsNeverOfferEqualOrOlderBuilds() {
        for (channel in listOf("release", "nightly")) {
            assertTrue(candidate(channel).isNewerThan(100000122))
            assertFalse(candidate(channel).isNewerThan(100000123))
            assertFalse(candidate(channel).isNewerThan(100000124))
        }
    }

    @Test fun onlyOfficialHttpsReleaseAssetsAreAccepted() {
        assertTrue(UpdateCandidate.isReleaseAsset(asset, "iDroidLoader-arm64.apk"))
        for (url in listOf(
            asset.replace("https:", "http:"),
            asset.replace("github.com", "github.com.evil.invalid"),
            asset.replace("AvianJay", "someone"),
            asset.replace("/nightly/", "/../"),
            asset.replace("/nightly/", "/%2e%2e/"),
            asset + "?redirect=elsewhere",
            asset.replace("github.com", "user@github.com"),
        )) assertFalse(url, UpdateCandidate.isReleaseAsset(url, "iDroidLoader-arm64.apk"))
    }

    @Test fun invalidMetadataCannotBecomeADownloadCandidate() {
        for (invalid in listOf<() -> Unit>(
            { candidate("unknown") },
            { candidate(code = 0) },
            { candidate().copy(sha256 = "not-a-checksum") },
            { candidate().copy(size = UpdateCandidate.MAX_APK_BYTES + 1) },
            { candidate().copy(url = "file:///data/local/tmp/update.apk") },
        )) {
            try { invalid(); fail("Invalid update accepted") } catch (_: IllegalArgumentException) { }
        }
    }
}
