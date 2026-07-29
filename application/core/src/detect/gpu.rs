//! GPU vendor/name detection, ported from `scripts/linux/02-configure-gpu.sh`
//! (`detect_all_gpus`, `configure_amd`'s gfx-code lookup) and
//! `scripts/windows/lib/common.ps1` (`Get-GpuVendor`).
//!
//! Every function here is pure: it takes the raw text a real command would
//! print (`lspci -nnk`, `rocminfo`, or a list of Windows `PNPDeviceID`
//! strings) and returns a parsed result, with no process spawned and no
//! system state touched. The actual `Command::new("lspci")`/CIM query calls
//! stay out of this crate entirely (same convention as `launcher-core`),
//! left for whichever caller wires this up later.

use serde::Serialize;
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GpuInfo {
    pub vendor: String,
    pub name: String,
}

impl Default for GpuInfo {
    fn default() -> Self {
        // Matches 02-configure-gpu.sh's GPU_VENDOR="none" / GPU_NAME="" defaults.
        GpuInfo { vendor: "none".to_string(), name: String::new() }
    }
}

/// Port of `detect_all_gpus()`: filters `lspci -nnk` output the same way
/// `grep -Ei 'vga|3d|display'` does (substring match, case-insensitive, not
/// anchored), then picks the first matching line for the highest-priority
/// vendor present (Nvidia `[10de:` > AMD `[1002:`/`[1022:` > Intel `[8086:`),
/// exactly the priority order used for hybrid GPU laptops in the Bash
/// script. Returns `GpuInfo::default()` ("none", "") when no controller line
/// matches, mirroring `detect_all_gpus`'s early `return` when `$lines` is
/// empty.
pub fn parse_gpu_from_lspci(lspci_nnk_output: &str) -> GpuInfo {
    let controller_lines: Vec<&str> = lspci_nnk_output
        .lines()
        .filter(|line| {
            let lower = line.to_ascii_lowercase();
            lower.contains("vga") || lower.contains("3d") || lower.contains("display")
        })
        .collect();

    if controller_lines.is_empty() {
        return GpuInfo::default();
    }

    if let Some(name) = first_line_for_pci_prefix(&controller_lines, &["[10de:"]) {
        return GpuInfo { vendor: "nvidia".to_string(), name };
    }
    if let Some(name) = first_line_for_pci_prefix(&controller_lines, &["[1002:", "[1022:"]) {
        return GpuInfo { vendor: "amd".to_string(), name };
    }
    if let Some(name) = first_line_for_pci_prefix(&controller_lines, &["[8086:"]) {
        return GpuInfo { vendor: "intel".to_string(), name };
    }

    GpuInfo::default()
}

/// Extracts the GPU_NAME the Bash script would compute for the first
/// matching line: `sed -E 's/.*: //'` strips everything up to and including
/// the *last* ": " (a space after the colon) on the line - the PCI
/// vendor:device pair inside brackets (e.g. `[10de:1e87]`) has no space
/// after its colon, so it survives untouched in the returned name, exactly
/// as it does in the Bash output.
fn first_line_for_pci_prefix(lines: &[&str], prefixes: &[&str]) -> Option<String> {
    let line = lines.iter().find(|l| prefixes.iter().any(|p| l.contains(p)))?;
    match line.rfind(": ") {
        Some(idx) => Some(line[idx + 2..].to_string()),
        None => Some((*line).to_string()),
    }
}

/// Port of `configure_amd`'s `gfx=$(rocminfo 2>/dev/null | grep -oE
/// 'gfx[0-9]+' | head -n1 || true)`: first `gfx` followed by one or more
/// ASCII digits, anywhere in the output. `None` when rocminfo produced no
/// such token (mirrors the Bash script's `gfx=""` fallback, which then logs
/// a warning and enables plain Vulkan without an HSA override).
pub fn parse_amd_gfx(rocminfo_output: &str) -> Option<String> {
    let bytes = rocminfo_output.as_bytes();
    let mut i = 0;
    while i + 3 <= bytes.len() {
        if &bytes[i..i + 3] == b"gfx" {
            let mut j = i + 3;
            while j < bytes.len() && bytes[j].is_ascii_digit() {
                j += 1;
            }
            if j > i + 3 {
                return Some(rocminfo_output[i..j].to_string());
            }
        }
        i += 1;
    }
    None
}

