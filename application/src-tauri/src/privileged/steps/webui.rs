//! Executes `core::install::webui::deps_plan` for real - and **only**
//! that plan. `application::privileged`'s "webui-deps" step matches
//! `04-install-webui.sh --install-deps` exactly (see Phase 4/CLAUDE.md's
//! Desktop GUI section): the *prerequisites* only (python3/pip/pipx),
//! which is genuinely the privileged part (`pkg_install` needs root).
//!
//! `core::install::webui`'s other functions - `package_install_plan`
//! (`pipx install open-webui` / the pip fallback), `webui_env_file_plan`,
//! `render_unit_content` - belong to the *unprivileged* run of
//! `04-install-webui.sh` (installing Open WebUI itself into the user's own
//! `~/.local`/`pipx` environment, writing `~/.config/systemd/user/...`),
//! which was never one of `application::privileged`'s three steps and
//! still isn't - `gui/src-tauri` calls that unprivileged script directly,
//! outside `pkexec` entirely, unaffected by any of this restructure.
//! Wiring those up is therefore not this phase's job.

use super::super::exec::{command_exists, pkg_install_commands, read_os_release, run_commands};
use super::StepRunError;
use core::detect::distro::parse_distro;
use core::install::webui::{deps_plan, DepsPlan};
use core::install::DistroFamily;

pub fn run() -> Result<(), StepRunError> {
    let distro_info = parse_distro(read_os_release().as_deref());
    let distro = DistroFamily::from(distro_info.family.as_str());
    let pipx_on_path = command_exists("pipx");

    match deps_plan(distro, pipx_on_path) {
        DepsPlan::AlreadySatisfied => {
            println!("pipx already present, nothing to install.");
            Ok(())
        }
        DepsPlan::InstallPackages(packages) => {
            run_commands(pkg_install_commands(distro, &packages)).map_err(StepRunError::Failed)
        }
    }
}
