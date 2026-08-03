//! Phase 4 of the SelfLlama restructure: adds `privileged`, the
//! single-elevation-prompt privileged install phase (see
//! `privileged`'s module doc comment, in `src/lib.rs`, for the full
//! design). No Tauri app is wired up yet - see CLAUDE.md's
//! `application/src-tauri` section for why, and for what remains before
//! `gui/`'s three separate `pkexec` calls can actually be replaced by this.

use selfllama_installer::privileged;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();

    if args.first().map(String::as_str) == Some(privileged::RUN_PRIVILEGED_PHASE_ARG) {
        std::process::exit(privileged::run_privileged_worker(&args[1..]));
    }

    eprintln!(
        "selfllama-installer: no GUI wired up yet (this phase only adds the privileged-phase \
         worker entry point, invoked via {}; see CLAUDE.md's application/src-tauri section)",
        privileged::RUN_PRIVILEGED_PHASE_ARG
    );
    std::process::exit(1);
}
