//! Distro family/pretty-name detection, ported from `detect_distro()` in
//! `scripts/linux/lib/common.sh`. Linux-only: Windows has no equivalent (one
//! OS, no package-manager family to pick), matching `lib/common.ps1`'s own
//! doc comment on why it has no distro detection of its own.

use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DistroInfo {
    pub family: String,
    pub pretty: String,
}

impl Default for DistroInfo {
    fn default() -> Self {
        // Matches detect_distro()'s DISTRO_FAMILY="unknown" / DISTRO_PRETTY="unknown".
        DistroInfo { family: "unknown".to_string(), pretty: "unknown".to_string() }
    }
}

/// Port of `detect_distro()`. `os_release` is `None` when `/etc/os-release`
/// doesn't exist (mirrors the Bash script's `if [ -f "$OS_RELEASE_FILE" ]`
/// guard, which otherwise leaves both fields at their "unknown" default),
/// `Some(contents)` otherwise. Family is picked from `${ID:-}${ID_LIKE:-}`
/// concatenated *without* a separator, exactly as the Bash `case` statement
/// does, checked in the same priority order (arch, then debian/ubuntu, then
/// fedora/rhel, then suse).
pub fn parse_distro(os_release: Option<&str>) -> DistroInfo {
    let Some(contents) = os_release else {
        return DistroInfo::default();
    };

    let fields = parse_os_release_fields(contents);
    let id = fields.get("ID").cloned().unwrap_or_default();
    let id_like = fields.get("ID_LIKE").cloned().unwrap_or_default();
    let pretty = fields.get("PRETTY_NAME").cloned().unwrap_or_else(|| "unknown".to_string());

    // No separator between $ID and $ID_LIKE, same as Bash's "${ID:-}${ID_LIKE:-}".
    let combined = format!("{id}{id_like}");

    let family = if combined.contains("arch") {
        "arch"
    } else if combined.contains("debian") || combined.contains("ubuntu") {
        "debian"
    } else if combined.contains("fedora") || combined.contains("rhel") {
        "fedora"
    } else if combined.contains("suse") {
        "opensuse"
    } else {
        "unknown"
    };

    DistroInfo { family: family.to_string(), pretty }
}

/// Minimal `KEY=VALUE` line parser for `/etc/os-release` content: strips one
/// layer of surrounding double quotes if present (real os-release files
/// always quote multi-word values like `PRETTY_NAME="Pop!_OS 24.04 LTS"`),
/// otherwise takes the value verbatim. This is not a full shell parser
/// (no variable expansion, no single-quote handling) - real os-release
/// files don't use either, only plain or double-quoted literals.
fn parse_os_release_fields(contents: &str) -> std::collections::HashMap<String, String> {
    let mut fields = std::collections::HashMap::new();
    for line in contents.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((key, value)) = line.split_once('=') {
            let value = value.trim();
            let value = value.strip_prefix('"').and_then(|v| v.strip_suffix('"')).unwrap_or(value);
            fields.insert(key.trim().to_string(), value.to_string());
        }
    }
    fields
}

#[cfg(test)]
mod tests {
    use super::*;

    const POP_OS: &str = "\
NAME=\"Pop!_OS\"
VERSION=\"24.04 LTS\"
ID=pop
ID_LIKE=\"ubuntu debian\"
PRETTY_NAME=\"Pop!_OS 24.04 LTS\"
VERSION_ID=\"24.04\"
VERSION_CODENAME=noble
UBUNTU_CODENAME=noble";

    const ARCH: &str = "\
NAME=\"Arch Linux\"
PRETTY_NAME=\"Arch Linux\"
ID=arch
BUILD_ID=rolling";

    const FEDORA: &str = "\
NAME=\"Fedora Linux\"
VERSION=\"44 (Workstation Edition)\"
ID=fedora
VERSION_ID=44
PRETTY_NAME=\"Fedora Linux 44 (Workstation Edition)\"";

    const RHEL_LIKE: &str = "\
NAME=\"Rocky Linux\"
ID=\"rocky\"
ID_LIKE=\"rhel centos fedora\"
PRETTY_NAME=\"Rocky Linux 9.4\"";

    const OPENSUSE: &str = "\
NAME=\"openSUSE Leap\"
ID=\"opensuse-leap\"
ID_LIKE=\"suse opensuse\"
PRETTY_NAME=\"openSUSE Leap 16.0\"";

    const UNRECOGNIZED: &str = "\
NAME=\"NixOS\"
ID=nixos
PRETTY_NAME=\"NixOS 24.11\"";

    #[test]
    fn pop_os_is_debian_family_via_id_like() {
        // Real-machine case (this session's own /etc/os-release): ID=pop
        // alone matches nothing, but ID_LIKE="ubuntu debian" does.
        let info = parse_distro(Some(POP_OS));
        assert_eq!(info.family, "debian");
        assert_eq!(info.pretty, "Pop!_OS 24.04 LTS");
    }

    #[test]
    fn arch_is_detected_via_id_alone() {
        assert_eq!(parse_distro(Some(ARCH)).family, "arch");
    }

    #[test]
    fn fedora_is_detected_via_id_alone() {
        assert_eq!(parse_distro(Some(FEDORA)).family, "fedora");
    }

    #[test]
    fn rhel_like_distro_is_fedora_family_via_id_like() {
        assert_eq!(parse_distro(Some(RHEL_LIKE)).family, "fedora");
    }

    #[test]
    fn opensuse_is_detected_via_id_like() {
        assert_eq!(parse_distro(Some(OPENSUSE)).family, "opensuse");
    }

    #[test]
    fn unrecognized_distro_falls_back_to_unknown_family_but_keeps_pretty_name() {
        let info = parse_distro(Some(UNRECOGNIZED));
        assert_eq!(info.family, "unknown");
        assert_eq!(info.pretty, "NixOS 24.11");
    }

    #[test]
    fn missing_os_release_file_defaults_both_fields_to_unknown() {
        assert_eq!(parse_distro(None), DistroInfo::default());
    }

    #[test]
    fn missing_pretty_name_field_defaults_to_unknown() {
        let info = parse_distro(Some("ID=arch\n"));
        assert_eq!(info.pretty, "unknown");
        assert_eq!(info.family, "arch");
    }
}
