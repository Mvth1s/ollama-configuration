//! Library half of `selfllama-installer`: exists so `application/src-tauri`
//! has a real public API surface (`privileged::spawn_privileged_phase` and
//! friends) that both `src/main.rs` and this crate's own `tests/`
//! integration tests can reach, rather than everything living private
//! inside a binary-only crate. `main.rs` is a thin wrapper over this.

pub mod privileged;
