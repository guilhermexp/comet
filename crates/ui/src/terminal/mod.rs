//! Terminal panel (feature-inventory §1.10): an `alacritty_terminal`-backed
//! emulator fed by the engine's PTY stream over the generic RPC client.
//!
//! - [`emulator`] — pure Term + vte state machine (bytes in, grid out);
//! - [`view`] — cell palette, keystroke→bytes encoding, input coalescing, and
//!   the custom grid-painting element;
//! - [`panel`] — session-scoped tabs, subscriptions with reconnect backoff,
//!   drag-reorder, and the Cmd/Ctrl+J toggle action.
//!
//! Method names come from `zeron_rpc::methods` and wire types from
//! `zeron_proto` (`TerminalSession`, `TerminalEvent`) — the same contract the
//! engine serves (feature-inventory §2.1).

// Upstream footer-terminal geometry (#620). The fork keeps the terminal in the
// right panel, so this stays dormant; kept so later syncs merge cleanly.
#[allow(dead_code)]
pub(crate) mod dock;
pub mod emulator;
pub mod panel;
pub mod scroll;
pub mod view;
