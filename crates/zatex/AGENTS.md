# zatex

Parent: [../AGENTS.md](../AGENTS.md)

## Purpose

UI-free TeX parser/layout emitting flat glyph/rule display lists over OpenType MATH fonts.

## Ownership

Owns this crate's public model and implementation. Hosts own product controls and platform painting.

## Local Contracts

Bound source, expansion/work/memory and malformed input; unsupported input fails for host source fallback. The bundled feature includes six unmodified fonts with licenses beside them and in THIRD_PARTY_NOTICES.md.

## Work Guidance

Preserve final pinned upstream fixes and existing regressions. Keep workspace dependency versions at the root.

## Verification

| Layer / path | Tier | Command |
| --- | --- | --- |
| `src/**` | unit | `cargo test -p zatex --features bundled` |
| Native integration | e2e — actual desktop/iOS app; Linux GPU tests require a software or hardware adapter | Isolated native smoke; no compilation-only acceptance |

## Child DOX Index

No child docs.
