//! Model-tier selection, ported from `compute_tier()` and the
//! `MODEL_<TIER>`/`CAND_<TIER>_<usage>` tables in
//! `scripts/linux/03-pull-models.sh`. Previously flagged as out of scope
//! for `core::detect` (see that module's own doc comment: "a decision
//! built on top of RAM/GPU detection, not detection itself") - it lives
//! here in `install::` instead, now that a real caller (`gui/src-tauri`'s
//! `detect_system`) needs it directly rather than shelling out to
//! `03-pull-models.sh --detect-only`.
//!
//! **Kept in sync by hand with `03-pull-models.sh`'s tables** - same
//! convention as `detect::gpu::amd_gfx_override_map` and `setup.ps1`'s own
//! `$ModelTiers` - update both places when changing a model. Windows has no
//! counterpart to port here: `setup.ps1`'s `$ModelTiers`/`Get-ModelTier`
//! stay PowerShell-native, unchanged by this module.

use serde::Serialize;
use std::collections::HashMap;

pub const USAGES: [&str; 4] = ["texte", "code", "reflexion", "embeddings"];

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ModelCandidate {
    pub model: String,
    pub desc: String,
}

/// Port of `compute_tier()`'s exact logic: a forced tier (`--tier=`) is
/// returned as-is, bypassing the RAM thresholds and the CPU-only downgrade
/// below entirely - same as the Bash script, where `FORCE_TIER` short-
/// circuits before either is ever evaluated.
pub fn compute_tier(ram_gb: u64, gpu_vendor: &str, forced: Option<&str>) -> String {
    if let Some(t) = forced {
        return t.to_string();
    }

    let mut tier = if ram_gb <= 8 {
        "XS"
    } else if ram_gb <= 16 {
        "S"
    } else if ram_gb <= 32 {
        "M"
    } else {
        "L"
    };

    // CPU only (no AMD, Nvidia, or active Intel Vulkan): a 12b+ model
    // becomes too slow in practice, so we drop down to S regardless of raw
    // RAM tier - exact mirror of the Bash script's own comment.
    if gpu_vendor == "none" && (tier == "M" || tier == "L") {
        tier = "S";
    }

    tier.to_string()
}

/// Port of the `MODEL_<TIER>` associative arrays: the single default model
/// per usage for a given tier. `None` for any tier string other than the
/// four known ones - mirrors the Bash script's `declare -n
/// tier_models="MODEL_${TIER}"`, which fails at runtime for anything else.
pub fn default_models(tier: &str) -> Option<HashMap<String, String>> {
    let pairs: [(&str, &str); 4] = match tier {
        "XS" => [
            ("texte", "llama3.2:3b"),
            ("code", "qwen2.5-coder:3b"),
            ("reflexion", "deepseek-r1:1.5b"),
            ("embeddings", "nomic-embed-text"),
        ],
        "S" => [
            ("texte", "llama3.1:8b"),
            ("code", "qwen2.5-coder:7b"),
            ("reflexion", "deepseek-r1:7b"),
            ("embeddings", "nomic-embed-text"),
        ],
        "M" => [
            ("texte", "gemma3:12b"),
            ("code", "devstral:24b"),
            ("reflexion", "deepseek-r1:14b"),
            ("embeddings", "nomic-embed-text"),
        ],
        "L" => [
            ("texte", "gemma3:27b"),
            ("code", "qwen2.5-coder:32b"),
            ("reflexion", "deepseek-r1:32b"),
            ("embeddings", "nomic-embed-text"),
        ],
        _ => return None,
    };
    Some(pairs.into_iter().map(|(k, v)| (k.to_string(), v.to_string())).collect())
}

