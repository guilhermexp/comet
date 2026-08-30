# Rust dependency patches

These are exact crates.io source snapshots with narrow compatibility patches.
Their package versions stay unchanged so every transitive dependency keeps the
same resolution contract.

| Crate | Source | License | Local patch |
|---|---|---|---|
| `block 0.1.6` | crates.io checksum `0d8c1fef690941d3e7788d328517591fecc684c084084702d6ff1641e993699a` | MIT | Model the private Objective-C block class symbol as opaque `c_void` instead of an uninhabited Rust enum, and make its existing C ABI explicit. |
| `proc-macro-error2 2.0.1` | crates.io checksum `11ec05c52be0a07b08061f7dd003e7d7092e0472bc731b4af7bb1ef876109802` | MIT OR Apache-2.0 | Make the `proc_macro` extern crate public because the crate's exported macros expose it. |
