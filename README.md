# Loom for Zed (WIP)

Zed extension/plugin that integrates Loom (loom-core) with Zed.

Primary goal: expose Loom as a Zed MCP "context server" by running `loom proxy`, so Zed's
Agent panel can use the full Loom MCP ecosystem (tavily, github/gitlab, k8s, etc.) through
one entrypoint.

Status: MVP works (context server + auto-download + slash commands). See `.loom/20-product-spec.md`
and `.loom/30-implementation-plan.md` from the workspace root for the broader roadmap.

## Planned Features

- Zed context server: `loom` (runs `loom proxy`)
- Slash commands: `/loom-check`, `/loom-status`, `/loom-sync`, `/loom-restart`
- Download Loom binaries from GitHub releases into the extension working directory

## Notes

Zed is GUI-launched; relying on shell-exported environment variables is brittle. Loom should
be configured using its secret store (`loom secrets set ...`) so `loomd` can resolve tokens
even when Zed has no environment variables.

## Configuration

This extension registers a Zed context server named `loom`. By default it will:

- Download an appropriate Loom build from the latest GitHub release for `crb2nu/loom-core`
- Run `loom proxy` as the context server command

You can customize behavior in Zed settings under `context_servers.loom`:

```json
{
  "context_servers": {
    "loom": {
      "command": {
        "path": null,
        "arguments": null,
        "env": {
          "LOOM_LOG_LEVEL": "info"
        }
      },
      "settings": {
        "download": {
          "enabled": true,
          "repo": "crb2nu/loom-core",
          "tag": null,
          "asset": null
        }
      }
    }
  }
}
```

Notes:

- If you set `context_servers.loom.command.path`, the extension will not download Loom and will run
  exactly what you configure.
- `settings.download.tag` can be used to pin a release tag (example: `"v0.9.0"`).
- `settings.download.asset` can be used to select an exact asset name from the release (advanced).
