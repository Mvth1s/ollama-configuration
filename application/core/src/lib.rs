//! Pure, tauri-free logic for the SelfLlama `application/` workspace -
//! system detection (GPU/RAM/CPU/distro) and ANSI-aware output streaming,
//! ported from `scripts/linux/`, `scripts/windows/lib/common.ps1`, and
//! `gui/src-tauri/src/main.rs`.
//!
//! Same convention as `launcher/launcher-core`: no `tauri`, no process
//! spawning, no filesystem access. Every function takes the text a real
//! command would print (or `None`/a list, for the cases with no text
//! output) and returns a parsed result, so `cargo test` here never needs
//! real hardware, a specific GPU vendor, or the WebKitGTK/GTK3 dev packages
//! `gui`/`launcher`'s own crates require to compile.
//!
//! Not yet wired into `gui/` or `launcher/` - see CLAUDE.md's
//! `application/core` section for the current scope and what remains for a
//! later phase.

pub mod detect;
pub mod progress;
