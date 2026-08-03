//! RAM detection, ported from `detect_ram()` in `scripts/linux/lib/common.sh`.
//!
//! Windows RAM detection (`Get-RamGb` in `scripts/windows/lib/common.ps1`)
//! computes `Ceiling(TotalPhysicalMemory / 1GB)` directly from a CIM byte
//! count rather than parsing `free`'s text output - there is no equivalent
//! text format to parse there, so no Windows counterpart is ported here;
//! see this phase's report for the byte-count approach if it's ever needed
//! as a pure function too.

/// Parses the `Mem:` line's second column (total memory) out of `free -g`
/// or `free -m` output, mirroring `free -g | awk '/^Mem:/{print $2}'`.
/// Locale-dependent labels (e.g. French `Mem:` still reads `Mem:` in
/// `free`'s machine-oriented columns, only the header row is translated) are
/// not a concern here since the match is on the literal `Mem:` prefix `free`
/// always emits regardless of locale.
fn parse_free_total(output: &str) -> Option<u64> {
    output
        .lines()
        .find(|line| line.starts_with("Mem:"))
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|field| field.parse::<u64>().ok())
}

/// Port of `detect_ram()`'s exact fallback rule: `free -g` rounds down to
/// whole gigabytes, so a machine with less than 1 GB free -g reports 0 -
/// rather than reporting "0 GB" (which would wrongly zero out every model
/// tier), the Bash script falls back to `free -m`'s value, divided by 1024
/// and rounded *up* by adding 1 (integer division truncates, so `+ 1`
/// mirrors the Bash arithmetic `$(( m / 1024 + 1 ))` exactly, not a real
/// ceiling for exact multiples of 1024 MiB - same rounding quirk as Bash,
/// deliberately not "fixed" here since the goal is a faithful port).
pub fn compute_ram_gb(free_g_output: &str, free_m_output: &str) -> u64 {
    let gb = parse_free_total(free_g_output).unwrap_or(0);
    if gb == 0 {
        let mb = parse_free_total(free_m_output).unwrap_or(0);
        mb / 1024 + 1
    } else {
        gb
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FREE_G_15GB: &str = "\
               total       used        free      shared  buff/cache   available
Mem:              15           7           4           1           6           7
Swap:             19           1          18";

    const FREE_G_ZERO: &str = "\
               total       used        free      shared  buff/cache   available
Mem:               0           0           0           0           0           0
Swap:              0           0           0";

    const FREE_M_FOR_ZERO_G: &str = "\
               total        used        free      shared  buff/cache   available
Mem:             700         300         200          50         200         350
Swap:              0           0           0";

    #[test]
    fn uses_free_g_directly_when_nonzero() {
        assert_eq!(compute_ram_gb(FREE_G_15GB, "unused"), 15);
    }

    #[test]
    fn falls_back_to_free_m_when_free_g_rounds_to_zero() {
        // 700 MiB / 1024 = 0 (integer division), + 1 => 1, exactly the
        // Bash script's rounding rule for small/low-RAM machines.
        assert_eq!(compute_ram_gb(FREE_G_ZERO, FREE_M_FOR_ZERO_G), 1);
    }

    #[test]
    fn missing_mem_line_defaults_to_zero_then_falls_back() {
        assert_eq!(compute_ram_gb("", FREE_M_FOR_ZERO_G), 1);
    }

    #[test]
    fn both_outputs_missing_yields_one_not_zero() {
        // 0 / 1024 + 1 == 1: matches the Bash arithmetic even in this
        // degenerate case, rather than silently reporting 0 GB of RAM.
        assert_eq!(compute_ram_gb("", ""), 1);
    }
}
