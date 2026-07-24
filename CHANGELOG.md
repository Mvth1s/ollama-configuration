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
