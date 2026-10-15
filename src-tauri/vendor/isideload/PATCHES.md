# isideload 0.4.0

Source: https://github.com/nab138/isideload (crates.io release 0.4.0).
Original metadata and license declaration are retained in Cargo.toml.

GrandSlam login backport: authentication and Anisette changes from upstream
through `8ca39b6203263e539408986e546c42e7c1cc3699` (2026-10-14), including
PR #26 and the fixes used by iloader 2.3.7/2.3.8. Client info comes from the
server, lookup retains the required macOS identity, hsc 434 triggers a single
Anisette resync, and the retried init and password proof share the refreshed
client-provided data. hsc 409 continues with the already-sent device 2FA code,
and failing to list SMS numbers does not block trusted-device authentication.
Invalid provisioning (-45061) is renewed with a bounded number of attempts.
The app migrates to the two upstream Windows Anisette servers using a new
preference key; SMS 2FA remains subject to upstream's current limitations.
This is a focused backport onto 0.4.0, not a full crate/dependency upgrade.

Android-only patch: GrandSlam uses reqwest's `tls_certs_only` with the Mozilla
root certificates supplied by `webpki-root-certs`, plus the existing Apple CA.
Reqwest's platform verifier cannot merge additional roots on Android and otherwise
fails before connecting. Certificate, hostname, and signature validation remain
enabled. All other targets retain the upstream TLS configuration.

Anisette v3 patch (all targets): normalize surrounding whitespace/trailing
slashes in base URLs, bound native HTTP requests to 30 seconds per attempt
(connect timeout 10 seconds), retry transport failures once with the same v3
payload/device identity, and unwrap reqwest middleware failures for the existing
error formatter, removing URLs that may contain credentials/tokens. A closed
provisioning stream now fails instead of spinning, and provisioning frames are
never logged because they contain ADI/key material. The v3 protocol and stored
device identity are retained; the root endpoint is the shared v1 API, not a
replacement for `/v3/get_headers`.

RemotePairing installation patch: expose `Sideloader::install_app_rsd` to sign
against the authenticated device identity and install through the existing RSD
installer. It retains device registration and signed-file cleanup behavior.
Legacy USB/Lockdown installation remains available.
