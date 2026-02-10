# Loom for Zed (WIP)

Zed extension/plugin that integrates Loom (loom-core) with Zed.

Primary goal: expose Loom as a Zed MCP "context server" by running `loom proxy`, so Zed's
Agent panel can use the full Loom MCP ecosystem (tavily, github/gitlab, k8s, etc.) through
one entrypoint.

Status: design + skeleton only. See `.loom/20-product-spec.md` and `.loom/30-implementation-plan.md`
from the workspace root for the current plan.

## Planned Features

- Zed context server: `loom` (runs `loom proxy`)
- Optional: slash commands for `loom check`, `loom restart`, `loom sync ...`
- Optional: download loom binaries from GitHub releases into extension work dir

## Notes

Zed is GUI-launched; relying on shell-exported environment variables is brittle. Loom should
be configured using its secret store (`loom secrets set ...`) so `loomd` can resolve tokens
even when Zed has no environment variables.

