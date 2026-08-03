//! Not part of the crate's normal `cargo test` run (see `#[ignore]` below,
//! same convention as `launcher/src-tauri`'s
//! `pull_list_and_delete_round_trip_against_live_ollama`): this is an
//! integration test, a separate compilation unit from `src/`, so it can
//! shell out to real system commands without adding any I/O to the pure
//! library itself.
//!
//! Feeds this machine's real `lspci -nnk`/`/etc/os-release`/`free -g`/
//! `free -m`/`/proc/cpuinfo`/`nproc` output through `core::detect`'s pure
//! functions and prints the result next to what
//! `scripts/linux/02-configure-gpu.sh --detect-only --no-tui` reports for
//! the same machine, for manual field-by-field comparison - this crate has
//! no dependency of its own on the Bash scripts to compare against
//! automatically. Run with:
//!   cargo test -p core --test real_machine_comparison -- --ignored --nocapture

use core::detect::{cpu, distro, gpu, ram};
use std::process::Command;

fn run(cmd: &str, args: &[&str]) -> String {
    String::from_utf8_lossy(&Command::new(cmd).args(args).output().unwrap().stdout).into_owned()
}

#[test]
#[ignore]
fn print_real_detection_for_manual_comparison() {
    let lspci = run("lspci", &["-nnk"]);
    let os_release = std::fs::read_to_string("/etc/os-release").ok();
    let free_g = run("free", &["-g"]);
    let free_m = run("free", &["-m"]);
    let cpuinfo = std::fs::read_to_string("/proc/cpuinfo").unwrap_or_default();
    let nproc = run("nproc", &[]);

    let gpu_info = gpu::parse_gpu_from_lspci(&lspci);
    let distro_info = distro::parse_distro(os_release.as_deref());
    let ram_gb = ram::compute_ram_gb(&free_g, &free_m);
    let cpu_model = cpu::parse_cpu_model(&cpuinfo);
    let cpu_threads = cpu::parse_cpu_threads(&nproc);

    println!("core::detect result on this real machine:");
    println!("  distro_pretty = {:?}", distro_info.pretty);
    println!("  distro_family = {:?} (not in the __DETECT__ JSON, GPU/CPU-only comparison below)", distro_info.family);
    println!("  gpu_vendor    = {:?}", gpu_info.vendor);
    println!("  gpu_name      = {:?}", gpu_info.name);
    println!("  cpu_model     = {:?}", cpu_model);
    println!("  cpu_threads   = {}", cpu_threads);
    println!("  ram_gb        = {}", ram_gb);
    println!();
    println!("Compare against, on the same machine:");
    println!("  scripts/linux/02-configure-gpu.sh --detect-only --no-tui");
    println!("  scripts/linux/03-pull-models.sh --detect-only --no-tui   (for ram_gb)");

    // Light sanity checks, not a strict oracle comparison (the two runs are
    // not perfectly simultaneous): a real machine always has at least 1
    // logical CPU and some detected distro family here, since this test
    // only makes sense to run on a real Linux dev machine.
    assert!(cpu_threads >= 1);
    assert_ne!(distro_info.family, "unknown");
}
