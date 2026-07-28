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

Neither `PKGBUILD` was run through a real `makepkg` — the environment this was prepared
in has no `pacman`/Arch toolchain available. Instead, every step `makepkg` would perform
was reproduced manually and confirmed to work:

- Downloaded the real `v1.1.4` release tarball from GitHub and computed its `sha256sum`
  (the value baked into both `PKGBUILD`s) — confirmed it extracts to
  `ollama-configuration-1.1.4/` with `gui/src-tauri/`, `launcher/src-tauri/`, the four
  numbered scripts, `lib/common.sh`, `setup.ps1`, `lib/common.ps1`, and `LICENSE` all
  present at the expected paths.
- Ran `cargo build --release --locked` for both `gui/src-tauri` and `launcher/src-tauri`
  from that pristine tarball extract (i.e. not the existing repo checkout's own
  `target/`) — both compiled cleanly end to end.
- Ran `cargo test --release --locked` for both — all tests pass (`gui`: 6/6; `launcher`:
  6/6 plus the one `#[ignore]`d live-Ollama integration test, correctly skipped by
  default).
- Ran `ldd` against both resulting binaries — no unresolved shared libraries (this
  sandbox's `webkit2gtk-4.1`/`gtk3` dev packages happen to be close enough to Arch's own
  to confirm dynamic linking works the same way, though the actual runtime `depends=()`
  package names are Arch's, confirmed against the real Arch package database via
  `archlinux.org`'s package search API rather than guessed).
- Manually replicated every `install -D...` line from both `package()` functions against
  a throwaway fake root and confirmed the resulting file tree, `.desktop` file contents,
  and — for `gui`, the one with scripts to bundle — that the installed layout
  (`/usr/bin/ollama-stack-gui` + `/usr/lib/Ollama Stack GUI/scripts/...`) matches exactly
  what Tauri's own `resource_dir()` resolves to at runtime for a binary installed at
  `/usr/bin` (`tauri-utils`' `resource_dir_from()`, read directly from the Tauri v2.9.3
  source): `<exe_dir>/../lib/<productName>`, where `productName` ("Ollama Stack GUI",
  with the literal space) comes from `gui/src-tauri/tauri.conf.json`, not the crate name.
  This is the same resource lookup `find_scripts_dir()` in `gui/src-tauri/src/main.rs`
  already uses for the `.deb`/`.rpm` bundles — this `PKGBUILD` reproduces that exact
  layout by hand instead of changing any app code.

Not yet run through a real `makepkg -si`/`pacman -U` — do that on your own Arch machine
to confirm before relying on this.

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
