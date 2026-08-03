// Thin wrapper: everything else lives in src/lib.rs, so this crate has a
// [lib] target (see Cargo.toml) that tests/*.rs integration tests can
// depend on - a binary-only crate can't be, same reasoning
// application/src-tauri already applied.
//
// This is also where the single-pkexec mechanism's other half lives:
// selfllama_gui::run_privileged_phase (in lib.rs) re-invokes *this
// binary* under pkexec with selfllama_installer::privileged::
// RUN_PRIVILEGED_PHASE_ARG, exactly like selfllama-installer's own
// main.rs re-invokes itself. Without the check below, that re-invocation
// would try to launch a second full Tauri/GTK GUI as root instead of
// running the privileged worker code - checked first, before anything
// Tauri-related.
fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some(selfllama_installer::privileged::RUN_PRIVILEGED_PHASE_ARG) {
        std::process::exit(selfllama_installer::privileged::run_privileged_worker(&args[1..]));
    }

    selfllama_gui::run_tauri_app();
}
