# Changelog

All notable changes to loom-zed will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.6.0] - 2026-02-14

### Added

- MCP wrapper (python) that enhances `loom proxy` with:
  - Prompt recipes in Zed's Agent prompt picker
  - Tool hot reload (`tools/list_changed`) when Loom's toolset changes
- MCP Resources exposure to support Zed's "Add Context" UX
- `/loom-info` slash command to show resolved Loom binary and version

### Changed

- Expanded the extension manifest to declare the full slash command surface implemented by the extension.

## [0.1.0] - 2025-05-01

### Added

- Context server integration: runs `loom proxy` as a Zed MCP context server
- Auto-download of loom-core binaries from GitHub releases
- Platform-aware asset selection (macOS/Linux/Windows, arm64/amd64/x86)
- Slash commands: `/loom-check`, `/loom-status`, `/loom-sync`, `/loom-restart`
- Configurable download settings (repo, tag, asset override)
- Install caching with 6-hour TTL for latest releases
- Fallback heuristics for non-canonical asset naming
- Support for explicit `command.path` to skip auto-download
