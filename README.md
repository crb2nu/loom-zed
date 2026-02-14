# Loom for Zed

Zed extension that integrates [loom-core](../loom-core/) with Zed as an MCP context server.

Exposes the full Loom MCP ecosystem (GitHub, GitLab, K8s, Prometheus, Tavily, etc.) through
Zed's Agent panel via `loom proxy`.

## Prerequisites

- **Zed editor** (latest stable or preview)
- **Network access** to GitHub (for auto-download of loom-core binaries), OR
- **Local loom binary** on `$PATH` or configured via `context_servers.loom.command.path`

## Features

- **Context server**: `loom` runs `loom proxy` as a Zed MCP context server
- **Slash commands**: `/loom-check`, `/loom-status`, `/loom-sync`, `/loom-restart`, `/loom-info`
- **Prompt recipes**: Curated MCP prompts in Zed's Agent prompt picker (onboarding, CI triage, rollout)
- **Tool hot reload**: Automatically refreshes the tool list when Loom's tool set changes
- **Auto-download**: Downloads loom-core binaries from GitHub releases with retry and exponential backoff
- **Platform-aware**: Selects the correct binary for macOS/Linux/Windows on arm64/amd64

## See Also

- [Loom VS Code Extension](../loom/) — Full-featured MCP management for VS Code
- [loom-core](../loom-core/) — Backend Go binary powering both extensions

## Configuration

This extension registers a Zed context server named `loom`. By default it will:

- Download an appropriate Loom build from the latest GitHub release for `crb2nu/loom-core`
- Run `loom proxy` as the context server command

The extension expects loom-core GitHub release assets named like:

- `loom-core_v0.9.1_darwin_arm64.tar.gz`
- `loom-core_v0.9.1_linux_amd64.tar.gz`
- `loom-core_v0.9.1_windows_amd64.zip`

You can customize behavior in Zed settings under `context_servers.loom`:

```json
{
  "context_servers": {
    "loom": {
      "command": {
        "path": "loom",
        "arguments": ["proxy"],
        "env": { "LOOM_LOG_LEVEL": "info" }
      },
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
          "prompts": {
            "enabled": true
          },
          "resources": {
            "enabled": true
          }
        }
      }
    }
  }
}
```

Notes:

- If you set `context_servers.loom.command` to an explicit absolute path (or a command resolvable in Zed's environment),
  the extension will not download Loom and will run exactly what you configure.
- The MCP wrapper that provides prompt recipes + tool hot reload requires `python3` to be available on your machine.
- `settings.download.tag` can be used to pin a release tag (example: `"v0.9.0"`).
- `settings.download.asset` can be used to select an exact asset name from the release (advanced).

## Troubleshooting

### Binary not found

If the auto-download fails, install loom-core manually and set the path:

```json
{
  "context_servers": {
    "loom": {
      "command": { "path": "/usr/local/bin/loom" }
    }
  }
}
```

### Network errors during download

The extension retries GitHub API calls with exponential backoff (500ms, 1s, 2s). If downloads
consistently fail, pin a specific release tag to skip the "latest" API call:

```json
{
  "context_servers": {
    "loom": {
      "settings": { "download": { "tag": "v0.9.1" } }
    }
  }
}
```

### Permission denied on binary

On macOS/Linux, the extension calls `zed::make_file_executable()` after download. If that fails,
manually fix permissions:

```bash
chmod +x ~/.local/share/zed/extensions/loom-zed/loom-core/*/loom
```

### Slash commands not working

Ensure `loom` is on your `$PATH` or the auto-download completed successfully. Check Zed's
extension host logs (View > Toggle Developer Tools) for error messages.
