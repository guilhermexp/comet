# Palette integration plan and evidence

## Scope

This slice adapts upstream commits `1400f7ab`, `74558a2c`, `48950003`,
`76dc6a46`, `12b03287`, and `9a4757be` for task 2.1. The fork already has a
native Add Space flow, Workers navigation, remote Files, model pickers, and
compaction/theme changes that must remain authoritative.

## Findings before editing

- `crates/ui/src/shell.rs` binds `mod-k` to `AddSpacePalette` and has no global
  command-palette state or module.
- The fork's `shell/spaces.rs` owns a larger device/project browser than the
  upstream snapshot; replacing it wholesale would remove Workers and remote
  Files behavior.
- `popover.rs` already provides popup lifecycle, glass surfaces, keyboard
  navigation, and picker scroll primitives. The palette should build on those
  primitives rather than reintroducing a second popup implementation.
- `pickers.rs` contains the fork's combined harness/model menu and jump-slot
  routing. The nested model menu fixes must be adapted locally and must not
  replace the fork's catalog, favorites, or target-device rules.

## Implementation plan

1. Add a shell-owned `command_palette` module with action entries and global
   Chat search. Search metadata includes Chat title, project, device, branch,
   and pull request; activation routes through existing Shell actions.
2. Make the palette an overlay that owns focus and keyboard navigation, and
   namespace Chat-row ids/hover state so a palette row cannot mutate the
   sidebar row. Add query highlighting through the existing popup/theme roles.
3. Rebind `mod-k` to the command palette and keep Add Space reachable from the
   palette's New project action. Add a persisted `NewProject` shortcut with
   migration-safe serde defaults and the Settings shortcut group.
4. Port only compatible popup geometry/scroll and nested-menu behavior from
   the upstream menu fixes. Preserve the fork's frost, spacing, native Files,
   Workers, and model-picker contracts.
5. Verify pure reducers and targeted UI tests, then run `cargo check -p
   zeron-ui --message-format short`. Native rendering remains a QA step for
   the parent integration pass.

## Evidence

Implemented in the fork's existing UI seams:

- `crates/ui/src/shell/command_palette.rs` adds Cmd+K action and Chat history
  search, metadata matching, focus restoration, keyboard navigation, shortcut
  badges, scroll gutters, query highlights, and isolated Chat-row ids.
- `crates/ui/src/shell.rs` registers the overlay/action, routes `mod-k`, and
  keeps overlay keyboard ownership isolated from the sidebar.
- `crates/ui/src/settings.rs` and `settings/shortcuts.rs` add the migration-safe
  `NewProject` shortcut; `shell/spaces.rs` keeps the existing native Add Space,
  Workers, and Files flow and exposes only its existing sort comparator.
- `crates/ui/src/popover.rs` and `pickers.rs` share menu geometry/glass roles,
  nested model-setting menus, and pointer-corridor dismissal while preserving
  the fork's catalog/favorites behavior. `change_requests.rs` highlights PR
  numbers when a Chat result is queried.

Validation so far:

- `git diff --check` passes for the palette slice.
- `rustfmt --check` passes for the new/edited `command_palette.rs`,
  `pickers.rs`, and `popover.rs`.
- `cargo test -p zeron-ui --lib --no-fail-fast` passes with 1,393 tests and no
  failures, including the palette and image match-arm integration.
- Native QA passes Cmd+K open/filter by Chat title and Return navigation to the
  selected Chat, plus the New chat action.
