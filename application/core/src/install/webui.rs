//! Ported from `scripts/linux/04-install-webui.sh`.
//!
//! **One deliberate deviation from this branch's literal current
//! behavior, confirmed with the user before porting** (see this phase's
//! verification report): the script's pip fallback (used when `pipx` isn't
//! available) is `pip install --break-system-packages --upgrade
//! open-webui` unconditionally on `refactor/selfllama-restructure` today -
//! that flag is Debian-specific (PEP 668) and would be wrong on Arch/
//! Fedora/openSUSE. A distro-aware fix (`--break-system-packages` on
//! Debian, `--user` everywhere else) already exists on the unmerged
//! `fix/comprehensive-issues` branch; the user asked to port the corrected
//! version rather than faithfully reproduce a bug already known and
//! already fixed elsewhere in this repo. `pip_install_flags` below
//! implements the *corrected* behavior, not `refactor/selfllama-
//! restructure`'s current literal one.

use super::DistroFamily;

// ---------------------------------------------------------------------------
// install_webui_deps: python3/pip/pipx, skipped entirely if pipx is already
// present (the idempotency check that lets application::privileged's
// webui-deps step run through pkexec without ever needing to prompt for a
// second, separate sudo password - see CLAUDE.md's Desktop GUI section).
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DepsPlan {
    AlreadySatisfied,
    InstallPackages(Vec<&'static str>),
}

fn webui_dep_packages(distro: DistroFamily) -> Vec<&'static str> {
    match distro {
        DistroFamily::Arch => vec!["python", "python-pip", "pipx"],
        DistroFamily::Debian | DistroFamily::Fedora => vec!["python3", "python3-pip", "pipx"],
        DistroFamily::OpenSuse => vec!["python3", "python3-pip", "python3-pipx"],
        DistroFamily::Unknown => vec![],
    }
}

/// `pipx_on_path` is a fact the caller already has (`command -v pipx`) -
/// this function does not check itself.
pub fn deps_plan(distro: DistroFamily, pipx_on_path: bool) -> DepsPlan {
    if pipx_on_path {
        DepsPlan::AlreadySatisfied
    } else {
        DepsPlan::InstallPackages(webui_dep_packages(distro))
    }
}

// ---------------------------------------------------------------------------
// Installing/upgrading Open WebUI itself, and resolving WEBUI_BIN - both
// pipx and the pip fallback resolve WEBUI_BIN by running a real command
// (`pipx environment --value PIPX_BIN_DIR`, or `command -v open-webui`) at
// install time; this plan carries *how* to resolve it, not a value, since
// this function has no way to run either command itself.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PipInstallFlags {
    /// `pip install --break-system-packages --upgrade open-webui` (Debian).
    BreakSystemPackages,
    /// `pip install --user --upgrade open-webui` (every other distro) -
    /// the corrected behavior, see the module doc comment.
    User,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WebuiPackageInstall {
    /// `pipx ensurepath` (best effort, errors ignored), then `pipx install
    /// open-webui || pipx upgrade open-webui`. `WEBUI_BIN` resolves via
    /// `pipx environment --value PIPX_BIN_DIR`, with `/open-webui`
    /// appended.
    Pipx,
    /// `pip install <flags> --upgrade open-webui`. `WEBUI_BIN` resolves via
    /// `command -v open-webui`.
    Pip(PipInstallFlags),
}

/// `pipx_on_path` is the same fact `deps_plan` takes - by the time this
/// runs for real, `install_webui_deps` will have made it true unless
/// package installation itself failed, but this function still takes it as
/// an explicit input rather than assuming.
pub fn package_install_plan(distro: DistroFamily, pipx_on_path: bool) -> WebuiPackageInstall {
    if pipx_on_path {
        WebuiPackageInstall::Pipx
    } else {
        let flags = match distro {
            DistroFamily::Debian => PipInstallFlags::BreakSystemPackages,
            _ => PipInstallFlags::User,
        };
        WebuiPackageInstall::Pip(flags)
    }
}

// ---------------------------------------------------------------------------
// The systemd user unit and its EnvironmentFile - the point this phase's
// task explicitly flagged as needing careful reproduction: `${WEBUI_HOST}`
// must stay a *literal*, unexpanded placeholder in the generated unit file
// (systemd substitutes it from EnvironmentFile= at service-start time, not
// this code at generation time), while webui_bin/env_file_path are baked
// in for real.
// ---------------------------------------------------------------------------

pub const WEBUI_ENV_DEFAULT_CONTENT: &str = "WEBUI_HOST=127.0.0.1\n";
pub const OLLAMA_BASE_URL: &str = "http://127.0.0.1:11434";

/// `env_file_path` only gets `WEBUI_ENV_DEFAULT_CONTENT` written if it
/// doesn't already exist - `04-install-webui.sh`'s `if [ ! -f
/// "$WEBUI_ENV_FILE" ]` guard, so re-running the installer (e.g. to
/// upgrade Open WebUI) never resets a LAN-access choice made after install
/// via `toggle-webui-lan.sh`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WebuiEnvFilePlan {
    pub path: String,
    pub default_content: &'static str,
    pub only_if_missing: bool,
}

pub fn webui_env_file_plan(state_dir: &str) -> WebuiEnvFilePlan {
    WebuiEnvFilePlan {
        path: format!("{state_dir}/webui.env"),
        default_content: WEBUI_ENV_DEFAULT_CONTENT,
        only_if_missing: true,
    }
}

