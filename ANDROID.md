# iDroidLoader Android port

Community Android port of [iloader](https://github.com/nab138/iloader). This is not an official upstream release.

## Connect and install

1. Prepare a trusted **Lockdown** pairing file for your iPhone using a computer. iloader's combined pairing export also works. Enable Wi-Fi debugging before unplugging the iPhone.
2. Put Android and iPhone on the same Wi-Fi network, with client isolation disabled. Find the iPhone's address in Settings > Wi-Fi > the connected network.
3. Open iDroidLoader, choose **Import pairing file**, enter the IP address, then tap **Connect to iPhone**. The device name and iOS version appear after an authenticated Lockdown session succeeds.
4. Use **Install already-signed IPA** for an IPA with valid signing and provisioning for this iPhone. To sign an IPA or install SideStore / LiveContainer, sign in with Apple ID and complete two-factor authentication first. Files selected through Android document providers are streamed into private app cache as needed.

You do not need a computer during subsequent installations while the pairing remains trusted and the iPhone remains reachable. Pairing does not replace signing credentials or iOS Developer Mode requirements.

Pairing credentials and mobile signing/authentication storage remain in process memory. Reimport the pairing file and sign in after restarting the app. Password saving is disabled on Android pending a native Keystore backend. Imported temporary IPA copies are deleted when the operation finishes.

RemotePairing-only files are detected and rejected with an explanation. The first version uses Lockdown over TCP; a RemotePairing/RSD network connection is not implemented. Cross-network use needs a routed VPN or equivalent connectivity. Imported records are never printed in logs or returned to the frontend.

## Build on Windows

Install Node.js, Rust (MSVC), Visual Studio C++ Build Tools, Android Studio, Android SDK, and NDK. The SDK and JDK are selected by `scripts/android.ps1`; existing `ANDROID_HOME`, `NDK_HOME`, and `JAVA_HOME` take precedence.

```powershell
npm ci
rustup target add aarch64-linux-android
powershell -ExecutionPolicy Bypass -File scripts/android.ps1 Init
powershell -ExecutionPolicy Bypass -File scripts/android.ps1 Build
```

The installable debug APK is copied to `artifacts/iDroidLoader-arm64-debug.apk`. It targets 64-bit ARM devices on Android 8.0 or newer. Debug signing is for development; production distribution needs a release signing configuration.

## Signed nightly builds

Download the signed APK from the single [Nightly release](https://github.com/AvianJay/idroidloader/releases/tag/nightly). The Android Nightly workflow runs on main pushes, daily at 18:00 UTC (02:00 Taiwan time), or manually from Actions. Each successful build replaces the APK and SHA-256 checksum on that same release, moves the `nightly` tag to the built commit, and increases Android's version code so future nightly APKs can update in place.

The repository needs these GitHub Actions secrets:

- `KEYSTORE_BASE64`: the signing keystore encoded in base64.
- `KEYSTORE_ALIAS`: the signing key alias.
- `KEYSTORE_PASSWORD`: used for both the keystore and the signing key password.

The workflow decodes the keystore into the runner's temporary directory, uses environment variables for signing, verifies the APK signature, and removes the temporary keystore after the build. Signing files and passwords are never committed. Gradle configuration caching is disabled so it cannot serialize signing credentials. Pull requests build with Android's debug key and do not receive the release secrets. The inherited desktop and download-badge workflows only run in the upstream repository.

When switching from the locally built debug APK to the signed nightly APK, uninstall the debug app once because the signing certificates differ. Keep the same keystore for subsequent nightly updates.

```powershell
npm run build
powershell -ExecutionPolicy Bypass -File scripts/android.ps1 Check
powershell -ExecutionPolicy Bypass -File scripts/android.ps1 Test
```

## Verified in this workspace (2026-10-01)

- Built the ARM64 debug APK, verified its Android signature, and confirmed it packages the ARM64 native library.
- Installed and cold-started the APK on an Android 16 emulator with ARM64 translation.
- Used the real Android document picker to import an incomplete plist. The native backend read the content URI and returned the expected missing-certificate error.
- Passed all 6 Rust unit tests and both mobile browser UI tests. Browser tests use mocked device responses; they do not validate iPhone communication.

No physical iPhone or valid pairing credentials were available for interoperability testing. Real-device checks still need to cover successful connection, revoked pairing, unreachable IP, signed IPA installation, Apple ID/2FA signing, and SideStore pairing placement.
