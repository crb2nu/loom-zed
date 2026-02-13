# AGENTS.md (services/loom-zed)

## Scope

This file applies to the `services/loom-zed` Zed extension repository.

## Repository Purpose

Zed extension that integrates Loom (loom-core) with the Zed editor. Provides:
- MCP context server: runs `loom proxy` so Zed's Agent panel can use the full Loom MCP ecosystem
- Slash commands: `/loom-check`, `/loom-status`, `/loom-sync`, `/loom-restart`
- Auto-download of loom-core binaries from GitHub releases
- Platform-aware asset selection (macOS/Linux/Windows, arm64/amd64)

## Current Version: 0.1.0

Key features in v0.1.0:
- **Context server**: Runs `loom proxy` as a Zed MCP context server
- **Auto-download**: Fetches loom-core from GitHub releases into extension working directory
- **Slash commands**: Diagnostic and operational commands via Zed's slash command UI
- **Platform-aware**: Selects correct binary for OS/architecture with fallback heuristics
- **Configurable**: Override repo, tag, asset, or provide an explicit binary path

## Workspace Structure

This repo is part of the `services/` GitLab group:

```text
gitlab.flexinfer.ai/
├── platform/gitops    ← K8s manifests, Flux, MCP registry
│   └── mcp/context/registry.yaml  ← Server definitions
└── services/
    ├── loom           ← VS Code extension (TypeScript)
    ├── loom-zed       ← YOU ARE HERE (Zed extension, Rust)
    └── loom-core      ← Go backend (MCP servers, daemon, CLI)
```

## Relationship to Other Projects

| Component | Purpose | Language | Min Version |
|-----------|---------|----------|-------------|
| `loom` | VS Code extension UI | TypeScript | v0.9.0 |
| `loom-zed` | Zed editor extension | Rust | — |
| `loom-core` | Backend daemon, CLI, MCP servers | Go | >= v0.7.0 (for `proxy`) |

### Scope Differences (loom vs loom-zed)

| Capability | loom (VS Code) | loom-zed (Zed) |
|------------|---------------|----------------|
| Context server | Via daemon client | Direct `loom proxy` |
| Tree views | 8 custom views | Not supported by Zed |
| Webviews | 6 panels (dashboard, graphs) | Not supported by Zed |
| Slash commands | Via command palette | `/loom-check`, `/loom-status`, etc. |
| Platform sync | Full multi-platform sync | Not yet (planned) |
| Auto-download | N/A (uses installed CLI) | GitHub release download |

### Shared Conventions
- **Asset naming**: `loom-core_{version}_{os}_{arch}.{tar.gz|zip}` (e.g. `loom-core_v0.9.1_darwin_arm64.tar.gz`)
- Both extensions require loom-core >= v0.7.0 for the `proxy` subcommand

See also:
- [`services/loom/AGENTS.md`](../loom/AGENTS.md) — VS Code extension
- [`services/loom-core/AGENTS.md`](../loom-core/AGENTS.md) — Go backend

## Architecture

### Current Source Structure
```
src/
└── lib.rs          # All logic: extension struct, download, settings, commands, env utils
```

### Module Structure (planned)
```
src/
├── lib.rs          # LoomExtension struct, trait impls, register_extension! (~100 lines)
├── download.rs     # ensure_loom_install, select_release_asset, find_file_named, infer_downloaded_file_type
├── settings.rs     # LoomExtensionSettings, LoomDownloadSettings, parse_extension_settings
├── commands.rs     # slash command dispatch, run_command_capture, truncate_output, join_args
└── env.rs          # env_map_to_vec, shell_env_to_vec, with_path_prefix, upsert_env, current_path_sep, install_key
```

### Key Types
- `LoomExtension` — Main extension struct implementing `zed::Extension`
- `LoomInstall` — Cached install info (path, version, bin_dir)
- `LoomExtensionSettings` / `LoomDownloadSettings` — Zed settings schema
- Uses `Mutex<HashMap<String, LoomInstall>>` for thread-safe install caching

### Build Targets
- **Native** (`cargo build`): Used for tests
- **WASI** (`cargo build --target wasm32-wasi`): Required for Zed runtime

## Configuration

Configured in Zed settings under `context_servers.loom`:

```json
{
  "context_servers": {
    "loom": {
      "command": "loom",
      "args": ["proxy"],
      "env": { "LOOM_LOG_LEVEL": "info" },
      "settings": {
        "download": {
          "enabled": true,
          "repo": "crb2nu/loom-core",
          "tag": null,
          "asset": null
        },
        "mcp": {
          "wrapper": {
            "enabled": true,
            "python": null,
            "tools_poll_interval_secs": 30
          },
          "prompts": { "enabled": true }
        }
      }
    }
  }
}
```

### Settings Reference
| Setting | Default | Description |
|---------|---------|-------------|
| `command.path` | `null` | Explicit binary path (skips download) |
| `command.arguments` | `["proxy"]` | Arguments passed to loom |
| `command.env` | `{}` | Environment variables |
| `settings.download.enabled` | `true` | Enable auto-download from GitHub |
| `settings.download.repo` | `crb2nu/loom-core` | GitHub repo for releases |
| `settings.download.tag` | `null` (latest) | Pin to specific release tag |
| `settings.download.asset` | `null` (auto) | Override exact asset name |

## Key Commands

### Development
```bash
cargo build                        # Build (native)
cargo test                         # Run tests
cargo clippy -- -D warnings        # Lint
cargo fmt -- --check               # Format check
```

### Full Check
```bash
make check                         # clippy + fmt check + test
make lint                          # clippy only
make format                        # auto-format
```

### Zed Development
```
# In Zed: Extensions > Install Dev Extension > select this directory
```

## Extension Manifest

Defined in `extension.toml`:
- **Context servers**: `loom` (runs `loom proxy`)
- **Slash commands**: `loom-check`, `loom-status`, `loom-sync`, `loom-restart`
- **Capabilities**: `process:exec`, `download_file`
- **Zed Extension API**: v0.7.0

## Release Notes

Zed extensions are sourced from GitHub (submitted to Zed extensions registry).
Plan for a GitHub mirror + GitHub release flow for loom-core binary bundles.

## Notes

- Zed is GUI-launched; shell-exported environment variables are unreliable
- Configure tokens via `loom secrets set ...` rather than env vars
- The extension caches downloads with a 6-hour TTL for "latest" releases
- `std::thread::sleep` works in Zed WASI; no async runtime needed