/// `AMD_GFX_OVERRIDE` from `02-configure-gpu.sh`: GFX codes not yet in
/// ROCm's officially supported list, mapped to the `HSA_OVERRIDE_GFX_VERSION`
/// value that unlocks the Vulkan fallback for that generation. Update this
/// list in lockstep with the Bash array - see CLAUDE.md's note on
/// `AMD_GFX_OVERRIDE` staying in sync.
pub fn amd_gfx_override_map() -> HashMap<&'static str, &'static str> {
    HashMap::from([
        ("gfx1200", "11.5.0"), // RDNA4 (RX 9060 / 9060 XT)
        ("gfx1201", "11.5.0"), // RDNA4 (RX 9070 / 9070 XT)
    ])
}

/// Looks up a single GFX code the same way `${AMD_GFX_OVERRIDE[$gfx]:-}`
/// does in Bash: `Some(version)` if this generation needs the workaround,
/// `None` if it's already officially supported (or unrecognized).
pub fn amd_gfx_override(gfx: &str) -> Option<&'static str> {
    amd_gfx_override_map().get(gfx).copied()
}

/// Port of `Get-GpuVendor` (`scripts/windows/lib/common.ps1`): each string is
/// a Windows `PNPDeviceID` (format `PCI\VEN_xxxx&DEV_...`). PowerShell's
/// `-match` is case-insensitive by default, so this compares uppercased,
/// same priority order as Linux (Nvidia > AMD > Intel).
pub fn parse_gpu_from_pnp_device_ids(ids: &[String]) -> GpuInfo {
    let upper: Vec<String> = ids.iter().map(|s| s.to_uppercase()).collect();

    if let Some(idx) = upper.iter().position(|s| s.contains("VEN_10DE")) {
        return GpuInfo { vendor: "nvidia".to_string(), name: ids[idx].clone() };
    }
    if let Some(idx) = upper.iter().position(|s| s.contains("VEN_1002") || s.contains("VEN_1022")) {
        return GpuInfo { vendor: "amd".to_string(), name: ids[idx].clone() };
    }
    if let Some(idx) = upper.iter().position(|s| s.contains("VEN_8086")) {
        return GpuInfo { vendor: "intel".to_string(), name: ids[idx].clone() };
    }

    GpuInfo::default()
}

/// Port of `find_rocminfo()`, added to `02-configure-gpu.sh` in the same
/// change that added this function: real ROCm packages sometimes install
/// `rocminfo` outside PATH entirely (confirmed in production on an
/// Arch/EndeavourOS machine with an AMD RDNA4 GPU: the package places the
/// binary under `/opt/rocm/bin` or a version-suffixed
/// `/opt/rocm-X.Y.Z/bin`, neither ever added to PATH).
///
/// Every input here is a pre-gathered fact, same convention as the rest of
/// this crate: this function makes no filesystem or process call itself. A
/// real caller resolves `path_lookup` via `command -v rocminfo` (or `which`),
/// `opt_rocm_bin_executable` via a metadata/executable-bit check on
/// `/opt/rocm/bin/rocminfo`, and `opt_rocm_versioned_candidates` by listing
/// `/opt/rocm-*/bin/rocminfo` matches that exist and are executable, in the
/// same lexicographic order a shell glob would produce (this function just
/// takes the first one, matching the Bash `for candidate in .../bin/rocminfo;
/// do if [ -x "$candidate" ]; then ...; done` loop, which also stops at the
/// first executable match).
pub fn resolve_rocminfo_path(
    path_lookup: Option<&str>,
    opt_rocm_bin_executable: bool,
    opt_rocm_versioned_candidates: &[String],
) -> Option<String> {
    if let Some(p) = path_lookup {
        return Some(p.to_string());
    }
    if opt_rocm_bin_executable {
        return Some("/opt/rocm/bin/rocminfo".to_string());
    }
    opt_rocm_versioned_candidates.first().cloned()
}

