# Workers Provider Icons — Design

## Goal

Show each Worker's monochrome provider SVG consistently in the native new-session menu and the Presets settings list, including dedicated OMP and prime-agent marks derived from their installed CLI identities.

## Visual direction

- Reuse the existing monochrome provider assets for Amp, Claude, Cline, Codex, Cursor, Gemini, Grok, Kimi, Kiro, Muse, OpenCode, Pi and the intentional Copilot fallback.
- Add `omp.svg`, derived from OMP's five-line block π mark.
- Add `prime-agent.svg`, derived from prime-agent's rising diagonal/wing terminal mark.
- Keep both new assets single-color and theme-tintable, matching the existing Workers SVG pipeline.

## Architecture

`runtime_icon_path(runtime_id, command)` remains the only provider-to-asset resolver. Preset rows must call it with the preset's `cli_id` and command instead of rendering `icons::TERMINAL`. The AppKit new-session menu already calls the resolver, so its embedded-byte table must include every returned provider asset, including OMP and prime-agent.

## Testing

- Extend the resolver test so OMP and prime-agent resolve to dedicated constants, never the generic-agent asset.
- Add a coverage test ensuring the native menu byte table recognizes every icon returned for the built-in runtime catalog.
- Run focused Workers presentation/new-session-menu tests, then the complete Workers test slice and `cargo check -p zeron-ui --no-default-features`.

## Done criteria

- Presets rows show provider SVGs rather than the generic terminal icon.
- The native `+` menu shows provider SVGs for every enabled built-in preset.
- OMP and prime-agent have distinct monochrome SVG assets based on their CLI marks.
- Unknown/custom commands retain the terminal fallback.
