# iDroidLoader transport patches

Based on the MIT-licensed idevice 0.1.68 crate from <https://github.com/jkcoxson/idevice>.

- Reject TLS-PSK server Finished proof mismatches and invalid framing instead of continuing the handshake. The iOS 27 wireless transport uses this pure-Rust PSK implementation.
- Bounds-check the declared CDTunnel response length, returning an error for truncated frames instead of panicking.
- Omit the example-only `tun-rs` dev dependency from the vendored crate; the port uses the userspace TCP adapter and does not require an OS TUN device.

These changes affect the RemotePairing tunnel only. RemotePairing credentials and plaintext protocol messages are excluded from both application log outputs by the application's tracing filters.
