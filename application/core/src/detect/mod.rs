//! System detection: GPU, RAM, CPU, distro. Every function in these
//! submodules is pure (string/data in, parsed result out) - none of them
//! spawn a process or touch the filesystem. See each submodule's own doc
//! comment for exactly which Bash/PowerShell function it ports.
//!
//! Deliberately out of scope here (see this phase's report): model tier
//! selection (`compute_tier` in `scripts/linux/03-pull-models.sh`) and the
//! candidate-model tables. Those are a model-selection *decision* built on
//! top of RAM/GPU detection, not detection itself, and belong with a future
//! `core::install` rather than `core::detect`.

pub mod cpu;
pub mod distro;
pub mod gpu;
pub mod ram;
