//! Ported from `scripts/linux/02-configure-gpu.sh`'s `configure_amd`/
//! `configure_nvidia`/`configure_intel` (the driver-install/config side;
//! GPU *detection* itself, including the Nvidia > AMD > Intel priority and
//! the AMD gfx-code/`AMD_GFX_OVERRIDE` lookup, was already ported to
//! `detect::gpu` in an earlier phase and is reused here, not
//! reimplemented - `amd_plan` below takes an already-resolved `gfx: Option<
//! &str>` rather than parsing `rocminfo` output itself).
//!
//! Verified against the current script before porting (see this phase's
//! report): the `set -e` pitfalls on `write_amd_override`/`configure_nvidia`
//! are already fixed in Bash and have no Rust equivalent to avoid (Rust has
//! no implicit propagation of a bare command's exit status as a function's
//! own return value). The Nvidia driver install is genuinely interactive in
//! Bash (a `tui_yesno`/`read -r -p` prompt) - modeled here as an explicit
//! "confirmation required" plan variant, not an assumed yes or no; actually
//! prompting is left to whatever runs this plan (a later phase).
//! `configure_intel` still does not distinguish an integrated GPU from a
//! discrete Arc card - confirmed unchanged since the last check, and not
//! coded here either; see the doc comment on `intel_plan` below.

use super::{render_ollama_vulkan_dropin, DistroFamily, OllamaServiceOverride};
use crate::detect::gpu::amd_gfx_override;

/// A distro-specific action: either a package list to install, or (only
/// openSUSE's Nvidia case today) a manual step the script can't automate
/// and only logs a message about.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DistroAction {
    InstallPackages(Vec<&'static str>),
    ManualStepRequired(&'static str),
}

// ---------------------------------------------------------------------------
// Nvidia
// ---------------------------------------------------------------------------

/// The exact prompt `configure_nvidia` shows (via `tui_yesno` or `read -r
/// -p`, same message either way) before installing a proprietary driver -
/// carried as data here since a plan cannot itself block waiting for a
/// human answer. Whatever runs this plan (a later phase, not
/// `core::install`) is responsible for actually asking and re-invoking with
/// a decision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NvidiaDriverConfirmation {
    pub prompt_title: &'static str,
    pub prompt_message: &'static str,
    pub action: DistroAction,
    /// `configure_nvidia` `exit 0`s right after queuing the driver install,
    /// asking for a reboot, rather than continuing to any later step.
    pub reboot_required_after_install: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NvidiaPlan {
    /// `Some(name)` when `nvidia-smi` is already present (the name comes
    /// from `nvidia-smi --query-gpu=name`, a real command this function
    /// does not run itself - the caller supplies it, same convention as the
    /// rest of this crate).
    pub driver_already_present: Option<String>,
    /// `None` when the driver is already present (nothing to confirm);
    /// `Some(confirmation)` when it's missing and installing it needs the
    /// user to say yes first.
    pub confirmation: Option<NvidiaDriverConfirmation>,
    /// `clear_ollama_override()` unconditionally, at the very top of
    /// `configure_nvidia` - CUDA needs no Vulkan/HSA override, whether or
    /// not a driver is present yet.
    pub override_action: OllamaServiceOverride,
}

/// `driver_present`/`driver_name` are facts the caller already gathered
/// (`command -v nvidia-smi`, then `nvidia-smi --query-gpu=name` if
/// present) - this function runs neither itself.
pub fn nvidia_plan(distro: DistroFamily, driver_present: bool, driver_name: Option<String>) -> NvidiaPlan {
    let confirmation = if driver_present {
        None
    } else {
        let action = match distro {
            DistroFamily::Arch => DistroAction::InstallPackages(vec!["nvidia", "nvidia-utils"]),
            DistroFamily::Debian => DistroAction::InstallPackages(vec!["nvidia-driver"]),
            DistroFamily::Fedora => DistroAction::InstallPackages(vec!["akmod-nvidia"]),
            DistroFamily::OpenSuse => DistroAction::ManualStepRequired(
                "On openSUSE, add the official NVIDIA repository then install x11-video-nvidiaG06.",
            ),
            DistroFamily::Unknown => DistroAction::InstallPackages(vec![]),
        };
        Some(NvidiaDriverConfirmation {
            prompt_title: "Nvidia driver",
            prompt_message: "Install the Nvidia driver now? (requires a reboot afterwards)",
            action,
            reboot_required_after_install: true,
        })
    };

    NvidiaPlan {
        driver_already_present: if driver_present { driver_name } else { None },
        confirmation,
        override_action: OllamaServiceOverride::Clear,
    }
}

// ---------------------------------------------------------------------------
// AMD
// ---------------------------------------------------------------------------

fn amd_packages(distro: DistroFamily) -> Vec<&'static str> {
    match distro {
        DistroFamily::Arch => vec!["vulkan-radeon", "vulkan-icd-loader", "vulkan-tools", "rocminfo"],
        DistroFamily::Debian | DistroFamily::Fedora => vec!["mesa-vulkan-drivers", "vulkan-tools", "rocminfo"],
        DistroFamily::OpenSuse => vec!["vulkan-tools", "rocminfo"],
        DistroFamily::Unknown => vec![],
    }
}