/// Port of the `CAND_<TIER>_<usage>` arrays: the interactive-picker
/// candidates for a given tier, keyed by usage. `None` for an unrecognized
/// tier, same convention as `default_models`. The first candidate for each
/// usage always matches that usage's `default_models` entry, same
/// invariant the Bash script's own comment documents.
pub fn candidates(tier: &str) -> Option<HashMap<String, Vec<ModelCandidate>>> {
    let raw: [(&str, &[(&str, &str)]); 4] = match tier {
        "XS" => [
            (
                "texte",
                &[
                    ("llama3.2:3b", "Fast, solid generalist for low-end hardware"),
                    ("qwen2.5:3b", "Multilingual alternative"),
                    ("phi3.5:3.8b", "Compact, decent basic reasoning"),
                ][..],
            ),
            (
                "code",
                &[
                    ("qwen2.5-coder:3b", "Lightweight general-purpose coding model"),
                    ("starcoder2:3b", "Alternative geared toward completion"),
                ][..],
            ),
            (
                "reflexion",
                &[
                    ("deepseek-r1:1.5b", "Step-by-step reasoning, very lightweight"),
                    ("qwen2.5:1.5b", "Lightweight generalist alternative"),
                ][..],
            ),
            (
                "embeddings",
                &[
                    ("nomic-embed-text", "Standard general-purpose embeddings"),
                    ("all-minilm", "Lighter, faster"),
                ][..],
            ),
        ],
        "S" => [
            (
                "texte",
                &[
                    ("llama3.1:8b", "Well-balanced generalist"),
                    ("gemma2:9b", "Google alternative, good instruction following"),
                    ("mistral:7b", "Fast, good tradeoff"),
                ][..],
            ),
            (
                "code",
                &[
                    ("qwen2.5-coder:7b", "General-purpose coding model"),
                    ("codellama:7b", "Meta alternative, geared toward completion"),
                ][..],
            ),
            (
                "reflexion",
                &[
                    ("deepseek-r1:7b", "Step-by-step reasoning"),
                    ("qwen2.5:7b", "Generalist alternative"),
                ][..],
            ),
            (
                "embeddings",
                &[
                    ("nomic-embed-text", "Standard general-purpose embeddings"),
                    ("all-minilm", "Lighter, faster"),
                ][..],
            ),
        ],
        "M" => [
            (
                "texte",
                &[
                    ("gemma3:12b", "Recent Google generalist"),
                    ("mistral-nemo:12b", "Mistral/Nvidia alternative"),
                    ("qwen2.5:14b", "Bigger, better general reasoning"),
                ][..],
            ),
            (
                "code",
                &[
                    ("devstral:24b", "Geared toward coding agents"),
                    ("qwen2.5-coder:14b", "Lighter alternative"),
                ][..],
            ),
            (
                "reflexion",
                &[
                    ("deepseek-r1:14b", "Step-by-step reasoning"),
                    ("qwen2.5:14b", "Generalist alternative"),
                ][..],
            ),
            (
                "embeddings",
                &[
                    ("nomic-embed-text", "Standard general-purpose embeddings"),
                    ("mxbai-embed-large", "More accurate, heavier"),
                ][..],
            ),
        ],
        "L" => [
            (
                "texte",
                &[
                    ("gemma3:27b", "Large Google generalist"),
                    ("qwen2.5:32b", "Alibaba alternative"),
                    ("mixtral:8x7b", "Mixture-of-experts, good speed/quality tradeoff"),
                ][..],
            ),
            (
                "code",
                &[
                    ("qwen2.5-coder:32b", "Large general-purpose coding model"),
                    ("devstral:24b", "Alternative geared toward coding agents"),
                ][..],
            ),
            (
                "reflexion",
                &[
                    ("deepseek-r1:32b", "Large step-by-step reasoning model"),
                    ("qwq:32b", "Alibaba alternative geared toward reasoning"),
                ][..],
            ),
            (
                "embeddings",
                &[
                    ("nomic-embed-text", "Standard general-purpose embeddings"),
                    ("mxbai-embed-large", "More accurate, heavier"),
                ][..],
            ),
        ],
        _ => return None,
    };

    Some(
        raw.into_iter()
            .map(|(usage, cands)| {
                (
                    usage.to_string(),
                    cands.iter().map(|(model, desc)| ModelCandidate { model: model.to_string(), desc: desc.to_string() }).collect(),
                )
            })
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ram_thresholds_match_the_bash_script_exactly() {
        assert_eq!(compute_tier(8, "nvidia", None), "XS");
        assert_eq!(compute_tier(9, "nvidia", None), "S");
        assert_eq!(compute_tier(16, "nvidia", None), "S");
        assert_eq!(compute_tier(17, "nvidia", None), "M");
        assert_eq!(compute_tier(32, "nvidia", None), "M");
        assert_eq!(compute_tier(33, "nvidia", None), "L");
    }

    #[test]
    fn cpu_only_downgrades_m_and_l_to_s_but_leaves_xs_alone() {
        assert_eq!(compute_tier(20, "none", None), "S");
        assert_eq!(compute_tier(64, "none", None), "S");
        assert_eq!(compute_tier(8, "none", None), "XS");
    }

    #[test]
    fn a_dedicated_gpu_never_triggers_the_downgrade() {
        assert_eq!(compute_tier(20, "amd", None), "M");
        assert_eq!(compute_tier(64, "intel", None), "L");
    }

    #[test]
    fn a_forced_tier_bypasses_ram_and_the_downgrade_rule_entirely() {
        // Would otherwise downgrade to S on RAM alone (>32) and again on
        // CPU-only, but a forced tier short-circuits both, exactly like
        // the Bash script's FORCE_TIER branch.
        assert_eq!(compute_tier(64, "none", Some("L")), "L");
    }

    #[test]
    fn default_models_xs_matches_the_bash_table() {
        let models = default_models("XS").unwrap();
        assert_eq!(models.get("texte").unwrap(), "llama3.2:3b");
        assert_eq!(models.get("code").unwrap(), "qwen2.5-coder:3b");
        assert_eq!(models.get("reflexion").unwrap(), "deepseek-r1:1.5b");
        assert_eq!(models.get("embeddings").unwrap(), "nomic-embed-text");
    }

    #[test]
    fn default_models_covers_every_tier_with_all_four_usages() {
        for tier in ["XS", "S", "M", "L"] {
            let models = default_models(tier).unwrap();
            assert_eq!(models.len(), 4);
            for usage in USAGES {
                assert!(models.contains_key(usage), "tier {tier} missing usage {usage}");
            }
        }
    }

    #[test]
    fn unknown_tier_yields_no_default_models() {
        assert_eq!(default_models("XL"), None);
    }

    #[test]
    fn every_candidate_lists_first_entry_matching_the_default_model() {
        for tier in ["XS", "S", "M", "L"] {
            let defaults = default_models(tier).unwrap();
            let cands = candidates(tier).unwrap();
            for usage in USAGES {
                let first = &cands.get(usage).unwrap()[0].model;
                assert_eq!(first, defaults.get(usage).unwrap(), "tier {tier} usage {usage}");
            }
        }
    }

    #[test]
    fn unknown_tier_yields_no_candidates() {
        assert_eq!(candidates("XL"), None);
    }
}
