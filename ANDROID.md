# iDroidLoader Android port

Community Android port of [iloader](https://github.com/nab138/iloader). This is not an official upstream release.

## Connect and install

1. Prepare a trusted **Lockdown** pairing file for your iPhone using a computer. iloader's combined pairing export also works. Enable Wi-Fi debugging before unplugging the iPhone.
2. Put Android and iPhone on the same Wi-Fi network, with client isolation disabled. Find the iPhone's address in Settings > Wi-Fi > the connected network.
3. Open iDroidLoader, choose **Import pairing file**, enter the IP address, then tap **Connect to iPhone**. The device name and iOS version appear after an authenticated Lockdown session succeeds.
4. Use **Install already-signed IPA** for an IPA with valid signing and provisioning for this iPhone. To sign an IPA or install SideStore / LiveContainer, sign in with Apple ID and complete two-factor authentication first. Files selected through Android document providers are streamed into private app cache as needed.

You do not need a computer during subsequent installations while the pairing remains trusted and the iPhone remains reachable. Pairing does not replace signing credentials or iOS Developer Mode requirements.

On Android, check **Save credentials** when signing in to remember an Apple ID and password. After restarting, select the account under **Saved logins** and tap **Sign in**. Apple may still request two-factor authentication. **Delete** removes its saved password; signing out ends the current session without deleting saved credentials.

Passwords are saved only after a successful login and only when you opt in. Passwords, anisette state, and signing certificates use AES-256-GCM authenticated encryption with a non-exportable Android Keystore key. The private preferences contain ciphertext; app backup is disabled. Passwords are never written to the frontend settings store. If Keystore is unavailable, password saving is disabled and signing data stays in memory. Clearing app data or uninstalling removes saved accounts.

Pairing credentials remain in process memory; reimport the pairing file after restarting. Imported temporary IPA copies are deleted when the operation finishes.

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
- Passed all 6 Rust unit tests and 4 mobile browser UI tests, including opt-in saved login, reopening, signing out, and deletion. Browser tests use mocked account/device responses; they do not validate Apple authentication or iPhone communication.
- Passed 5 Android instrumentation checks using isolated public fixtures: encrypted writes/overwrites, deletion, ciphertext tampering/record substitution, missing encryption keys, and retrieval in a new process after a forced stop.
- Confirmed the running debug APK reports Keystore availability, displays the unchecked **Save credentials** option, and rejects direct frontend access to native stored values.
- Reproduced 4 Anisette failures with the old verifier under release HTTP restrictions, then passed all 9 Android checks with the updated verifier and production network policy: Apple's lookup, the Android system verifier, expired certificate rejection, v3 POST responses from SideStore `.app` and `.io`, fresh in-memory provisioning followed by real v3 headers from `.app`, and 3 CRL/application HTTP policy checks. These checks use no account credentials. Public service probes cannot guarantee availability on every network.
- Passed 2 Anisette Rust regression tests: a dropped POST retries with the same payload/device identity and normalized URL, and a final transport error retains its underlying cause while removing URL credentials/tokens. These checks also run in the nightly workflow.

The vendored isideload 0.4.0 has an Android TLS patch and Anisette request/error-handling patches, documented in `src-tauri/vendor/isideload/PATCHES.md`. GrandSlam uses an explicit Mozilla + Apple root store because Android's platform verifier cannot merge extra root certificates. Other HTTP clients use the initialized Android platform verifier; certificate and hostname checks remain enabled.

Android release builds use rustls-platform-verifier 0.7.1 and its matching Android component 0.2.0 from the official Maven archive. The component supplies a restricted network security configuration allowing HTTP only for certificate revocation list (CRL) hosts. Previously, the release policy blocked these downloads and Android reported valid Let's Encrypt certificates as `InvalidCertificate(Revoked)`. The [upstream fix](https://github.com/rustls/rustls-platform-verifier/pull/253) preserves certificate and revocation checks. Application HTTP remains blocked in release builds; ordinary debug builds allow local Vite development.

To exercise release networking with debug-only JNI probes, build with the production network policy and run the Android instrumentation tests:

```powershell
$env:ORG_GRADLE_PROJECT_idroidReleaseNetworkPolicy = 'true'
powershell -ExecutionPolicy Bypass -File scripts/android.ps1 Build
# From src-tauri/gen/android, with the same Cargo/JDK/SDK environment:
.\gradlew.bat :app:assembleUniversalDebugAndroidTest -x :app:rustBuildUniversalDebug
# Install the debug app and AndroidTest APK on an isolated emulator first.
adb shell am instrument -w -e class app.idroidloader.mobile.TlsSmokeTest,app.idroidloader.mobile.NetworkPolicyTest app.idroidloader.mobile.test/androidx.test.runner.AndroidJUnitRunner
Remove-Item Env:ORG_GRADLE_PROJECT_idroidReleaseNetworkPolicy
```

`NetworkPolicyTest` verifies CRL HTTP is allowed while application HTTP and lookalike CRL domains remain blocked. These probes do not use Apple ID credentials or save provisioning data.

Anisette continues to use `/v3/provisioning_session` and POST `/v3/get_headers` with the stored device identity. The root GET endpoint serves the shared v1 identity and is not used as a fallback. Native header requests have a 30-second timeout per attempt and retry transport failures once; HTTP/API errors are not retried. If a request still fails, its TLS/DNS/connection cause is shown without logging the provisioning payload. See the [SideStore protocol guidance](https://docs.sidestore.io/docs/advanced/anisette).

No physical iPhone or valid pairing credentials were available for interoperability testing. Real-device checks still need to cover successful connection, revoked pairing, unreachable IP, signed IPA installation, Apple ID/2FA signing, and SideStore pairing placement.
