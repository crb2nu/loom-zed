# AGENTS.md (services/loom-zed)

Project: Zed extension/plugin to integrate Loom (loom-core) with Zed.

## Repo Goal

- Provide Loom as a Zed MCP context server by running `loom proxy`.
- Keep scope aligned with what Zed extensions support (commands + slash commands, not VS Code-style webviews).

## Development

- Build/test:
  - `cargo test`
  - `cargo build`

Zed dev install (expected workflow):

- Use Zed "Install Dev Extension" and point it at this directory.

## Implementation Notes

- `extension.toml` declares:
  - Context server `loom` (runs `loom proxy`)
  - Slash commands: `loom-check`, `loom-status`, `loom-sync`, `loom-restart`
  - Capabilities: `process:exec`, `download_file`
- `src/lib.rs` downloads `loom` (and `loomd` when present) from GitHub releases into the extension
  working directory when `context_servers.loom.settings.download.enabled` is true (default).

## Release Notes

Zed extensions are typically sourced from GitHub (and often submitted to the Zed extensions registry).
Plan for a GitHub mirror + GitHub release flow for loom-core binary bundles.
