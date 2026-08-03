//! CPU model/thread-count detection, ported from `detect_cpu()` in
//! `scripts/linux/lib/common.sh`. Display-only: no tier/config decision
//! depends on this, same as in the Bash script.
//!
//! Windows's `Get-CpuInfo` (`scripts/windows/lib/common.ps1`) reads these
//! directly off a CIM `Win32_Processor` object (`.Name`,
//! `.NumberOfLogicalProcessors`), so there is no text format to parse there
//! either - nothing to port for Windows in this module, unlike GPU vendor
//! detection which does have a comparable Windows text format
//! (`PNPDeviceID`).

/// Port of `awk -F': ' '/^model name/{print $2; exit}' /proc/cpuinfo`: first
/// `model name` line, split once on `: `, trimmed. `"unknown"` when absent,
/// matching the Bash script's `[ -z "$CPU_MODEL" ] && CPU_MODEL="unknown"`.
pub fn parse_cpu_model(proc_cpuinfo: &str) -> String {
    proc_cpuinfo
        .lines()
        .find(|line| line.starts_with("model name"))
        .and_then(|line| line.split_once(": "))
        .map(|(_, value)| value.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "unknown".to_string())
}

/// Port of `nproc 2>/dev/null || echo 1`: parses a plain integer, falling
/// back to `1` on anything unparseable (missing command, empty output,
/// garbage), exactly like the Bash `||` fallback.
pub fn parse_cpu_threads(nproc_output: &str) -> u32 {
    nproc_output.trim().parse::<u32>().unwrap_or(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    const CPUINFO_I5: &str = "\
processor\t: 0
vendor_id\t: GenuineIntel
model name\t: 11th Gen Intel(R) Core(TM) i5-1145G7 @ 2.60GHz
cpu MHz\t\t: 2600.000

processor\t: 1
vendor_id\t: GenuineIntel
model name\t: 11th Gen Intel(R) Core(TM) i5-1145G7 @ 2.60GHz
cpu MHz\t\t: 2600.000";

    #[test]
    fn extracts_model_name_from_first_processor_entry() {
        assert_eq!(parse_cpu_model(CPUINFO_I5), "11th Gen Intel(R) Core(TM) i5-1145G7 @ 2.60GHz");
    }

    #[test]
    fn missing_model_name_line_defaults_to_unknown() {
        assert_eq!(parse_cpu_model("processor\t: 0\nvendor_id\t: GenuineIntel"), "unknown");
    }

    #[test]
    fn empty_cpuinfo_defaults_to_unknown() {
        assert_eq!(parse_cpu_model(""), "unknown");
    }

    #[test]
    fn parses_thread_count() {
        assert_eq!(parse_cpu_threads("8\n"), 8);
    }

    #[test]
    fn unparseable_thread_count_defaults_to_one() {
        assert_eq!(parse_cpu_threads(""), 1);
        assert_eq!(parse_cpu_threads("nproc: command not found\n"), 1);
    }
}
