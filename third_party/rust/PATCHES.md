# Rust dependency patches

These are exact crates.io source snapshots with narrow compatibility patches.
Their package versions stay unchanged so every transitive dependency keeps the
same resolution contract.

| Crate | Source | License | Local patch |
|---|---|---|---|
| `block 0.1.6` | crates.io checksum `0d8c1fef690941d3e7788d328517591fecc684c084084702d6ff1641e993699a` | MIT | Model the private Objective-C block class symbol as opaque `c_void` instead of an uninhabited Rust enum, and make its existing C ABI explicit. |
| `proc-macro-error2 2.0.1` | crates.io checksum `11ec05c52be0a07b08061f7dd003e7d7092e0472bc731b4af7bb1ef876109802` | MIT OR Apache-2.0 | Make the `proc_macro` extern crate public because the crate's exported macros expose it. |
| `rtc-sctp 0.20.5` | crates.io checksum `2f786b8224032e25084a52401fcc28a8b6a4a5b1b1971152ad01113279628861` | MIT OR Apache-2.0 | Reduce the default outbound SCTP packet from 1228 to 1180 bytes so DTLS + UDP/IPv6 fits a 1280-byte VPN path. Existing real-peer preview transfer stalled with macOS `EMSGSIZE` on a tunnel; no public MTU setter reaches the SCTP endpoint through webrtc 0.20.5. Keep API/version and reliable stream framing unchanged. Remove when the transport exposes a safe path MTU or provides this fix. Regression: `cargo test --manifest-path third_party/rust/rtc-sctp-0.20.5/Cargo.toml comet_mtu_tests`, then `cargo test -p zeron-preview`. |
