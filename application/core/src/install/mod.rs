//! Install *plans*: which packages, which commands, which config files -
//! ported from `scripts/linux/01-install-ollama.sh`, `02-configure-gpu.sh`,
//! and `04-install-webui.sh`. Every function here is pure (facts in, a plan
//! data structure out); nothing in this module spawns a process, writes a
//! file, or prompts for anything, same convention as `detect/`. Executing a
//! plan (actually installing packages, writing files, waiting for a
//! service to come up) is `application::privileged`'s job (Phase 4,
//! already built) - not wired to these plans yet, a later phase.
//!
//! **Verified against the real `scripts/linux/` code before writing this
//! port, not assumed** - see this phase's report for the full trap-by-trap
//! verification. Two confirmed, already-fixed traps (`write_amd_override`'s
//! `set -e` pitfall, `configure_nvidia`'s matching one) have no Rust
//! equivalent to port at all: Rust has no implicit "last command's exit
//! status becomes the function's own" propagation for a bare `&&`, so this
//! class of bug cannot recur here by construction, not by a specific design
//! choice made in this module.

pub mod gpu;
pub mod ollama;
pub mod tier;
pub mod webui;

/// Package-manager family, as already determined by
/// `detect::distro::parse_distro` (`DistroInfo.family`: `"arch"`,
/// `"debian"`, `"fedora"`, `"opensuse"`, or `"unknown"`). Re-typed as an enum
/// here (rather than matching on that raw string repeatedly across
/// `gpu.rs`/`webui.rs`) purely for exhaustiveness-checked `match`es in this
/// module - `detect/` itself is out of scope to modify this phase, so this
/// stays a small local conversion rather than a shared type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DistroFamily {
    Arch,
    Debian,
    Fedora,
    OpenSuse,
    Unknown,
}

impl From<&str> for DistroFamily {
    fn from(family: &str) -> Self {
        match family {
            "arch" => DistroFamily::Arch,
            "debian" => DistroFamily::Debian,
            "fedora" => DistroFamily::Fedora,
            "opensuse" => DistroFamily::OpenSuse,
            _ => DistroFamily::Unknown,
        }
    }
}

/// Renders a systemd drop-in's `[Service]` section the same way
/// `write_amd_override`/`configure_intel` do: an always-present
/// `Environment="OLLAMA_VULKAN=1"` line, plus one `Environment="KEY=value"`
/// line per extra variable, in order. Shared between `gpu.rs`'s AMD/Intel
/// plans since both write to the same
/// `/etc/systemd/system/ollama.service.d/override.conf` path with this
/// same shape - only which extra variables (if any) differ.
pub(crate) fn render_ollama_vulkan_dropin(extra_vars: &[(&str, &str)]) -> String {
    let mut content = String::from("[Service]\nEnvironment=\"OLLAMA_VULKAN=1\"\n");
    for (key, value) in extra_vars {
        content.push_str(&format!("Environment=\"{key}={value}\"\n"));
    }
    content
}

/// Path every GPU vendor plan's systemd override writes to or clears -
/// `/etc/systemd/system/ollama.service.d/override.conf` in
/// `02-configure-gpu.sh`.
pub const OLLAMA_SERVICE_OVERRIDE_PATH: &str = "/etc/systemd/system/ollama.service.d/override.conf";

/// What to do with the systemd drop-in: write new content, or remove it
/// (`clear_ollama_override`'s `sudo rm -f ...`) - not "write an empty
/// file", an actual deletion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OllamaServiceOverride {
    Write(String),
    Clear,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distro_family_parses_the_four_known_families() {
        assert_eq!(DistroFamily::from("arch"), DistroFamily::Arch);
        assert_eq!(DistroFamily::from("debian"), DistroFamily::Debian);
        assert_eq!(DistroFamily::from("fedora"), DistroFamily::Fedora);
        assert_eq!(DistroFamily::from("opensuse"), DistroFamily::OpenSuse);
    }

    #[test]
    fn distro_family_falls_back_to_unknown() {
        assert_eq!(DistroFamily::from("unknown"), DistroFamily::Unknown);
        assert_eq!(DistroFamily::from("nixos"), DistroFamily::Unknown);
    }

    #[test]
    fn dropin_with_no_extra_vars_is_just_the_vulkan_flag() {
        assert_eq!(render_ollama_vulkan_dropin(&[]), "[Service]\nEnvironment=\"OLLAMA_VULKAN=1\"\n");
    }

    #[test]
    fn dropin_appends_extra_vars_in_order() {
        assert_eq!(
            render_ollama_vulkan_dropin(&[("HSA_OVERRIDE_GFX_VERSION", "11.5.0")]),
            "[Service]\nEnvironment=\"OLLAMA_VULKAN=1\"\nEnvironment=\"HSA_OVERRIDE_GFX_VERSION=11.5.0\"\n"
        );
    }
}
