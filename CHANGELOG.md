# [1.3.0](https://github.com/Mvth1s/ollama-configuration/compare/v1.2.0...v1.3.0) (2026-08-03)


### Bug Fixes

* **ci:** switch msedgedriver download host, azureedge.net is dead ([3ed702e](https://github.com/Mvth1s/ollama-configuration/commit/3ed702eee3e679f7f8bb66943aba082981c39c84))
* **e2e:** add webviewOptions to tauri:options for msedgedriver ([a988956](https://github.com/Mvth1s/ollama-configuration/commit/a988956138096fac5dc6c4488ebc226a0d45677e)), closes [tauri-apps/tauri#12381](https://github.com/tauri-apps/tauri/issues/12381)
* **gpu:** find rocminfo outside PATH; document untested iGPU/Arc split ([d787417](https://github.com/Mvth1s/ollama-configuration/commit/d787417f8eacb4f6680ffc42bb71b3b0d97b7f6d))
* resolve bugs, accessibility gaps, and consistency issues across the project ([05bb630](https://github.com/Mvth1s/ollama-configuration/commit/05bb63083e105c8232b99428def5b55f4d796384))
* windows compile break in application/src-tauri, PKGBUILD paths, msedgedriver version match ([b358ee8](https://github.com/Mvth1s/ollama-configuration/commit/b358ee8b9b0f69f273ac44d14ffcc8fcee2ee908))


### Features

* **application:** add application/core, a tauri-free detection crate ([d39cf05](https://github.com/Mvth1s/ollama-configuration/commit/d39cf0541c622737d068cd39ebe9a272665478c9))
* **application:** add application/src-tauri and application::privileged ([124617b](https://github.com/Mvth1s/ollama-configuration/commit/124617bf001579cca505216784dd1f4587d2bcee))
* **application:** add core::install, pure install plans for ollama/gpu/webui ([0cf8511](https://github.com/Mvth1s/ollama-configuration/commit/0cf8511d1d8bc079bf1259b2374e03eb6108c37e))
* **application:** wire application::privileged to core::install's real plans ([10667b8](https://github.com/Mvth1s/ollama-configuration/commit/10667b872f79bb06e794497c7cd77651cf1cc2f8))
* apply the SelfLlama rebrand across product-facing identifiers ([53f7e52](https://github.com/Mvth1s/ollama-configuration/commit/53f7e52952c684fd2e6dd858c2716182040b7190))
* **ci:** add an e2e-windows WebDriver job alongside e2e-linux ([72f2831](https://github.com/Mvth1s/ollama-configuration/commit/72f2831a3d766edc2115e98d66693a3a0b008616))
* **gui:** wire detect_system to selfllama_core directly on Linux ([9e0bbe3](https://github.com/Mvth1s/ollama-configuration/commit/9e0bbe3eab69bc78936f19ec174f6ddffb1b8148))
* **gui:** wire gui/src-tauri to application::privileged, one pkexec call ([c314041](https://github.com/Mvth1s/ollama-configuration/commit/c314041ec8ce93bedc44bf0be37c5ea21f8bf54b))
* **privileged:** add a silent-phase indicator for pacman/dnf/zypper ([d105b51](https://github.com/Mvth1s/ollama-configuration/commit/d105b515556ce82ae79d95a8b225daa6cca9ff29))
* **windows:** add model-picker candidates parity with Linux ([2b95c64](https://github.com/Mvth1s/ollama-configuration/commit/2b95c6434f6daa6a71588eb925818bdfaca43a15))


### Reverts

* **e2e:** drop e2e-windows, three targeted fixes did not resolve it ([c886075](https://github.com/Mvth1s/ollama-configuration/commit/c8860759173bcb706514a72708fa34929ac57548))

# [1.2.0](https://github.com/Mvth1s/ollama-configuration/compare/v1.1.4...v1.2.0) (2026-07-28)


### Bug Fixes

* **gui:** stream ollama pull's ANSI-redrawn progress instead of buffering it ([add58f4](https://github.com/Mvth1s/ollama-configuration/commit/add58f4fe028e72e17f3c442b23ad145edc97638))


### Features

* **packaging:** add Arch Linux PKGBUILDs for gui and launcher ([fd28179](https://github.com/Mvth1s/ollama-configuration/commit/fd28179de04bcd6df9f613f3509f48b96dd31a5b))

## [1.1.4](https://github.com/Mvth1s/ollama-configuration/compare/v1.1.3...v1.1.4) (2026-07-28)


### Bug Fixes

* **gui:** stop the GUI's webui step from hanging on a hidden sudo call ([e6e9d0c](https://github.com/Mvth1s/ollama-configuration/commit/e6e9d0c4492ff6514c386aa391c98a0ea02faaea))

## [1.1.3](https://github.com/Mvth1s/ollama-configuration/compare/v1.1.2...v1.1.3) (2026-07-27)


### Bug Fixes

* **gui:** bundle setup.sh itself, missing from the resource list ([e5cbb2c](https://github.com/Mvth1s/ollama-configuration/commit/e5cbb2cda2e253a31f925a2ae9287a478a3b8304)), closes [#49](https://github.com/Mvth1s/ollama-configuration/issues/49)

## [1.1.2](https://github.com/Mvth1s/ollama-configuration/compare/v1.1.1...v1.1.2) (2026-07-27)


### Bug Fixes

* **release:** run the version-sync step under bash on windows-latest too ([a987b30](https://github.com/Mvth1s/ollama-configuration/commit/a987b30d06c22a9ca3b865a1eb73b4f13cf75856)), closes [#48](https://github.com/Mvth1s/ollama-configuration/issues/48)

## [1.1.1](https://github.com/Mvth1s/ollama-configuration/compare/v1.1.0...v1.1.1) (2026-07-27)


### Bug Fixes

* **gui:** bundle the install scripts as Tauri resources ([f684143](https://github.com/Mvth1s/ollama-configuration/commit/f684143818de9384f3fd4edd37bd4e88285e46b3))
* **release:** stop clobbering release notes, unify asset names, sync app version ([e6b63d0](https://github.com/Mvth1s/ollama-configuration/commit/e6b63d0b4a40fd896fef53b16c05639c6efe8187))

# [1.1.0](https://github.com/Mvth1s/ollama-configuration/compare/v1.0.1...v1.1.0) (2026-07-27)


### Bug Fixes

* **gui,launcher:** re-encode icon PNGs as 8-bit to fix the Windows build ([4a20b8a](https://github.com/Mvth1s/ollama-configuration/commit/4a20b8aee9202bf0c368f2da63e3c4bc2524538e))
* **release:** bump release.yml to Node 24, sync dev's package.json/lock ([f973fb9](https://github.com/Mvth1s/ollama-configuration/commit/f973fb90aaad4e7a4683904e75de5c3d7ff3e976)), closes [#45](https://github.com/Mvth1s/ollama-configuration/issues/45) [#39](https://github.com/Mvth1s/ollama-configuration/issues/39)


### Features

* **docs:** add a direct-download section to the showcase site ([b136b98](https://github.com/Mvth1s/ollama-configuration/commit/b136b9853732cfb791f66dc0151cb70bb16abfcd)), closes [#download](https://github.com/Mvth1s/ollama-configuration/issues/download)

## [1.0.1](https://github.com/Mvth1s/ollama-configuration/compare/v1.0.0...v1.0.1) (2026-07-24)


### Bug Fixes

* **release:** explicitly dispatch build-desktop.yml after a release ([7869d55](https://github.com/Mvth1s/ollama-configuration/commit/7869d555bca1aee219c1316564cd89eac3a05e52))

# 1.0.0 (2026-07-24)


### Bug Fixes

* **ci:** pass PSScriptAnalyzer -Path one file at a time ([3cfd361](https://github.com/Mvth1s/ollama-configuration/commit/3cfd361f15c4b72bcd6ce64a67a941745ac2cf0c))
* **common:** retry distro detection when a stale state.env cached "unknown" ([5ddbdc1](https://github.com/Mvth1s/ollama-configuration/commit/5ddbdc1c59f66cec665d60bb6a05b66bb2f49159))
* detach pkexec child from controlling tty in gui/ ([72bc0c5](https://github.com/Mvth1s/ollama-configuration/commit/72bc0c56a915a0743b93f3b5c29e7bbc016bcfd2))
* **docs,gui,launcher:** unify visual identity around the llama icon ([4cdfd39](https://github.com/Mvth1s/ollama-configuration/commit/4cdfd39e3cb33d520a162ea5fea7f33e6348ed4f))
* **docs:** refresh stale distro screenshot, prep Vercel deploy for the showcase site ([44d2dbb](https://github.com/Mvth1s/ollama-configuration/commit/44d2dbb2f260a43890840c96b8423a0d307145c6))
* enable iGPU in Intel Vulkan drop-in (OLLAMA_IGPU_ENABLE=1) ([046da40](https://github.com/Mvth1s/ollama-configuration/commit/046da402039ca08d2705bca62185d81c2dd67690))
* **gpu:** stop configure_nvidia and write_amd_override aborting under set -e ([5163aad](https://github.com/Mvth1s/ollama-configuration/commit/5163aadd5e588a589a4f062f8f9cc345ddf6bc54))
* grant contents:write permission to build-desktop.yml ([f55ee09](https://github.com/Mvth1s/ollama-configuration/commit/f55ee09eb7d917bfd0e46d9332cc19fea56e1798))
* **gui:** add missing entrance animations for steps 2-4 ([153d6ff](https://github.com/Mvth1s/ollama-configuration/commit/153d6ff88304cf5c43289c2d99c1ed9ba07b1b8e))
* match GPU vendor on PCI IDs instead of commercial names ([a2b1b73](https://github.com/Mvth1s/ollama-configuration/commit/a2b1b730d4adb03047971456da43854c635ec970))
* **release:** relax commitlint body/footer line-length for release commits ([8b44b6b](https://github.com/Mvth1s/ollama-configuration/commit/8b44b6b6113ad7e9b2387ce3f1f6fa23a5fc1b88))
* set a restrictive CSP in the Tauri apps instead of disabling it ([d510d96](https://github.com/Mvth1s/ollama-configuration/commit/d510d9665af88b6fcae2f4e14bb3fb687a892973))
* translate TUI candidate descriptions and menu text to English ([51948bd](https://github.com/Mvth1s/ollama-configuration/commit/51948bd15a6f03fa925279f6f5abecc214e1fce2)), closes [#2](https://github.com/Mvth1s/ollama-configuration/issues/2)
* **windows:** rename Set-ModelOverrides to Set-ModelOverride (PSUseSingularNouns) ([9f8a6b7](https://github.com/Mvth1s/ollama-configuration/commit/9f8a6b765135cd37d9374e69212449b201041d42))


### Features

* add a separate Ollama Launcher app for day-to-day use ([24ff923](https://github.com/Mvth1s/ollama-configuration/commit/24ff923867ed1bf3b33bf700e0a7235b7d26fcfb))
* add native Windows installer (setup.ps1 + lib/common.ps1) ([8400d4b](https://github.com/Mvth1s/ollama-configuration/commit/8400d4bf3ae8f53a33ab39612b4f7e2e04a68177))
* add Open WebUI window to the Tauri GUI ([969dff6](https://github.com/Mvth1s/ollama-configuration/commit/969dff6e0e1bb2986934ebf6eb0772f34a9a2cca))
* add optional dialog/whiptail TUI for GPU and model selection ([54efd0e](https://github.com/Mvth1s/ollama-configuration/commit/54efd0ec8bbf883403bc8135b5a0630887641702))
* add release automation (semantic-release, commitlint, tauri bundles) ([e229e00](https://github.com/Mvth1s/ollama-configuration/commit/e229e0046a52659f5225aead80e89bd68edfc8d2))
* add Tauri desktop GUI orchestrating setup.sh/setup.ps1 ([5fe3ff7](https://github.com/Mvth1s/ollama-configuration/commit/5fe3ff7033ee2ab9674fd0ebc48ad896c2e6ec7e))
* ajout des fichiers de scripts d'installation et de configuration pour ollama ([1f3a819](https://github.com/Mvth1s/ollama-configuration/commit/1f3a819ed02eca422c0a9963d56ee3207f175797))
* **docs:** add SEO/GEO to the showcase site ahead of its actual deploy ([c06fabd](https://github.com/Mvth1s/ollama-configuration/commit/c06fabd4df07066fd5d5494b75a15261fe9e4a40))
* **gui,launcher:** redesign installer wizard and launcher from design mockup ([88cfa38](https://github.com/Mvth1s/ollama-configuration/commit/88cfa381b17ce8d2f9137155003624ad8fdb7889))
* **gui:** add a step indicator to the installer ([496a2b4](https://github.com/Mvth1s/ollama-configuration/commit/496a2b4fc57d5e1ece0456b3216468efb82a47d7))
* **launcher:** expose the Open WebUI LAN toggle in the UI ([03cfd88](https://github.com/Mvth1s/ollama-configuration/commit/03cfd88239adac7e3d571b0f8dd747bf93bf40e4))
* restrict Open WebUI to localhost by default with a LAN access toggle ([e654943](https://github.com/Mvth1s/ollama-configuration/commit/e654943efd50fcb65ea8c637bd8a9f8d84877898))
* **windows:** add -Model<Usage> overrides to setup.ps1 ([3905597](https://github.com/Mvth1s/ollama-configuration/commit/3905597a1fcb4c5fab7714d3299d2bc4f9d6de2e))