// ---------------------------------------------------------------------------
// NOTE ON A SECOND CLAIMED "PRODUCTION TRAP" THAT DOES NOT ACTUALLY EXIST IN
// scripts/linux/02-configure-gpu.sh (verified by direct inspection of the
// current file, not assumed): `configure_intel()` does not distinguish an
// integrated GPU from a discrete Arc card. It applies `OLLAMA_VULKAN=1` +
// `OLLAMA_IGPU_ENABLE=1` unconditionally to any `GPU_VENDOR=intel` match -
// there is no separate "iGPU vs Arc" code path to port, in Bash or here.
//
// Unlike the rocminfo PATH fallback above (a confirmed regression, now
// fixed in both scripts/linux/02-configure-gpu.sh and this crate), this is
// NOT something to fix: it was never implemented and never validated
// against real discrete Arc hardware in any session that touched this
// script. The idea floated during earlier planning - that Vulkan is more
// likely a net win on a discrete Arc card than on some weaker iGPUs, where
// plain CPU inference can sometimes be faster - remains an untested
// hypothesis. Do not add a `parse_intel_is_discrete`-style function or an
// iGPU/Arc branch to this crate without validating on real Arc hardware
// first; see CLAUDE.md's Intel GPU handling section for the same note.
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    const NVIDIA_LSPCI: &str = "\
01:00.0 VGA compatible controller [0300]: NVIDIA Corporation TU104 [GeForce RTX 2080] [10de:1e87] (rev a1)
01:00.1 Audio device [0403]: NVIDIA Corporation TU104 HD Audio Controller [10de:10f8] (rev a1)";

    const AMD_LSPCI: &str = "\
03:00.0 VGA compatible controller [0300]: Advanced Micro Devices, Inc. [AMD/ATI] Navi 48 [Radeon RX 9070/9070 XT] [1002:7550] (rev c0)";

    const INTEL_IGPU_LSPCI: &str = "\
00:02.0 VGA compatible controller [0300]: Intel Corporation TigerLake-LP GT2 [Iris Xe Graphics] [8086:9a49] (rev 01)";

    const HYBRID_INTEL_NVIDIA_LSPCI: &str = "\
00:02.0 VGA compatible controller [0300]: Intel Corporation TigerLake-LP GT2 [Iris Xe Graphics] [8086:9a49] (rev 01)
01:00.0 3D controller [0302]: NVIDIA Corporation GA107M [GeForce RTX 3050 Mobile] [10de:25a2] (rev a1)";

    const NO_GPU_LSPCI: &str = "\
00:00.0 Host bridge [0600]: Intel Corporation Device [8086:9a14]
00:1f.3 Audio device [0403]: Intel Corporation Device [8086:a0c8]";

    #[test]
    fn detects_nvidia_and_extracts_the_pci_bracket_verbatim() {
        let info = parse_gpu_from_lspci(NVIDIA_LSPCI);
        assert_eq!(info.vendor, "nvidia");
        assert_eq!(info.name, "NVIDIA Corporation TU104 [GeForce RTX 2080] [10de:1e87] (rev a1)");
    }

    #[test]
    fn detects_amd_via_either_1002_or_1022_vendor_id() {
        let info = parse_gpu_from_lspci(AMD_LSPCI);
        assert_eq!(info.vendor, "amd");
        assert!(info.name.contains("Radeon RX 9070/9070 XT"));
    }

    #[test]
    fn detects_intel_igpu() {
        let info = parse_gpu_from_lspci(INTEL_IGPU_LSPCI);
        assert_eq!(info.vendor, "intel");
        assert!(info.name.contains("Iris Xe Graphics"));
    }

    #[test]
    fn hybrid_laptop_prioritizes_nvidia_over_intel() {
        let info = parse_gpu_from_lspci(HYBRID_INTEL_NVIDIA_LSPCI);
        assert_eq!(info.vendor, "nvidia");
        assert!(info.name.contains("RTX 3050"));
    }

    #[test]
    fn no_graphics_controller_falls_back_to_none() {
        let info = parse_gpu_from_lspci(NO_GPU_LSPCI);
        assert_eq!(info, GpuInfo::default());
    }

    #[test]
    fn empty_lspci_output_falls_back_to_none() {
        let info = parse_gpu_from_lspci("");
        assert_eq!(info, GpuInfo::default());
    }

    #[test]
    fn parses_gfx_code_from_real_rocminfo_output() {
        let output = "\
Agent 2
*******
  Name:                    gfx1201
  Marketing Name:          AMD Radeon RX 9070 XT
  Vendor Name:             AMD";
        assert_eq!(parse_amd_gfx(output).as_deref(), Some("gfx1201"));
    }

    #[test]
    fn no_gfx_token_when_rocminfo_output_has_none() {
        assert_eq!(parse_amd_gfx("rocminfo: command not found\n"), None);
    }

    #[test]
    fn unsupported_gfx1201_needs_the_hsa_override() {
        assert_eq!(amd_gfx_override("gfx1201"), Some("11.5.0"));
        assert_eq!(amd_gfx_override("gfx1200"), Some("11.5.0"));
    }

    #[test]
    fn officially_supported_gfx_has_no_override() {
        assert_eq!(amd_gfx_override("gfx1030"), None);
    }

    #[test]
    fn windows_pnp_device_id_priority_matches_linux() {
        let ids = vec![
            "PCI\\VEN_8086&DEV_9A49&SUBSYS_...".to_string(),
            "PCI\\VEN_10DE&DEV_25A2&SUBSYS_...".to_string(),
        ];
        let info = parse_gpu_from_pnp_device_ids(&ids);
        assert_eq!(info.vendor, "nvidia");
    }

    #[test]
    fn windows_vendor_match_is_case_insensitive() {
        let ids = vec!["pci\\ven_1002&dev_7550".to_string()];
        let info = parse_gpu_from_pnp_device_ids(&ids);
        assert_eq!(info.vendor, "amd");
    }

    #[test]
    fn windows_no_known_vendor_falls_back_to_none() {
        let ids = vec!["PCI\\VEN_1234&DEV_0000".to_string()];
        assert_eq!(parse_gpu_from_pnp_device_ids(&ids), GpuInfo::default());
    }

    // -- resolve_rocminfo_path: not validated against real AMD RDNA4
    // hardware (none was available in this session) - fixtures only. See
    // the note above `resolve_rocminfo_path` and this phase's report.

    #[test]
    fn path_lookup_wins_over_opt_rocm_when_both_are_present() {
        let candidates = vec!["/opt/rocm-6.1.0/bin/rocminfo".to_string()];
        assert_eq!(
            resolve_rocminfo_path(Some("/usr/bin/rocminfo"), true, &candidates).as_deref(),
            Some("/usr/bin/rocminfo")
        );
    }

    #[test]
    fn falls_back_to_plain_opt_rocm_bin_when_not_on_path() {
        assert_eq!(
            resolve_rocminfo_path(None, true, &[]).as_deref(),
            Some("/opt/rocm/bin/rocminfo")
        );
    }

    #[test]
    fn falls_back_to_first_versioned_opt_rocm_candidate_when_plain_bin_is_absent() {
        let candidates = vec![
            "/opt/rocm-6.1.0/bin/rocminfo".to_string(),
            "/opt/rocm-6.2.0/bin/rocminfo".to_string(),
        ];
        assert_eq!(
            resolve_rocminfo_path(None, false, &candidates).as_deref(),
            Some("/opt/rocm-6.1.0/bin/rocminfo")
        );
    }

    #[test]
    fn none_found_anywhere_resolves_to_none() {
        assert_eq!(resolve_rocminfo_path(None, false, &[]), None);
    }
}
