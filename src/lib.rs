//! Plattformunabhängige Kernlogik von EVE Multibox.
//!
//! Alles hier ist reines Rust ohne Win32-Aufrufe und kann deshalb unter
//! WSL/Linux mit `cargo test --lib` getestet werden.

pub mod config;
pub mod hotkey;
pub mod i18n;
pub mod icon;
pub mod layout;
