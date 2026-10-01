# isideload 0.4.0

Source: https://github.com/nab138/isideload (crates.io release 0.4.0).
Original metadata and license declaration are retained in Cargo.toml.

Android-only patch: GrandSlam uses reqwest's `tls_certs_only` with the Mozilla
root certificates supplied by `webpki-root-certs`, plus the existing Apple CA.
Reqwest's platform verifier cannot merge additional roots on Android and otherwise
fails before connecting. Certificate, hostname, and signature validation remain
enabled. All other targets retain the upstream TLS configuration.