/// What `configure_amd` decides once (if at all) it has a GFX code, ported
/// 1:1 from its three-way branch: no code found at all (`rocminfo` missing
/// or the chip unrecognized) => plain Vulkan, no HSA override; code found
/// and present in `AMD_GFX_OVERRIDE` => needs the workaround; code found
/// and absent from that map => already officially supported by ROCm,
/// default HIP config.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RocmGfxResolution {
    NoGfxDetected,
    NeedsOverride { gfx: String, hsa_override_gfx_version: &'static str },
    OfficiallySupported { gfx: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AmdPlan {
    pub packages: Vec<&'static str>,
    pub resolution: RocmGfxResolution,
    pub override_action: OllamaServiceOverride,
}

/// `gfx` is the already-parsed GFX code (via `detect::gpu::parse_amd_gfx`
/// on real `rocminfo` output), or `None` when `rocminfo` couldn't be found
/// at all (`detect::gpu::resolve_rocminfo_path` returned nothing) or
/// produced no recognizable code - this function does not run `rocminfo`
/// or resolve its path itself, reusing `detect::gpu::amd_gfx_override` for
/// the actual GFX-code decision rather than duplicating
/// `AMD_GFX_OVERRIDE`.
pub fn amd_plan(distro: DistroFamily, gfx: Option<&str>) -> AmdPlan {
    let resolution = match gfx {
        None => RocmGfxResolution::NoGfxDetected,
        Some(gfx) => match amd_gfx_override(gfx) {
            Some(hsa_override_gfx_version) => {
                RocmGfxResolution::NeedsOverride { gfx: gfx.to_string(), hsa_override_gfx_version }
            }
            None => RocmGfxResolution::OfficiallySupported { gfx: gfx.to_string() },
        },
    };

    let override_action = match &resolution {
        RocmGfxResolution::NoGfxDetected => {
            OllamaServiceOverride::Write(render_ollama_vulkan_dropin(&[]))
        }
        RocmGfxResolution::NeedsOverride { hsa_override_gfx_version, .. } => {
            OllamaServiceOverride::Write(render_ollama_vulkan_dropin(&[(
                "HSA_OVERRIDE_GFX_VERSION",
                hsa_override_gfx_version,
            )]))
        }
        RocmGfxResolution::OfficiallySupported { .. } => OllamaServiceOverride::Clear,
    };

    AmdPlan { packages: amd_packages(distro), resolution, override_action }
}

// ---------------------------------------------------------------------------
// Intel
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IntelPlan {
    pub packages: Vec<&'static str>,
    pub override_action: OllamaServiceOverride,
}

fn intel_packages(distro: DistroFamily) -> Vec<&'static str> {
    match distro {
        DistroFamily::Arch => vec!["vulkan-intel", "vulkan-icd-loader", "vulkan-tools"],
        DistroFamily::Debian | DistroFamily::Fedora => vec!["mesa-vulkan-drivers", "vulkan-tools"],
        DistroFamily::OpenSuse => vec!["vulkan-tools"],
        DistroFamily::Unknown => vec![],
    }
}

/// **Does not distinguish an integrated GPU from a discrete Arc card**,
/// on purpose, matching `configure_intel()` exactly as it stands today
/// (re-verified before writing this port, unchanged since it was first
/// documented): `OLLAMA_VULKAN=1`+`OLLAMA_IGPU_ENABLE=1` is applied to
/// *any* Intel GPU. A distinction was discussed during earlier planning
/// but has never been implemented or validated against real discrete Arc
/// hardware in any session that has touched this code - it remains an
/// untested hypothesis. Do not add an iGPU/Arc branch to this function
/// without testing on real Arc hardware first; see CLAUDE.md's Intel GPU
/// handling section for the same note.
pub fn intel_plan(distro: DistroFamily) -> IntelPlan {
    IntelPlan {
        packages: intel_packages(distro),
        override_action: OllamaServiceOverride::Write(render_ollama_vulkan_dropin(&[(
            "OLLAMA_IGPU_ENABLE",
            "1",
        )])),
    }
}

/// `case "$GPU_VENDOR" in ... none) log_info "..."; clear_ollama_override
/// ;; esac`: no dedicated GPU, nothing to install, just make sure no
/// leftover override from a previous run/different GPU lingers.
pub fn none_plan() -> OllamaServiceOverride {
    OllamaServiceOverride::Clear
}

#[cfg(test)]
mod tests {
    use super::*;

    // -- Nvidia: confirmation is modeled explicitly, never assumed --

    #[test]
    fn nvidia_with_driver_present_needs_no_confirmation() {
        let plan = nvidia_plan(DistroFamily::Arch, true, Some("NVIDIA GeForce RTX 3070".to_string()));
        assert_eq!(plan.driver_already_present, Some("NVIDIA GeForce RTX 3070".to_string()));
        assert_eq!(plan.confirmation, None);
        assert_eq!(plan.override_action, OllamaServiceOverride::Clear);
    }

    #[test]
    fn nvidia_without_driver_requires_confirmation_with_the_scripts_exact_prompt() {
        let plan = nvidia_plan(DistroFamily::Arch, false, None);
        let confirmation = plan.confirmation.expect("missing driver must require confirmation");
        assert_eq!(confirmation.prompt_title, "Nvidia driver");
        assert_eq!(confirmation.prompt_message, "Install the Nvidia driver now? (requires a reboot afterwards)");
        assert_eq!(confirmation.action, DistroAction::InstallPackages(vec!["nvidia", "nvidia-utils"]));
        assert!(confirmation.reboot_required_after_install);
    }

    #[test]
    fn nvidia_confirmation_packages_are_distro_specific() {
        assert_eq!(
            nvidia_plan(DistroFamily::Debian, false, None).confirmation.unwrap().action,
            DistroAction::InstallPackages(vec!["nvidia-driver"])
        );
        assert_eq!(
            nvidia_plan(DistroFamily::Fedora, false, None).confirmation.unwrap().action,
            DistroAction::InstallPackages(vec!["akmod-nvidia"])
        );
    }

    #[test]
    fn nvidia_on_opensuse_is_a_manual_step_not_a_package_list() {
        let action = nvidia_plan(DistroFamily::OpenSuse, false, None).confirmation.unwrap().action;
        assert_eq!(
            action,
            DistroAction::ManualStepRequired(
                "On openSUSE, add the official NVIDIA repository then install x11-video-nvidiaG06."
            )
        );
    }

    #[test]
    fn nvidia_always_clears_the_override_driver_or_not() {
        assert_eq!(nvidia_plan(DistroFamily::Arch, true, None).override_action, OllamaServiceOverride::Clear);
        assert_eq!(nvidia_plan(DistroFamily::Arch, false, None).override_action, OllamaServiceOverride::Clear);
    }

    // -- AMD: reuses detect::gpu::amd_gfx_override, three-way resolution --

    #[test]
    fn amd_no_gfx_detected_enables_plain_vulkan_without_a_workaround() {
        let plan = amd_plan(DistroFamily::Arch, None);
        assert_eq!(plan.resolution, RocmGfxResolution::NoGfxDetected);
        assert_eq!(
            plan.override_action,
            OllamaServiceOverride::Write("[Service]\nEnvironment=\"OLLAMA_VULKAN=1\"\n".to_string())
        );
    }

    #[test]
    fn amd_unsupported_gfx1201_needs_the_hsa_override() {
        let plan = amd_plan(DistroFamily::Arch, Some("gfx1201"));
        assert_eq!(
            plan.resolution,
            RocmGfxResolution::NeedsOverride { gfx: "gfx1201".to_string(), hsa_override_gfx_version: "11.5.0" }
        );
        assert_eq!(
            plan.override_action,
            OllamaServiceOverride::Write(
                "[Service]\nEnvironment=\"OLLAMA_VULKAN=1\"\nEnvironment=\"HSA_OVERRIDE_GFX_VERSION=11.5.0\"\n"
                    .to_string()
            )
        );
    }

    #[test]
    fn amd_officially_supported_gfx_clears_the_override_default_hip() {
        let plan = amd_plan(DistroFamily::Arch, Some("gfx1030"));
        assert_eq!(plan.resolution, RocmGfxResolution::OfficiallySupported { gfx: "gfx1030".to_string() });
        assert_eq!(plan.override_action, OllamaServiceOverride::Clear);
    }

    #[test]
    fn amd_packages_are_distro_specific_arch_gets_a_bigger_list() {
        assert_eq!(
            amd_plan(DistroFamily::Arch, None).packages,
            vec!["vulkan-radeon", "vulkan-icd-loader", "vulkan-tools", "rocminfo"]
        );
        assert_eq!(amd_plan(DistroFamily::OpenSuse, None).packages, vec!["vulkan-tools", "rocminfo"]);
    }

    // -- Intel: uniform treatment, no iGPU/Arc distinction, on purpose --

    #[test]
    fn intel_always_sets_both_vulkan_and_igpu_enable_uniformly() {
        // The whole point of this test: there is no way, today, to make
        // intel_plan behave differently for an iGPU vs. a discrete Arc
        // card - both produce byte-identical override content, since the
        // function takes no such distinction as input at all.
        let plan = intel_plan(DistroFamily::Arch);
        assert_eq!(
            plan.override_action,
            OllamaServiceOverride::Write(
                "[Service]\nEnvironment=\"OLLAMA_VULKAN=1\"\nEnvironment=\"OLLAMA_IGPU_ENABLE=1\"\n".to_string()
            )
        );
    }

    #[test]
    fn intel_packages_are_distro_specific() {
        assert_eq!(
            intel_plan(DistroFamily::Arch).packages,
            vec!["vulkan-intel", "vulkan-icd-loader", "vulkan-tools"]
        );
        assert_eq!(intel_plan(DistroFamily::OpenSuse).packages, vec!["vulkan-tools"]);
    }

    // -- No dedicated GPU --

    #[test]
    fn no_gpu_just_clears_any_leftover_override() {
        assert_eq!(none_plan(), OllamaServiceOverride::Clear);
    }
}
