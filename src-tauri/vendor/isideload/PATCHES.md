# isideload 0.4.0

Source: https://github.com/nab138/isideload (crates.io release 0.4.0).
Original metadata and license declaration are retained in Cargo.toml.

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
