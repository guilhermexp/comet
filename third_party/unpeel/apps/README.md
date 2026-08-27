# apps/

Everything user-facing that isn't the Rust session backend (`crates/`). Each
app has its own README; the open/closed status refers to the boundary decided
in `docs/plans/open-source.md`.

| app | what it is | status |
| --- | --- | --- |
| `native/` | The macOS app (Swift + libghostty) and the `unpeel-attach` terminal client | open |
| `ios/` | The iPhone/iPad app — a remote controller for Hosts; live on TestFlight | open |
| `shared/` | `UnpeelShared` Swift package: pairing, remote-control, and Relay E2E protocol code shared by the Mac and iOS apps | open |
| `website/` | unpeel.com — marketing site, docs, purchase UI, and the worker shell that mounts the closed account service | open (shell) |
| `relay/` | The Unpeel Link relay worker: E2E-opaque off-LAN transport + APNs push | open (code); the operated service is the paid product |
| `releases/` | Standalone worker serving downloads, Sparkle appcasts, and the CLI installer from R2 | open |

Not here: the account/licensing/Link service backend lives in the **private**
sibling repo `~/Dev/unpeel-account` (`unpeel-com/unpeel-account`), consumed by
`website/` as the `@unpeel/account-service` `file:` dependency.
