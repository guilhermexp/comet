//! Portable UI SDK for Unpeel Apps.
//!
//! An App remains a normal Rust CLI that can render through Ratatui in any
//! terminal. Apps that build their view with [`portable`] can also serialize
//! the same semantic tree for a native Unpeel renderer (and, eventually, a
//! web renderer). The terminal adapter deliberately uses Ratatui itself, so
//! there is no second terminal widget implementation, layout solver, canvas
//! rasterizer, buffer, or backend.
//!
//! Existing style, key, fuzzy-match, status, and host helpers remain available
//! for terminal-first Apps. Raw Ratatui is re-exported as [`ratatui`] and is
//! always the escape hatch for terminal-only or custom widgets.

pub mod fuzzy;
pub mod host;
pub mod keys;
pub mod portable;
pub mod status;
pub mod style;

/// Convenient imports for authoring a portable Unpeel App view.
pub mod prelude {
    pub use crate::portable::prelude::*;
}

/// Re-exported so plugins compile against the exact Ratatui this crate was
/// built with (pin and re-export, don't leak version skew).
pub use ratatui;
