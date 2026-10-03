# iDroidLoader Android port

Community Android port of [iloader](https://github.com/nab138/iloader). This is not an official upstream release.

## Connect and install

For **iOS 27 or newer**, initial pairing can start directly over Wi-Fi. This follows the device-initiated RemotePairing flow documented by [idevice_pair](https://github.com/jkcoxson/idevice_pair#over-wi-fi-with-iphone-or-ipad) and used by [SideStore's wireless pairing interface](https://github.com/SideStore/SideStore/tree/develop/SideStore/Views/Settings/Advanced/PairingFile/WirelessPair).

1. Enable Developer Mode on iPhone, keep it unlocked, and connect both phones to the same Wi-Fi. The network must allow mDNS and communication between clients.
2. In iDroidLoader, tap **Pair wirelessly**. On iPhone, open **Settings > Privacy & Security > Developer Mode** and select **iDroidLoader** among the pairing hosts.
3. Enter the six-digit code displayed on Android into iPhone. Keep iDroidLoader open while it establishes the authenticated tunnel. Cancel or retry if pairing expires.
4. Once the device name appears, install an already-signed IPA, or sign in with Apple ID to sign and install. **Export pairing** saves the RemotePairing record if you want to reconnect after restarting.

This creates a **RemotePairing (RPPairing)** record. It does not create Lockdown certificates. The app uses an authenticated TLS-PSK CDTunnel and a userspace TCP adapter to reach RSD services; it needs no root access or Android VPN permission. Android enables Wi-Fi multicast reception and keeps the screen awake during pairing/discovery, releasing both when the attempt ends. Pairing codes and private protocol messages are excluded from application logs.

For earlier iOS versions, use the existing pairing-file connection:

1. Prepare a trusted **Lockdown** pairing file for your iPhone using a computer. iloader's combined pairing export also works. Enable Wi-Fi debugging before unplugging the iPhone.
2. Put Android and iPhone on the same Wi-Fi network, with client isolation disabled. Find the iPhone's address in Settings > Wi-Fi > the connected network.
3. Open iDroidLoader, choose **Import pairing file**, enter the IP address, then tap **Connect to iPhone**. The device name and iOS version appear after an authenticated Lockdown session succeeds.
4. Use **Install already-signed IPA** for an IPA with valid signing and provisioning for this iPhone. To sign an IPA or install SideStore / LiveContainer, sign in with Apple ID and complete two-factor authentication first. Files selected through Android document providers are streamed into private app cache as needed.

You do not need a computer during subsequent installations while the pairing remains trusted and the iPhone remains reachable. Pairing does not replace signing credentials or iOS Developer Mode requirements.

On Android, check **Save credentials** when signing in to remember an Apple ID and password. After restarting, select the account under **Saved logins** and tap **Sign in**. Apple may still request two-factor authentication. **Delete** removes its saved password; signing out ends the current session without deleting saved credentials.

Passwords are saved only after a successful login and only when you opt in. Passwords, anisette state, and signing certificates use AES-256-GCM authenticated encryption with a non-exportable Android Keystore key. The private preferences contain ciphertext; app backup is disabled. Passwords are never written to the frontend settings store. If Keystore is unavailable, password saving is disabled and signing data stays in memory. Clearing app data or uninstalling removes saved accounts.

Pairing credentials remain in process memory; pair again or reimport an exported pairing file after restarting. With a RemotePairing-only file, enter the iPhone's current IP address and keep both phones on the same Wi-Fi so authenticated mDNS discovery can find its current pairing port. RemotePairing records created through a computer can also be imported when the device exposes that service. Imported temporary IPA copies are deleted when the operation finishes.

Cross-network use needs routed connectivity and, for RemotePairing, service discovery that reaches the device. Imported records are never printed in logs or returned to the frontend. Pairing credentials grant access to your device; keep exported files private.

## Build on Windows

Install Node.js, Rust (MSVC), Visual Studio C++ Build Tools, Android Studio, Android SDK, and NDK. The SDK and JDK are selected by `scripts/android.ps1`; existing `ANDROID_HOME`, `NDK_HOME`, and `JAVA_HOME` take precedence.

```powershell
npm ci
rustup target add aarch64-linux-android
powershell -ExecutionPolicy Bypass -File scripts/android.ps1 Init
powershell -ExecutionPolicy Bypass -File scripts/android.ps1 Build
```

The installable debug APK is copied to `artifacts/iDroidLoader-arm64-debug.apk`. It targets 64-bit ARM devices on Android 8.0 or newer. Debug signing is for development; production distribution needs a release signing configuration.

## Updates: Release and Nightly

In **Settings > App updates**, choose **Release** for stable versions or **Nightly** for development builds. The selection is saved and checked when the app starts; **Check for updates** checks it again. New installations default to the channel they were built from.

Tap **Download and install** to update. If Android asks, allow iDroidLoader to install apps, return to the app, and tap the button again. The APK is downloaded into private cache, checked against its SHA-256 checksum, package name, version, and installed signing certificate, then handed to Android's installer for confirmation. Existing app data is retained. A debug APK cannot update to a release APK signed with a different key.

Both channels share an increasing Android version code. Switching channels only offers a newer build; switching from Nightly to Release may require waiting for the next stable release. If a channel has no Android release with updater metadata yet, the app says so.

## Signed release builds

The **Android Builds** workflow publishes a stable release when a `vX.Y.Z` tag is pushed. Bump the app version to match the tag before tagging. The packaged version must match the tag or publication fails. Stable builds use the same signing secrets as Nightly and publish an APK, checksum, and `android-update.json` to that tagged GitHub release.

## Signed nightly builds

Download the signed APK from the single [Nightly release](https://github.com/AvianJay/idroidloader/releases/tag/nightly). The Android Builds workflow runs on main pushes, daily at 18:00 UTC (02:00 Taiwan time), or manually from Actions. Each successful nightly build replaces the APK, SHA-256 checksum, and updater metadata on that same release and moves the `nightly` tag to the built commit. Both channels use `100000000 + github.run_number` from this workflow for Android's version code, preserving the previous nightly sequence. Keep this workflow's run-number sequence when maintaining the release pipeline.

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
- Passed 21 idevice transport tests and 13 application Rust tests, including a correct and incorrect PIN exchange, rejection of invalid TLS-PSK Finished proofs, authenticated discovery filtering, dual-stack listeners, incomplete pairing records, cancellation ordering, and a mobile future-size budget.
- Passed 7 mobile browser UI tests, covering saved logins, imported pairing, wireless pairing progress, PIN clearing, cancellation, retry, and preserving an existing selection. Browser tests use mocked account/device responses; they do not validate Apple authentication or iPhone communication.
- Passed an Android wireless smoke test through the packaged interface and native backend: an actual mDNS host announcement, screen-awake acquisition, cancellation, and resource release. It does not simulate a paired iPhone.
- Passed 5 Android instrumentation checks using isolated public fixtures: encrypted writes/overwrites, deletion, ciphertext tampering/record substitution, missing encryption keys, and retrieval in a new process after a forced stop.
- Confirmed the running debug APK reports Keystore availability, displays the unchecked **Save credentials** option, and rejects direct frontend access to native stored values.
- Reproduced 4 Anisette failures with the old verifier under release HTTP restrictions, then passed all 9 Android checks with the updated verifier and production network policy: Apple's lookup, the Android system verifier, expired certificate rejection, v3 POST responses from SideStore `.app` and `.io`, fresh in-memory provisioning followed by real v3 headers from `.app`, and 3 CRL/application HTTP policy checks. These checks use no account credentials. Public service probes cannot guarantee availability on every network.
- Passed 2 Anisette Rust regression tests: a dropped POST retries with the same payload/device identity and normalized URL, and a final transport error retains its underlying cause while removing URL credentials/tokens. These checks also run in the nightly workflow.

The vendored isideload 0.4.0 has Android TLS, Anisette request/error-handling, and RSD installation patches, documented in `src-tauri/vendor/isideload/PATCHES.md`. The MIT-licensed idevice 0.1.68 transport is vendored with strict TLS-PSK Finished validation and tunnel frame bounds checking; see `src-tauri/vendor/idevice/PATCHES.md`. GrandSlam uses an explicit Mozilla + Apple root store because Android's platform verifier cannot merge extra root certificates. Other HTTP clients use the initialized Android platform verifier; certificate and hostname checks remain enabled.

Android release builds use rustls-platform-verifier 0.7.1 and its matching Android component 0.2.0 from the official Maven archive. The component supplies a restricted network security configuration allowing HTTP only for certificate revocation list (CRL) hosts. Previously, the release policy blocked these downloads and Android reported valid Let's Encrypt certificates as `InvalidCertificate(Revoked)`. The [upstream fix](https://github.com/rustls/rustls-platform-verifier/pull/253) preserves certificate and revocation checks. Application HTTP remains blocked in release builds; ordinary debug builds allow local Vite development.

To exercise release networking with debug-only JNI probes, build with the production network policy and run the Android instrumentation tests:

```powershell
$env:ORG_GRADLE_PROJECT_idroidReleaseNetworkPolicy = 'true'
powershell -ExecutionPolicy Bypass -File scripts/android.ps1 Build
# From src-tauri/gen/android, with the same Cargo/JDK/SDK environment:
.\gradlew.bat :app:assembleUniversalDebugAndroidTest -x :app:rustBuildUniversalDebug
# Install the debug app and AndroidTest APK on an isolated emulator first.
adb shell am instrument -w -e class app.idroidloader.mobile.TlsSmokeTest,app.idroidloader.mobile.NetworkPolicyTest app.idroidloader.mobile.test/androidx.test.runner.AndroidJUnitRunner
adb shell am instrument -w -e class app.idroidloader.mobile.WirelessPairSmokeTest app.idroidloader.mobile.test/androidx.test.runner.AndroidJUnitRunner
Remove-Item Env:ORG_GRADLE_PROJECT_idroidReleaseNetworkPolicy
```

`NetworkPolicyTest` verifies CRL HTTP is allowed while application HTTP and lookalike CRL domains remain blocked. These probes do not use Apple ID credentials or save provisioning data.

Anisette continues to use `/v3/provisioning_session` and POST `/v3/get_headers` with the stored device identity. The root GET endpoint serves the shared v1 identity and is not used as a fallback. Native header requests have a 30-second timeout per attempt and retry transport failures once; HTTP/API errors are not retried. If a request still fails, its TLS/DNS/connection cause is shown without logging the provisioning payload. See the [SideStore protocol guidance](https://docs.sidestore.io/docs/advanced/anisette).

No physical iPhone or valid pairing credentials were available for interoperability testing. Real-device checks still need to cover iOS 27 host discovery and full PIN onboarding, tunnel reconnection, revoked pairing, signed IPA installation, Apple ID/2FA signing, and SideStore pairing placement. Wireless pairing is experimental until those checks have been completed.
