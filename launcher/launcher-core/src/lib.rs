// Pure logic extracted out of launcher/src-tauri's #[tauri::command]s, with
// zero dependency on tauri/wry/webkit2gtk. Kept as its own crate rather than
// a module in main.rs so `cargo test` here never pulls in tauri at all -
// see the repo-root CLAUDE.md for why (CI cost of installing
// libwebkit2gtk-4.1-dev/libgtk-3-dev just to test string/JSON parsing).

use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
pub struct TagsResponse {
    pub models: Vec<RawModel>,
}

#[derive(Debug, Deserialize)]
pub struct RawModel {
    pub name: String,
    pub size: u64,
    pub modified_at: String,
    #[serde(default)]
    pub details: Option<RawDetails>,
}

#[derive(Debug, Default, Deserialize)]
pub struct RawDetails {
    pub parameter_size: Option<String>,
    pub quantization_level: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelInfo {
    pub name: String,
    pub size: u64,
    pub modified_at: String,
    pub parameter_size: String,
    pub quantization_level: String,
}

// Maps Ollama's raw /api/tags model shape to ModelInfo, defaulting
// parameter_size/quantization_level to empty strings when `details` is
// absent (Ollama omits it for some model types).
pub fn to_model_info(m: RawModel) -> ModelInfo {
    let (parameter_size, quantization_level) = m
        .details
        .map(|d| (d.parameter_size.unwrap_or_default(), d.quantization_level.unwrap_or_default()))
        .unwrap_or_default();
    ModelInfo { name: m.name, size: m.size, modified_at: m.modified_at, parameter_size, quantization_level }
}

// `systemctl --user is-active` sometimes returns empty stdout (e.g. the unit
// was just uninstalled from under us) rather than a recognized state.
pub fn parse_systemctl_is_active(output: &str) -> String {
    let trimmed = output.trim();
    if trimmed.is_empty() { "unknown".to_string() } else { trimmed.to_string() }
}

// IPv6 needs bracket syntax in a URL; plain format!("http://{ip}:8080") is
// not a valid URL for those.
pub fn build_lan_url(ip: std::net::IpAddr) -> String {
    match ip {
        std::net::IpAddr::V4(v4) => format!("http://{v4}:8080"),
        std::net::IpAddr::V6(v6) => format!("http://[{v6}]:8080"),
    }
}

// Parses the WEBUI_HOST= line out of ~/.config/ollama-stack/webui.env
// (Linux) content, defaulting to "not LAN" if the file is empty, missing
// the line, or malformed.
pub fn parse_webui_lan_status(content: &str) -> bool {
    content
        .lines()
        .find_map(|line| line.strip_prefix("WEBUI_HOST=").map(str::trim))
        .map(|host| host == "0.0.0.0")
        .unwrap_or(false)
}

// The webui.env file content to write for a given LAN on/off choice.
pub fn format_webui_env(enabled: bool) -> String {
    let host = if enabled { "0.0.0.0" } else { "127.0.0.1" };
    format!("WEBUI_HOST={host}\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_full_details_from_ollamas_tags_response() {
        let json = r#"{
            "models": [{
                "name": "llama3.1:8b",
                "size": 4920753328,
                "modified_at": "2026-01-01T00:00:00Z",
                "details": { "parameter_size": "8B", "quantization_level": "Q4_0" }
            }]
        }"#;
        let parsed: TagsResponse = serde_json::from_str(json).unwrap();
        let info = to_model_info(parsed.models.into_iter().next().unwrap());

        assert_eq!(info.name, "llama3.1:8b");
        assert_eq!(info.size, 4920753328);
        assert_eq!(info.parameter_size, "8B");
        assert_eq!(info.quantization_level, "Q4_0");
    }

    #[test]
    fn defaults_to_empty_strings_when_details_is_missing() {
        let json = r#"{
            "models": [{
                "name": "custom-model:latest",
                "size": 123,
                "modified_at": "2026-01-01T00:00:00Z"
            }]
        }"#;
        let parsed: TagsResponse = serde_json::from_str(json).unwrap();
        let info = to_model_info(parsed.models.into_iter().next().unwrap());

        assert_eq!(info.parameter_size, "");
        assert_eq!(info.quantization_level, "");
    }

    #[test]
    fn parses_normal_systemctl_is_active_output() {
        assert_eq!(parse_systemctl_is_active("active\n"), "active");
        assert_eq!(parse_systemctl_is_active("inactive\n"), "inactive");
        assert_eq!(parse_systemctl_is_active("failed"), "failed");
    }

    #[test]
    fn falls_back_to_unknown_on_empty_systemctl_output() {
        assert_eq!(parse_systemctl_is_active(""), "unknown");
        assert_eq!(parse_systemctl_is_active("   \n"), "unknown");
    }

    #[test]
    fn builds_ipv4_lan_url_without_brackets() {
        let ip: std::net::IpAddr = "192.168.1.42".parse().unwrap();
        assert_eq!(build_lan_url(ip), "http://192.168.1.42:8080");
    }

    #[test]
    fn builds_ipv6_lan_url_with_brackets() {
        let ip: std::net::IpAddr = "fe80::1".parse().unwrap();
        assert_eq!(build_lan_url(ip), "http://[fe80::1]:8080");
    }

    #[test]
    fn parses_lan_enabled_from_webui_env() {
        assert!(parse_webui_lan_status("WEBUI_HOST=0.0.0.0\n"));
    }

    #[test]
    fn parses_lan_disabled_from_webui_env() {
        assert!(!parse_webui_lan_status("WEBUI_HOST=127.0.0.1\n"));
    }

    #[test]
    fn defaults_to_lan_disabled_when_file_is_empty_or_malformed() {
        assert!(!parse_webui_lan_status(""));
        assert!(!parse_webui_lan_status("SOME_OTHER_VAR=1\n"));
    }

    #[test]
    fn formats_lan_enabled_env_line() {
        assert_eq!(format_webui_env(true), "WEBUI_HOST=0.0.0.0\n");
    }

    #[test]
    fn formats_lan_disabled_env_line() {
        assert_eq!(format_webui_env(false), "WEBUI_HOST=127.0.0.1\n");
    }
}
