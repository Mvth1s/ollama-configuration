// Orchestration only: this GUI never re-implements GPU/RAM/distro detection
// or model-tier selection. It locates the existing setup.sh / setup.ps1
// scripts next to the running executable, spawns them (splitting privileged
// from unprivileged steps on Linux, since a single pkexec over the whole
// setup.sh would run the unprivileged steps as root too and misplace
// per-user state such as ~/.config and the pipx install), and streams their
// stdout/stderr back to the frontend as Tauri events.

use selfllama_installer::privileged::protocol::{parse_step_event, ConfirmationRequest, StepStatus as PrivStepStatus};
use selfllama_installer::privileged::{spawn_privileged_phase, PhaseLine, PrivilegedPhaseOptions};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder};

const WEBUI_URL: &str = "http://127.0.0.1:8080";

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct InstallOptions {
    tier: Option<String>,
    skip_models: bool,
    skip_webui: bool,
    // Per-usage model overrides from the wizard's "Modeles" step, applied as
    // --model-<usage>=<name> flags to 03-pull-models.sh. Linux only: setup.ps1
    // has no per-usage override mechanism (no interactive picker on Windows
    // either, by existing design), so this is ignored in run_windows.
    #[serde(default)]
    models: Option<HashMap<String, String>>,
    // Set by the frontend only on a deliberate re-run after the user has
    // really answered the Nvidia driver confirmation dialog (see
    // run_linux's handling of PrivStepStatus::NeedsConfirmation below) -
    // false on every normal run, so a fresh install never silently assumes
    // consent. Linux only, same reasoning as `models`.
    #[serde(default)]
    confirm_nvidia_driver_install: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DetectOptions {
    tier: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ModelCandidate {
    model: String,
    desc: String,
}

// Raw shape matching the snake_case JSON the scripts emit as-is (bash/
// PowerShell field names), deserialized straight off the merged/parsed
// __DETECT__ line(s) before being converted to the camelCase DetectResult
// below for the frontend.
#[derive(Debug, Deserialize)]
struct DetectResultRaw {
    distro_pretty: String,
    gpu_vendor: String,
    gpu_name: String,
    cpu_model: String,
    cpu_threads: u32,
    ram_gb: u64,
    tier: String,
    tier_models: HashMap<String, String>,
    // candidates stays empty on Windows: no CAND_<TIER>_<usage> equivalent
    // exists there, matching the existing "no interactive model picker on
    // Windows" design already documented for 03-pull-models.sh's TUI picker.
    #[serde(default)]
    candidates: HashMap<String, Vec<ModelCandidate>>,
}

// camelCase shape sent to the frontend, matching this file's existing
// Rust->JS convention (see ModelInfo/PullProgress-style structs).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct DetectResult {
    distro_pretty: String,
    gpu_vendor: String,
    gpu_name: String,
    cpu_model: String,
    cpu_threads: u32,
    ram_gb: u64,
    tier: String,
    tier_models: HashMap<String, String>,
    candidates: HashMap<String, Vec<ModelCandidate>>,
}

impl From<DetectResultRaw> for DetectResult {
    fn from(r: DetectResultRaw) -> Self {
        DetectResult {
            distro_pretty: r.distro_pretty,
            gpu_vendor: r.gpu_vendor,
            gpu_name: r.gpu_name,
            cpu_model: r.cpu_model,
            cpu_threads: r.cpu_threads,
            ram_gb: r.ram_gb,
            tier: r.tier,
            tier_models: r.tier_models,
            candidates: r.candidates,
        }
    }
}

// Runs a detection command to completion and returns its captured stdout.
// Unlike run_step/stream_child (used for the real install, which streams
// output live to the frontend as events), detection is a single fast
// read-only call whose result is needed synchronously, so plain
// Command::output() is enough here. Windows-only since Phase (detect_linux
// rewiring): the Linux path now calls selfllama_core directly against real
// command output rather than parsing a script's __DETECT__ JSON line, so
// this stays only for detect_windows below.
#[cfg(target_os = "windows")]
fn run_capture(cmd: &mut Command) -> Result<String, String> {
    let output = cmd
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("failed to run detection command: {e}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("detection command failed: {stderr}"));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

// Scripts print detection data as a single line prefixed with __DETECT__
// (see 02-configure-gpu.sh/03-pull-models.sh/setup.ps1), so it can't be
// confused with the human-readable log_info/Log-Info lines printed before it.
// Not Windows-gated like run_capture above: still meaningfully tested
// cross-platform (see extracts_detect_json_ignoring_human_readable_log_lines
// below) even though only detect_windows calls it in the real (non-test)
// code path now - the allow(dead_code) below only silences the resulting
// "unused in production code" warning on non-Windows targets, it doesn't
// affect whether the tests below actually exercise this function.
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
fn extract_detect_json(output: &str) -> Result<serde_json::Value, String> {
    for line in output.lines() {
        if let Some(json_str) = line.strip_prefix("__DETECT__") {
            return serde_json::from_str(json_str)
                .map_err(|e| format!("failed to parse detection output: {e}"));
        }
    }
    Err("no detection output (__DETECT__ line) found in script output".into())
}

// Linux detection used to shell out to `02-configure-gpu.sh --detect-only`
// / `03-pull-models.sh --detect-only` and merge their two __DETECT__ JSON
// lines, the same as detect_windows below still does. Now that
// selfllama_core::detect/install::tier hold every piece of logic those two
// invocations needed (GPU vendor from `lspci`, distro from
// `/etc/os-release`, CPU/RAM from `/proc/cpuinfo`+`nproc`/`free`, tier
// selection and the model tables), this calls those pure functions
// directly against real command output gathered here - no script process,
// no __DETECT__ line, no JSON round trip. Detection never needs the
// AMD `rocminfo`/gfx-code lookup 02-configure-gpu.sh also does: that only
// feeds the HSA_OVERRIDE_GFX_VERSION install-time decision
// (core::install::gpu), which DetectResult has no field for anyway.
//
// Windows keeps shelling out to `setup.ps1 -DetectOnly` (see detect_windows
// below) - GPU vendor there needs a CIM `Win32_VideoController` query, which
// has no equivalent pure-Rust path in this crate graph without a new WMI
// dependency, so rewiring that side is left for a later phase.
#[cfg(not(target_os = "windows"))]
fn run_capture_lossy(cmd: &mut Command) -> String {
    cmd.stdin(Stdio::null())
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
        .unwrap_or_default()
}

#[cfg(not(target_os = "windows"))]
fn detect_linux(_repo_root: &Path, tier: &Option<String>) -> Result<DetectResult, String> {
    let mut lspci_cmd = Command::new("lspci");
    lspci_cmd.arg("-nnk");
    let lspci_output = run_capture_lossy(&mut lspci_cmd);
    let gpu = selfllama_core::detect::gpu::parse_gpu_from_lspci(&lspci_output);

    let os_release = std::fs::read_to_string("/etc/os-release").ok();
    let distro = selfllama_core::detect::distro::parse_distro(os_release.as_deref());

    let cpuinfo = std::fs::read_to_string("/proc/cpuinfo").unwrap_or_default();
    let cpu_model = selfllama_core::detect::cpu::parse_cpu_model(&cpuinfo);
    let cpu_threads = selfllama_core::detect::cpu::parse_cpu_threads(&run_capture_lossy(&mut Command::new("nproc")));

    let mut free_g_cmd = Command::new("free");
    free_g_cmd.arg("-g");
    let free_g = run_capture_lossy(&mut free_g_cmd);
    let mut free_m_cmd = Command::new("free");
    free_m_cmd.arg("-m");
    let free_m = run_capture_lossy(&mut free_m_cmd);
    let ram_gb = selfllama_core::detect::ram::compute_ram_gb(&free_g, &free_m);

    let resolved_tier = selfllama_core::install::tier::compute_tier(ram_gb, &gpu.vendor, tier.as_deref());
    let tier_models = selfllama_core::install::tier::default_models(&resolved_tier)
        .ok_or_else(|| format!("no default models for tier {resolved_tier}"))?;
    let candidates = selfllama_core::install::tier::candidates(&resolved_tier)
        .ok_or_else(|| format!("no candidates for tier {resolved_tier}"))?
        .into_iter()
        .map(|(usage, cands)| {
            (usage, cands.into_iter().map(|c| ModelCandidate { model: c.model, desc: c.desc }).collect())
        })
        .collect();

    Ok(DetectResult {
        distro_pretty: distro.pretty,
        gpu_vendor: gpu.vendor,
        gpu_name: gpu.name,
        cpu_model,
        cpu_threads,
        ram_gb,
        tier: resolved_tier,
        tier_models,
        candidates,
    })
}

// Windows detection needs no elevation (Get-RamGb/Get-GpuVendor/Get-CpuInfo
// are read-only CIM queries), so this calls powershell.exe directly rather
// than through the Start-Process -Verb RunAs wrapper run_windows uses for
// the real (privileged) install below.
#[cfg(target_os = "windows")]
fn detect_windows(repo_root: &Path, tier: &Option<String>) -> Result<DetectResult, String> {
    let mut args = vec![
        "-NoProfile".to_string(),
        "-ExecutionPolicy".to_string(),
        "Bypass".to_string(),
        "-File".to_string(),
        repo_root.join("setup.ps1").to_string_lossy().to_string(),
        "-DetectOnly".to_string(),
    ];
    if let Some(t) = tier {
        args.push("-Tier".to_string());
        args.push(t.clone());
    }

    let mut cmd = Command::new("powershell.exe");
    cmd.current_dir(repo_root).args(&args);
    let json = extract_detect_json(&run_capture(&mut cmd)?)?;
    let raw: DetectResultRaw =
        serde_json::from_value(json).map_err(|e| format!("failed to parse detection result: {e}"))?;
    Ok(raw.into())
}

#[tauri::command]
fn detect_system(app: AppHandle, options: DetectOptions) -> Result<DetectResult, String> {
    let repo_root = find_scripts_dir(&app)?;
    #[cfg(target_os = "windows")]
    return detect_windows(&repo_root, &options.tier);
    #[cfg(not(target_os = "windows"))]
    return detect_linux(&repo_root, &options.tier);
}

#[derive(Debug, Clone, Serialize)]
struct LogLine {
    stream: String, // "stdout" | "stderr" | "meta"
    text: String,
}

// Drives the frontend's step indicator. Linux gets one event per script
// (ollama/gpu/models/webui) since each runs as its own child process; setup.ps1
// runs as a single opaque elevated process on Windows, so it only ever gets
// one combined "windows" step — a real 4-step breakdown there would just be
// guesswork, since we can't see inside that process's own progress.
#[derive(Debug, Clone, Serialize)]
struct InstallStep {
    id: String,
    label: String,
    status: String, // "pending" | "running" | "running-silent" | "done" | "failed" | "skipped" | "needs-confirmation"
    // Some only alongside "running-silent" - a static status line (see
    // selfllama_installer::privileged::protocol::StepEvent's own doc
    // comment), never a percentage or any other fabricated progress figure.
    #[serde(skip_serializing_if = "Option::is_none")]
    message: Option<String>,
}

fn emit_step(app: &AppHandle, id: &str, label: &str, status: &str) {
    emit_step_with_message(app, id, label, status, None);
}

fn emit_step_with_message(app: &AppHandle, id: &str, label: &str, status: &str, message: Option<&str>) {
    let _ = app.emit(
        "install-step",
        InstallStep { id: id.into(), label: label.into(), status: status.into(), message: message.map(str::to_string) },
    );
}

#[derive(Debug, Clone, Serialize)]
struct InstallDone {
    success: bool,
    message: String,
}

// Sent as the "install-confirmation-needed" event payload when
// core::install::gpu::NvidiaPlan.confirmation (surfaced through
// application::privileged's __STEP__ protocol as
// PrivStepStatus::NeedsConfirmation) reaches run_linux. camelCase to match
// this file's existing Rust->JS convention; converted from
// selfllama_installer's own snake_case ConfirmationRequest rather than
// reusing it directly, same DetectResultRaw->DetectResult idiom already
// used above for the scripts' own JSON.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfirmationPayload {
    pub prompt_title: String,
    pub prompt_message: String,
    pub action_description: String,
}

impl From<ConfirmationRequest> for ConfirmationPayload {
    fn from(r: ConfirmationRequest) -> Self {
        ConfirmationPayload {
            prompt_title: r.prompt_title,
            prompt_message: r.prompt_message,
            action_description: r.action_description,
        }
    }
}

// run_linux's/run_windows's shared result shape: either the whole sequence
// completed (success/failure still communicated the existing way, via
// install-done), or it stopped specifically because a step needs real user
// confirmation - which must never be resolved automatically one way or the
// other (see run_install below). run_windows always returns `Completed`:
// core::install's Nvidia-confirmation plan only exists on the Linux side.
enum InstallOutcome {
    Completed,
    NeedsConfirmation(ConfirmationPayload),
}

// A packaged install (.deb/.rpm/.AppImage/.msi/.exe, downloaded from GitHub
// Releases) has no repo checkout anywhere near it - it never did, despite
// find_repo_root's old doc comment calling that "a future bundled app placed
// at the repo root". A real user hitting this got a bare "could not locate
// setup.sh in any parent directory of the running executable" with no repo
// in sight to place it next to. The actual fix is bundling the scripts as
// Tauri resources (tauri.conf.json's bundle.resources, both .sh and .ps1
// variants copied into every platform's package - simpler than juggling
// per-platform resource lists, and a few KB of unused-on-that-OS scripts
// costs nothing that matters), and resolving them from the app's resource
// directory at runtime. find_repo_root() is kept as the fallback for
// `cargo run`/`cargo build` from within a repo checkout during development,
// where resources aren't necessarily copied next to the debug binary.
fn find_scripts_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let marker_name = if cfg!(target_os = "windows") { "setup.ps1" } else { "setup.sh" };

    if let Ok(resource_dir) = app.path().resource_dir() {
        let candidate = resource_dir.join("scripts");
        if candidate.join(marker_name).is_file() {
            return Ok(candidate);
        }
    }

    find_repo_root()
}

// Walks up from the running executable's directory looking for setup.sh
// (Linux) / setup.ps1 (Windows) - a repo checkout's root, for `cargo run`/
// `cargo build` during development. Not meant for a packaged install; see
// find_scripts_dir above.
fn find_repo_root() -> Result<PathBuf, String> {
    let start =
        std::env::current_exe().map_err(|e| format!("cannot resolve current executable path: {e}"))?;
    let start_dir = start.parent().map(Path::to_path_buf).ok_or_else(|| {
        format!("executable path {} has no parent directory", start.display())
    })?;

    let marker_name = if cfg!(target_os = "windows") { "setup.ps1" } else { "setup.sh" };
    find_marker_upwards(&start_dir, marker_name)
}

// Split out from find_repo_root so the walk-up logic can be exercised with a
// throwaway directory tree instead of the test binary's own real location.
fn find_marker_upwards(start_dir: &Path, marker_name: &str) -> Result<PathBuf, String> {
    let mut dir = Some(start_dir.to_path_buf());

    for _ in 0..8 {
        let Some(candidate) = dir.clone() else { break };
        if candidate.join(marker_name).is_file() {
            return Ok(candidate);
        }
        dir = candidate.parent().map(Path::to_path_buf);
    }

    Err(format!(
        "could not locate {marker_name} in any parent directory of the running executable"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_marker_in_starting_directory() {
        let tmp = std::env::temp_dir().join(format!("selfllama-test-{}", std::process::id()));
        std::fs::create_dir_all(&tmp).unwrap();
        std::fs::write(tmp.join("setup.sh"), "").unwrap();

        let found = find_marker_upwards(&tmp, "setup.sh").unwrap();
        assert_eq!(found, tmp);

        std::fs::remove_dir_all(&tmp).unwrap();
    }

    #[test]
    fn finds_marker_several_levels_up() {
        let base = std::env::temp_dir().join(format!("selfllama-test-nested-{}", std::process::id()));
        let nested = base.join("gui/src-tauri/target/debug");
        std::fs::create_dir_all(&nested).unwrap();
        std::fs::write(base.join("setup.sh"), "").unwrap();

        let found = find_marker_upwards(&nested, "setup.sh").unwrap();
        assert_eq!(found, base);

        std::fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn errors_when_marker_is_never_found() {
        let tmp = std::env::temp_dir().join(format!("selfllama-test-missing-{}", std::process::id()));
        std::fs::create_dir_all(&tmp).unwrap();

        let result = find_marker_upwards(&tmp, "setup.sh");
        assert!(result.is_err());

        std::fs::remove_dir_all(&tmp).unwrap();
    }

    #[test]
    fn extracts_detect_json_ignoring_human_readable_log_lines() {
        let output = "[INFO] Detected distro: Ubuntu 24.04 LTS (family: debian)\n\
                       [INFO] GPU selected for acceleration: none (none)\n\
                       __DETECT__{\"distro_pretty\":\"Ubuntu 24.04 LTS\",\"gpu_vendor\":\"none\"}\n";

        let value = extract_detect_json(output).unwrap();
        assert_eq!(value["distro_pretty"], "Ubuntu 24.04 LTS");
        assert_eq!(value["gpu_vendor"], "none");
    }

    #[test]
    fn errors_when_no_detect_line_present() {
        let output = "[INFO] just some regular log output\n";
        assert!(extract_detect_json(output).is_err());
    }

    #[test]
    fn merges_gpu_and_models_detect_json_into_one_result() {
        let gpu_json: serde_json::Value = serde_json::from_str(
            r#"{"distro_pretty":"Ubuntu 24.04 LTS","gpu_vendor":"none","gpu_name":"","cpu_model":"Generic CPU","cpu_threads":8}"#,
        )
        .unwrap();
        let models_json: serde_json::Value = serde_json::from_str(
            r#"{"ram_gb":16,"tier":"S","tier_models":{"texte":"llama3.1:8b"},"candidates":{"texte":[{"model":"llama3.1:8b","desc":"Well-balanced generalist"}]}}"#,
        )
        .unwrap();

        let mut merged = gpu_json.as_object().cloned().unwrap();
        merged.extend(models_json.as_object().cloned().unwrap());
        let raw: DetectResultRaw =
            serde_json::from_value(serde_json::Value::Object(merged)).unwrap();
        let result: DetectResult = raw.into();

        assert_eq!(result.distro_pretty, "Ubuntu 24.04 LTS");
        assert_eq!(result.tier, "S");
        assert_eq!(result.tier_models["texte"], "llama3.1:8b");
        assert_eq!(result.candidates["texte"][0].model, "llama3.1:8b");
    }

    // Validates the native detect_linux (selfllama_core-backed, no script
    // process spawned) against this session's own real dev machine (Intel
    // Iris Xe iGPU, Pop!_OS 24.04) - #[ignore] like
    // application/core/tests/real_machine_comparison.rs, since it depends
    // on real hardware/lspci/free output rather than being safe to run
    // unconditionally in CI. Run with:
    //   cargo test --lib -- --ignored --nocapture native_linux_detection_matches_the_scripts_real_output
    //
    // Expected values captured by running the OLD script-based path on this
    // exact machine right before this test was written:
    //   02-configure-gpu.sh --detect-only --no-tui:
    //     {"distro_pretty":"Pop!_OS 24.04 LTS","gpu_vendor":"intel",
    //      "gpu_name":"Intel Corporation TigerLake-LP GT2 [Iris Xe Graphics]
    //      [8086:9a49] (rev 01)",
    //      "cpu_model":"11th Gen Intel(R) Core(TM) i5-1145G7 @ 2.60GHz",
    //      "cpu_threads":8}
    //   03-pull-models.sh --detect-only --no-tui:
    //     {"ram_gb":15,"tier":"S","tier_models":{"texte":"llama3.1:8b", ...}}
    #[cfg(not(target_os = "windows"))]
    #[test]
    #[ignore]
    fn native_linux_detection_matches_the_scripts_real_output() {
        let result = detect_linux(Path::new("."), &None).expect("native detection failed");
        println!("{result:#?}");

        assert_eq!(result.distro_pretty, "Pop!_OS 24.04 LTS");
        assert_eq!(result.gpu_vendor, "intel");
        assert!(result.gpu_name.contains("Iris Xe Graphics"));
        assert_eq!(result.cpu_threads, 8);
        assert_eq!(result.ram_gb, 15);
        assert_eq!(result.tier, "S");
        assert_eq!(result.tier_models["texte"], "llama3.1:8b");
        assert_eq!(result.tier_models["code"], "qwen2.5-coder:7b");
        assert_eq!(result.tier_models["reflexion"], "deepseek-r1:7b");
        assert_eq!(result.tier_models["embeddings"], "nomic-embed-text");
        assert_eq!(result.candidates["texte"][0].model, "llama3.1:8b");
    }
}

fn emit_log(app: &AppHandle, stream: &str, text: String) {
    let _ = app.emit("install-log", LogLine { stream: stream.into(), text });
}

// Uses selfllama_core::progress::stream_ansi_aware (the same function
// application::privileged uses internally) rather than a local copy - this
// file used to carry its own, byte-for-byte identical implementation;
// removed as part of wiring this crate up to application::core "in depth",
// not just application::privileged, per this phase's task. Its own
// ANSI-parsing unit tests moved with it - already covered by
// application/core/src/progress/mod.rs's test suite, so nothing lost.
fn stream_child(app: &AppHandle, mut child: std::process::Child) -> Result<bool, String> {
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();

    let app_out = app.clone();
    let out_handle = stdout.map(|s| {
        std::thread::spawn(move || {
            selfllama_core::progress::stream_ansi_aware(s, |line| emit_log(&app_out, "stdout", line));
        })
    });

    let app_err = app.clone();
    let err_handle = stderr.map(|s| {
        std::thread::spawn(move || {
            selfllama_core::progress::stream_ansi_aware(s, |line| emit_log(&app_err, "stderr", line));
        })
    });

    let status = child.wait().map_err(|e| format!("failed to wait for child process: {e}"))?;
    if let Some(h) = out_handle {
        let _ = h.join();
    }
    if let Some(h) = err_handle {
        let _ = h.join();
    }

    Ok(status.success())
}

fn run_step(app: &AppHandle, id: &str, label: &str, mut cmd: Command) -> Result<bool, String> {
    emit_log(app, "meta", format!("--- {label} ---"));
    emit_step(app, id, label, "running");
    let child = cmd
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("failed to start {label}: {e}"))?;
    let ok = stream_child(app, child)?;
    emit_step(app, id, label, if ok { "done" } else { "failed" });
    Ok(ok)
}

#[cfg(not(target_os = "windows"))]
fn run_linux(app: &AppHandle, repo_root: &Path, opts: &InstallOptions) -> Result<InstallOutcome, String> {
    // Emitted up front so the stepper shows the full picture (including
    // skipped steps) before anything actually starts.
    emit_step(app, "ollama", "Installing Ollama", "pending");
    emit_step(app, "gpu", "Configuring GPU", "pending");
    emit_step(
        app,
        "webui-deps",
        "Installing Open WebUI prerequisites",
        if opts.skip_webui { "skipped" } else { "pending" },
    );
    emit_step(app, "models", "Downloading models", if opts.skip_models { "skipped" } else { "pending" });
    emit_step(app, "webui", "Installing Open WebUI", if opts.skip_webui { "skipped" } else { "pending" });

    // ollama/gpu/webui-deps: previously three separate `pkexec` calls here
    // (three password prompts in the worst case - the exact problem this
    // whole restructure exists to fix, see CLAUDE.md's Phase 2 finding that
    // polkit offers no session-caching trick that could have fixed this
    // from the app side alone). Now a single application::privileged phase.
    if let Some(confirmation) = run_privileged_phase(app, opts)? {
        return Ok(InstallOutcome::NeedsConfirmation(confirmation));
    }

    // models/webui: unaffected by this phase, still the invoking user's own
    // scripts run directly, exactly as before - see CLAUDE.md's
    // application::privileged section for why (only the "webui-deps"
    // prerequisites step was ever privileged; installing Open WebUI itself
    // and pulling models both stay per-user, unprivileged work).
    if !opts.skip_models {
        let mut cmd = Command::new(repo_root.join("03-pull-models.sh"));
        cmd.current_dir(repo_root).arg("--no-tui");
        if let Some(tier) = &opts.tier {
            cmd.arg(format!("--tier={tier}"));
        }
        if let Some(models) = &opts.models {
            for (usage, model) in models {
                cmd.arg(format!("--model-{usage}={model}"));
            }
        }
        let ok = run_step(app, "models", "Downloading models", cmd)?;
        if !ok {
            return Err("Model download failed, see the log above.".into());
        }
    }

    if !opts.skip_webui {
        let mut cmd = Command::new(repo_root.join("04-install-webui.sh"));
        cmd.current_dir(repo_root);
        let ok = run_step(app, "webui", "Installing Open WebUI", cmd)?;
        if !ok {
            return Err("Open WebUI installation failed, see the log above.".into());
        }
    }

    Ok(InstallOutcome::Completed)
}

// Runs the single privileged phase (ollama, gpu, webui-deps, in that
// load-bearing order - see application::privileged's own doc comment) via
// application::privileged::spawn_privileged_phase, translating its
// __STEP__ protocol into this app's existing install-step/install-log
// events so the frontend's contract doesn't change even though the
// mechanism producing them did.
//
// current_exe is *this* GUI binary, re-invoked by pkexec with
// selfllama_installer::privileged::RUN_PRIVILEGED_PHASE_ARG - see main()
// below, which now checks for that argument before starting the Tauri app,
// exactly like selfllama-installer's own main.rs does. Without that check
// here too, pkexec would try to launch a second full GUI as root instead of
// running the privileged worker code.
//
// Returns `Ok(Some(payload))` when the sequence stopped because
// `core::install::gpu::NvidiaPlan.confirmation` needs a real answer (see
// application::privileged's own report on why this is surfaced, not
// resolved, at that layer) - never assumed one way or the other here
// either; `run_install` is the layer that actually asks, via a real
// confirm() dialog in the frontend.
fn run_privileged_phase(app: &AppHandle, opts: &InstallOptions) -> Result<Option<ConfirmationPayload>, String> {
    let current_exe = std::env::current_exe()
        .map_err(|e| format!("cannot resolve current executable path: {e}"))?;

    emit_log(app, "meta", "--- Installing Ollama, configuring GPU, installing Open WebUI prerequisites ---".into());
    emit_log(
        app,
        "meta",
        "This step needs administrator privileges — you may be prompted for your password.".into(),
    );

    let privileged_opts = PrivilegedPhaseOptions {
        skip_webui: opts.skip_webui,
        confirm_nvidia_driver_install: opts.confirm_nvidia_driver_install,
    };

    run_privileged_phase_raw(&current_exe, privileged_opts, None, &[], |stream, text| {
        match parse_step_event(text) {
            Some(event) => {
                let status = match event.status {
                    PrivStepStatus::Running => "running",
                    PrivStepStatus::Done => "done",
                    PrivStepStatus::Failed => "failed",
                    PrivStepStatus::NeedsConfirmation => "needs-confirmation",
                    // Still mid-step, not a new transition on its own - see
                    // this crate's CLAUDE.md section on the silent-phase
                    // indicator. event.message carries the static status
                    // text (pacman/dnf/zypper's known-silent network phase);
                    // never a percentage.
                    PrivStepStatus::RunningSilent => "running-silent",
                };
                emit_step_with_message(app, &event.id, &event.label, status, event.message.as_deref());
                if let Some(error) = &event.error {
                    emit_log(app, "stderr", format!("{}: {error}", event.label));
                }
            }
            None => emit_log(app, stream, text.to_string()),
        }
    })
}

/// The `AppHandle`-free core of [`run_privileged_phase`]: spawns the single
/// privileged phase and drains its output through `on_line(stream, text)`
/// for *every* line (both `__STEP__` protocol lines and plain log text -
/// unlike the `AppHandle`-based wrapper above, this does not pre-filter
/// which is which, so a caller/test can inspect either). Returns
/// `Ok(Some(payload))` when the sequence stopped on
/// `core::install::gpu::NvidiaPlan.confirmation` (parsed from whichever
/// `__STEP__` line reported it), same as the wrapper.
///
/// Split out specifically so `tests/single_pkexec_call.rs` can call this
/// directly: `gui/src-tauri` has no other public API a `tests/*.rs`
/// integration test could reach (a binary-only crate can't be depended on
/// by its own integration tests - same reason `application/src-tauri`
/// gained a `[lib]` target in an earlier phase), and this is genuinely new
/// code this phase adds (`current_exe` here resolves to *this* GUI binary,
/// re-invoked via `main()`'s new sentinel-argument dispatch) - proving "one
/// pkexec" holds specifically in this context, not just relying on
/// `application/src-tauri`'s own already-thorough proof of the same
/// property for `selfllama-installer`'s re-invocation of itself.
pub fn run_privileged_phase_raw(
    current_exe: &Path,
    privileged_opts: PrivilegedPhaseOptions,
    path_override: Option<&str>,
    extra_env: &[(&str, &str)],
    mut on_line: impl FnMut(&str, &str),
) -> Result<Option<ConfirmationPayload>, String> {
    let handle = spawn_privileged_phase(current_exe, privileged_opts, path_override, extra_env)
        .map_err(|e| format!("failed to start the privileged install phase: {e}"))?;

    let mut confirmation: Option<ConfirmationPayload> = None;
    for line in handle.lines.iter() {
        let (stream, text) = match &line {
            PhaseLine::Stdout(s) => ("stdout", s.as_str()),
            PhaseLine::Stderr(s) => ("stderr", s.as_str()),
        };

        if let Some(event) = parse_step_event(text) {
            if event.status == PrivStepStatus::NeedsConfirmation {
                if let Some(c) = event.confirmation.clone() {
                    confirmation = Some(c.into());
                }
            }
        }

        on_line(stream, text);
    }

    match handle.wait() {
        Ok(()) => Ok(None),
        Err(e) => match confirmation {
            Some(c) => Ok(Some(c)),
            None => Err(format!("Privileged install phase failed: {e}")),
        },
    }
}

// setup.ps1 is a single script covering both privileged (winget/Ollama
// install) and unprivileged (model pull, Open WebUI) steps. Unlike pkexec on
// Linux, Windows UAC elevation keeps the same user account and just raises
// the integration level, so running the whole script elevated does not
// misplace per-user state the way running everything as root would on
// Linux, and a single elevation prompt is enough.
#[cfg(target_os = "windows")]
fn run_windows(app: &AppHandle, repo_root: &Path, opts: &InstallOptions) -> Result<InstallOutcome, String> {
    emit_step(app, "windows", "Running setup.ps1 (elevated)", "pending");

    let mut ps_args = vec![
        "-NoProfile".to_string(),
        "-ExecutionPolicy".to_string(),
        "Bypass".to_string(),
        "-File".to_string(),
        repo_root.join("setup.ps1").to_string_lossy().to_string(),
    ];
    if opts.skip_models {
        ps_args.push("-SkipModels".to_string());
    }
    if opts.skip_webui {
        ps_args.push("-SkipWebui".to_string());
    }
    if let Some(tier) = &opts.tier {
        ps_args.push("-Tier".to_string());
        ps_args.push(tier.clone());
    }

    let arg_list = ps_args
        .iter()
        .map(|a| format!("'{}'", a.replace('\'', "''")))
        .collect::<Vec<_>>()
        .join(",");

    let mut cmd = Command::new("powershell.exe");
    cmd.current_dir(repo_root).arg("-Command").arg(format!(
        "Start-Process -FilePath powershell.exe -Verb RunAs -Wait -ArgumentList {arg_list}"
    ));

    let ok = run_step(app, "windows", "Running setup.ps1 (elevated)", cmd)?;
    if !ok {
        return Err("setup.ps1 failed, see the log above.".into());
    }
    Ok(InstallOutcome::Completed)
}

#[tauri::command]
fn run_install(app: AppHandle, options: InstallOptions) -> Result<InstallDone, String> {
    let repo_root = find_scripts_dir(&app)?;
    emit_log(&app, "meta", format!("Scripts directory: {}", repo_root.display()));

    #[cfg(target_os = "windows")]
    let result = run_windows(&app, &repo_root, &options);
    #[cfg(not(target_os = "windows"))]
    let result = run_linux(&app, &repo_root, &options);

    match result {
        Ok(InstallOutcome::Completed) => {
            let done = InstallDone { success: true, message: "Installation complete.".into() };
            let _ = app.emit("install-done", done.clone());
            Ok(done)
        }
        // Deliberately does NOT emit install-done: the install isn't
        // finished or failed, it's paused on a real decision only the user
        // can make (see run_privileged_phase/ConfirmationPayload). The
        // frontend shows its own dialog on this event and either re-invokes
        // run_install with confirmNvidiaDriverInstall: true (accepted) or
        // treats the install as cancelled locally (declined) - never
        // auto-resolved on the Rust side either way.
        Ok(InstallOutcome::NeedsConfirmation(payload)) => {
            let _ = app.emit("install-confirmation-needed", payload);
            Ok(InstallDone { success: false, message: "Waiting for confirmation.".into() })
        }
        Err(e) => {
            let done = InstallDone { success: false, message: e.clone() };
            let _ = app.emit("install-done", done);
            Err(e)
        }
    }
}

// Reuses the same Tauri app/window infrastructure rather than a separate
// webview tech (e.g. pywebview): just a second window pointed directly at
// Open WebUI, with no browser chrome (Tauri windows never have an address
// bar), identical on Linux and Windows since it only uses Tauri's own
// cross-platform window APIs.
#[tauri::command]
fn open_webui_window(app: AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("webui") {
        return window.set_focus().map_err(|e| e.to_string());
    }

    let url = WEBUI_URL.parse().map_err(|e: url::ParseError| e.to_string())?;
    WebviewWindowBuilder::new(&app, "webui", WebviewUrl::External(url))
        .title("Open WebUI")
        .inner_size(1100.0, 800.0)
        .min_inner_size(480.0, 360.0)
        .build()
        .map_err(|e| e.to_string())?;

    Ok(())
}

/// Starts the actual Tauri app. Split out from `main()` (in `src/main.rs`,
/// a thin wrapper) so this crate has a `[lib]` target at all - a
/// binary-only crate can't be depended on by its own `tests/*.rs`
/// integration tests, and this phase needs one for
/// `tests/single_pkexec_call.rs` to reach `run_privileged_phase_raw` above
/// (same reasoning `application/src-tauri` already applied in an earlier
/// phase).
pub fn run_tauri_app() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![run_install, open_webui_window, detect_system])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