/// Renders the unit file content byte-for-byte the way
/// `04-install-webui.sh`'s heredoc does: `webui_bin`/`env_file_path` are
/// substituted for real (they're known at generation time, unlike in the
/// Bash script, where they're shell variables expanded by the heredoc
/// itself - same end result), but the literal string `${WEBUI_HOST}` in
/// `ExecStart=` is written out unexpanded on purpose, exactly matching the
/// Bash heredoc's `--host \${WEBUI_HOST}` (the `\` there escapes it out of
/// bash's own heredoc expansion) - systemd is the one that substitutes it,
/// from `EnvironmentFile=`, when the service actually starts.
pub fn render_unit_content(webui_bin: &str, env_file_path: &str) -> String {
    format!(
        "[Unit]\n\
         Description=Open WebUI (local web interface for Ollama)\n\
         After=network.target\n\
         \n\
         [Service]\n\
         EnvironmentFile={env_file_path}\n\
         Environment=\"OLLAMA_BASE_URL={OLLAMA_BASE_URL}\"\n\
         Environment=\"WEBUI_AUTH=False\"\n\
         ExecStart={webui_bin} serve --port 8080 --host ${{WEBUI_HOST}}\n\
         Restart=on-failure\n\
         \n\
         [Install]\n\
         WantedBy=default.target\n"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    // -- install_webui_deps: idempotency check --

    #[test]
    fn pipx_already_present_needs_no_package_install() {
        assert_eq!(deps_plan(DistroFamily::Arch, true), DepsPlan::AlreadySatisfied);
    }

    #[test]
    fn pipx_missing_installs_the_distro_specific_packages() {
        assert_eq!(
            deps_plan(DistroFamily::Debian, false),
            DepsPlan::InstallPackages(vec!["python3", "python3-pip", "pipx"])
        );
        assert_eq!(
            deps_plan(DistroFamily::OpenSuse, false),
            DepsPlan::InstallPackages(vec!["python3", "python3-pip", "python3-pipx"])
        );
    }

    // -- package install: the confirmed, corrected pip fallback --

    #[test]
    fn pipx_present_uses_pipx_not_pip() {
        assert_eq!(package_install_plan(DistroFamily::Debian, true), WebuiPackageInstall::Pipx);
    }

    #[test]
    fn pip_fallback_on_debian_uses_break_system_packages() {
        assert_eq!(
            package_install_plan(DistroFamily::Debian, false),
            WebuiPackageInstall::Pip(PipInstallFlags::BreakSystemPackages)
        );
    }

    #[test]
    fn pip_fallback_on_arch_fedora_opensuse_uses_user_not_break_system_packages() {
        // This is the exact point confirmed with the user before porting:
        // --break-system-packages is Debian/PEP-668-specific and would be
        // wrong here - refactor/selfllama-restructure's current literal
        // Bash applies it unconditionally, a known bug already fixed on
        // the unmerged fix/comprehensive-issues branch. This ports the
        // corrected behavior, not the current one.
        for distro in [DistroFamily::Arch, DistroFamily::Fedora, DistroFamily::OpenSuse, DistroFamily::Unknown] {
            assert_eq!(
                package_install_plan(distro, false),
                WebuiPackageInstall::Pip(PipInstallFlags::User),
                "{distro:?} should use --user, not --break-system-packages"
            );
        }
    }

    // -- webui.env: only-if-missing semantics --

    #[test]
    fn env_file_plan_defaults_to_localhost_only_and_never_overwrites() {
        let plan = webui_env_file_plan("/home/user/.config/selfllama");
        assert_eq!(plan.path, "/home/user/.config/selfllama/webui.env");
        assert_eq!(plan.default_content, "WEBUI_HOST=127.0.0.1\n");
        assert!(plan.only_if_missing, "must never reset an existing LAN-access choice");
    }

    // -- the systemd unit: ${WEBUI_HOST} must stay literal --

    #[test]
    fn unit_content_keeps_webui_host_as_a_literal_placeholder_for_systemd() {
        let content = render_unit_content("/home/user/.local/bin/open-webui", "/home/user/.config/selfllama/webui.env");

        assert!(
            content.contains("--host ${WEBUI_HOST}"),
            "WEBUI_HOST must reach the unit file unexpanded, for systemd's own EnvironmentFile= substitution at service-start time - got:\n{content}"
        );
        assert!(content.contains("EnvironmentFile=/home/user/.config/selfllama/webui.env"));
        assert!(content.contains("Environment=\"OLLAMA_BASE_URL=http://127.0.0.1:11434\""));
        assert!(content.contains("Environment=\"WEBUI_AUTH=False\""));
        assert!(content.contains("ExecStart=/home/user/.local/bin/open-webui serve --port 8080 --host ${WEBUI_HOST}"));
        assert!(content.contains("Restart=on-failure"));
        assert!(content.contains("[Install]\nWantedBy=default.target"));
    }

    #[test]
    fn unit_content_substitutes_webui_bin_and_env_file_path_for_real() {
        // Unlike ${WEBUI_HOST}, these two are NOT placeholders - they're
        // resolved values, baked into the rendered content directly.
        let content = render_unit_content("/opt/pipx/venvs/open-webui/bin/open-webui", "/custom/state/webui.env");
        assert!(content.contains("EnvironmentFile=/custom/state/webui.env"));
        assert!(content.contains("ExecStart=/opt/pipx/venvs/open-webui/bin/open-webui serve"));
    }
}
