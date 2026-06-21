# ollama-configuration

Bash scripts to install and configure a local [Ollama](https://ollama.com) stack on Linux, with automatic GPU and RAM detection. Installs Ollama, configures GPU acceleration, downloads a set of models suited to the machine, and deploys [Open WebUI](https://github.com/open-webui/open-webui) as a web interface.

## Requirements

- Linux (Arch, Debian/Ubuntu, Fedora, openSUSE — other distros require manual GPU driver installation)
- `curl`, `bash` ≥ 4.0 (for associative arrays)
- `sudo` privileges for package installation and systemd configuration

## Quick start

```bash
git clone https://github.com/Mvth1s/ollama-configuration.git
cd ollama-configuration
./setup.sh
```

The web interface is then available at **http://localhost:8080**.

## Options

```bash
./setup.sh                   # full install, auto-detection
./setup.sh --tier=M          # force a specific model tier (XS / S / M / L)
./setup.sh --skip-models     # install Ollama + GPU + WebUI without models
./setup.sh --skip-webui      # skip Open WebUI installation
```

## Individual steps

Each script can be re-run on its own without reinstalling everything:

```bash
./01-install-ollama.sh                   # install Ollama and start the service
./02-configure-gpu.sh                    # detect GPU and configure acceleration
./03-pull-models.sh [--tier=XS|S|M|L]   # download models
./04-install-webui.sh                    # install Open WebUI
```

## Model tiers

The tier is chosen automatically based on available RAM and can be overridden with `--tier=`.

| Tier | RAM | Text | Code | Reasoning | Embeddings |
|------|-----|------|------|-----------|------------|
| XS | ≤ 8 GB | llama3.2:3b | qwen2.5-coder:3b | deepseek-r1:1.5b | nomic-embed-text |
| S | ≤ 16 GB | llama3.1:8b | qwen2.5-coder:7b | deepseek-r1:7b | nomic-embed-text |
| M | ≤ 32 GB | gemma3:12b | devstral:24b | deepseek-r1:14b | nomic-embed-text |
| L | > 32 GB | gemma3:27b | qwen2.5-coder:32b | deepseek-r1:32b | nomic-embed-text |

On CPU-only machines (no dedicated GPU), the tier is capped at S regardless of RAM.

## GPU support

| Vendor | Backend | Notes |
|--------|---------|-------|
| Nvidia | CUDA | No per-card configuration needed. |
| AMD | ROCm/HIP | Automatically falls back to Vulkan + `HSA_OVERRIDE_GFX_VERSION` for RDNA4 chips not yet officially supported by ROCm. |
| Intel | Vulkan (Mesa ANV) | Best effort — covers Xe/Iris iGPUs and Arc dedicated GPUs. Falls back to CPU silently if unsupported. |
| None | CPU | No configuration needed. |

## Useful commands after installation

```bash
ollama list                            # list installed models
ollama run llama3.1:8b                 # run a model from the terminal
systemctl status ollama                # Ollama service status
systemctl --user status open-webui     # Open WebUI service status
```

To start Open WebUI without an active user session:
```bash
sudo loginctl enable-linger $USER
```

## License

[MIT](LICENSE)
