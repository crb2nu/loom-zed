# AGENTS.md (services/loom-zed)

Project: Zed extension/plugin to integrate Loom (loom-core) with Zed.

## Repo Goal

- Provide Loom as a Zed MCP context server by running `loom proxy`.
- Keep scope aligned with what Zed extensions support (commands + slash commands, not VS Code-style webviews).

## Development

This repo is currently a skeleton; once implemented:

- Build/test:
  - `cargo test`
  - `cargo build`

Zed dev install (expected workflow):

- Use Zed "Install Dev Extension" and point it at this directory.

## Release Notes

Zed extensions are typically sourced from GitHub (and often submitted to the Zed extensions registry).
Plan for a GitHub mirror + GitHub release flow for loom-core binary bundles.

