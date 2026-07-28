# Arch Linux packaging (`PKGBUILD`)

Native Arch packages for `gui/` (`ollama-stack-gui`) and `launcher/` (`ollama-launcher`),
built from source and linked against the system's own `webkit2gtk-4.1`/`gtk3`, instead of
the copies bundled into the `.AppImage` release. This exists because the `.AppImage` was
found to crash on launch on at least one Arch-based system (EndeavourOS, AMD RDNA4 GPU)
with:

```
Could not create surfaceless EGL display: EGL_BAD_ALLOC. Aborting...
```

pointing at a mismatch between the AppImage's bundled `libwebkit2gtk-4.1` and the host's
Mesa/amdgpu stack. Building against the system's own libs, the way any other native Arch
package does, avoids that mismatch entirely. The `.deb`/`.rpm`/`.AppImage`/`.msi`/`.exe`
release artifacts (`.github/workflows/build-desktop.yml`) are untouched by this — this is
an additional, independent packaging path, not a replacement.

## Building and installing

Each app has its own `PKGBUILD` in its own subdirectory (`gui/`, `launcher/`) — install
either or both independently:

```bash
cd packaging/arch/gui       # or packaging/arch/launcher
makepkg -si
```

`makepkg` downloads the tagged release source tarball from GitHub (see
[Keeping this up to date](#keeping-this-up-to-date) below), builds the app with a plain
`cargo build --release --locked` (no `tauri-cli`/`cargo tauri build` involved — see the
comment at the top of each `PKGBUILD`'s `build()`), runs `cargo test --release --locked`
in `check()`, and installs the binary, a `.desktop` entry, and hicolor icons.

## How this was verified

Both `PKGBUILD`s have been run through a real `makepkg -s` inside an actual Arch Linux
environment (the official `archlinux:base-devel` container image, with a throwaway
unprivileged `builder` user created for the run — `makepkg` refuses to run as root): full
`build()` (`cargo build --release --locked`), `check()` (`cargo test --release --locked`),
and `package()` all completed successfully for both apps, producing a real
`.pkg.tar.zst`. For `gui` (the one with scripts to bundle), the resulting package's file
tree was inspected directly and confirmed correct: `/usr/bin/ollama-stack-gui` alongside
`/usr/lib/Ollama Stack GUI/scripts/` (all four numbered scripts, `lib/common.sh`,
`setup.ps1`, `lib/common.ps1`), the `.desktop` entry, and hicolor icons at all three
sizes — exactly the layout `find_scripts_dir()` in `gui/src-tauri/src/main.rs` looks for.
`.github/workflows/arch-package.yml` re-runs this exact `makepkg -s` check in CI (matrix
over `gui`/`launcher`) whenever either `PKGBUILD` changes, so this stays verified going
forward instead of being a one-off manual check.

One non-fatal `makepkg` warning showed up for both packages and is expected, not a bug:
`WARNING: Package contains reference to $srcdir` on the built binary — Rust embeds
compile-time (including panic-location) paths in binaries by default, which is common for
Rust-based AUR packages and not something this `PKGBUILD` attempts to suppress.

Still only checked inside a container, not installed for real via `pacman -U` on a
physical/VM Arch desktop (polkit/session/GPU behavior can't be verified from a headless
container) — that last step is on whoever installs this for real.

## Keeping this up to date

`pkgver`/`source`/`sha256sums` are pinned to a specific release tag (`v1.1.4` at the time
of writing) — there is no `-git`/VCS variant. On a new release:

1. Bump `pkgver` in both `PKGBUILD`s to the new tag's version (without the `v` prefix).
2. Update `sha256sums` — `updpkgsums` (from `pacman-contrib`) does this automatically, or
   compute it by hand: `curl -sL https://github.com/Mvth1s/ollama-configuration/archive/refs/tags/vX.Y.Z.tar.gz | sha256sum`.
3. Reset `pkgrel` to `1`.

## Known limitation

`gui/src-tauri/tauri.conf.json`/`launcher/src-tauri/tauri.conf.json`'s own committed
`version` field is not kept in sync with release tags between releases (see the
repo-root `CLAUDE.md`'s Releases section) — this only matters for `build-desktop.yml`'s
CI builds, which stamp the tag's version in ephemerally before building. It has no effect
here: this `PKGBUILD`'s `pkgver` is the only version that matters for the Arch package
itself, and is set independently in step 1 above.
