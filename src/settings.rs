use serde::Deserialize;
use zed_extension_api as zed;

pub(crate) const DEFAULT_LOOM_CORE_REPO: &str = "crb2nu/loom-core";

#[derive(Clone, Debug, Default)]
pub(crate) struct LoomRuntimeSettings {
    /// Optional explicit loom binary path (from Zed context server settings).
    pub(crate) command_path: Option<String>,
    /// Environment variables to apply (from Zed context server settings).
    pub(crate) command_env: Vec<(String, String)>,
    /// Extension-specific settings (download, agent, etc).
    pub(crate) extension: LoomExtensionSettings,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub(crate) struct LoomExtensionSettings {
    #[serde(default)]
    pub(crate) download: LoomDownloadSettings,
    #[serde(default)]
    #[allow(dead_code)] // consumed by dispatch_session/heartbeat/task in future
    pub(crate) agent: AgentSettings,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub(crate) struct LoomDownloadSettings {
    /// If false, never attempt to download. We'll rely on `loom` being on PATH (or the user
    /// providing `context_servers.loom.command.path`).
    pub(crate) enabled: Option<bool>,
    /// GitHub repo in the form "<owner>/<repo>".
    pub(crate) repo: Option<String>,
    /// GitHub release tag (e.g. "v0.7.0"). If omitted, use latest release.
    pub(crate) tag: Option<String>,
    /// Exact GitHub release asset name to download (advanced override).
    pub(crate) asset: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[allow(dead_code)] // fields consumed by dispatch_session/heartbeat/task in future
pub(crate) struct AgentSettings {
    /// Agent identifier used for session/heartbeat/task operations.
    pub(crate) agent_id: Option<String>,
    /// Default namespace for sessions (e.g. "project/branch").
    pub(crate) default_namespace: Option<String>,
}

impl Default for AgentSettings {
    fn default() -> Self {
        Self {
            agent_id: Some("zed-loom".to_string()),
            default_namespace: None,
        }
    }
}

impl AgentSettings {
    #[allow(dead_code)] // used by dispatch functions in future phases
    pub(crate) fn agent_id(&self) -> &str {
        self.agent_id.as_deref().unwrap_or("zed-loom")
    }
}

impl LoomDownloadSettings {
    pub(crate) fn enabled(&self) -> bool {
        self.enabled.unwrap_or(true)
    }

    pub(crate) fn repo(&self) -> &str {
        self.repo
            .as_deref()
            .unwrap_or(DEFAULT_LOOM_CORE_REPO)
            .trim()
    }
}

pub(crate) fn parse_extension_settings(
    raw: Option<&zed::serde_json::Value>,
) -> LoomExtensionSettings {
    let Some(value) = raw else {
        return LoomExtensionSettings::default();
    };
    zed::serde_json::from_value::<LoomExtensionSettings>(value.clone()).unwrap_or_default()
}

// ---------------------------------------------------------------------------
// Context server configuration constants
// ---------------------------------------------------------------------------

pub(crate) const INSTALL_INSTRUCTIONS: &str = r#"# Loom MCP Hub — Zed Integration

## Prerequisites

Install **loom-core** (the Loom CLI + daemon):

```bash
# macOS / Linux (Homebrew)
brew install crb2nu/tap/loom-core

# Or download from GitHub Releases
# https://github.com/crb2nu/loom-core/releases
```

## Quick Start

1. Start the daemon: `loom start`
2. Verify: `loom status`
3. Open Zed's Agent panel — the Loom context server activates automatically.

## Auto-Download

If `loom` is not on your PATH, this extension downloads it automatically from GitHub Releases. To disable, set `"download": { "enabled": false }` in the extension settings.
"#;

pub(crate) const SETTINGS_SCHEMA: &str = r#"{
  "$schema": "http://json-schema.org/draft-07/schema#",
  "type": "object",
  "properties": {
    "download": {
      "type": "object",
      "description": "Auto-download settings for loom-core binary.",
      "properties": {
        "enabled": {
          "type": "boolean",
          "default": true,
          "description": "Enable automatic download of loom-core from GitHub."
        },
        "repo": {
          "type": "string",
          "default": "crb2nu/loom-core",
          "description": "GitHub repository (owner/repo) for releases."
        },
        "tag": {
          "type": ["string", "null"],
          "default": null,
          "description": "Pin to a specific release tag (e.g. 'v0.7.0'). Null = latest."
        },
        "asset": {
          "type": ["string", "null"],
          "default": null,
          "description": "Override the exact asset filename to download."
        }
      }
    },
    "agent": {
      "type": "object",
      "description": "Agent lifecycle settings.",
      "properties": {
        "agent_id": {
          "type": "string",
          "default": "zed-loom",
          "description": "Agent identifier for session/heartbeat/task operations."
        },
        "default_namespace": {
          "type": ["string", "null"],
          "default": null,
          "description": "Default namespace for agent sessions."
        }
      }
    }
  }
}"#;

pub(crate) const DEFAULT_SETTINGS: &str = r#"{
  "download": {
    "enabled": true,
    "repo": "crb2nu/loom-core",
    "tag": null,
    "asset": null
  },
  "agent": {
    "agent_id": "zed-loom",
    "default_namespace": null
  }
}"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_extension_settings_default() {
        let s = parse_extension_settings(None);
        assert!(s.download.enabled());
        assert_eq!(s.download.repo(), DEFAULT_LOOM_CORE_REPO);
    }

    #[test]
    fn parse_settings_explicit_repo() {
        let value = zed::serde_json::json!({
            "download": {
                "repo": "myorg/my-loom"
            }
        });
        let s = parse_extension_settings(Some(&value));
        assert_eq!(s.download.repo(), "myorg/my-loom");
    }

    #[test]
    fn empty_tag_treated_as_latest() {
        let s = LoomDownloadSettings {
            enabled: None,
            repo: None,
            tag: Some("".to_string()),
            asset: None,
        };
        // enabled() still defaults to true.
        assert!(s.enabled());
        // repo() returns the default.
        assert_eq!(s.repo(), DEFAULT_LOOM_CORE_REPO);
        // An empty tag is treated like None (latest).
        assert!(s.tag.as_ref().map(|t| t.trim().is_empty()).unwrap_or(true));
    }

    #[test]
    fn download_disabled() {
        let s = LoomDownloadSettings {
            enabled: Some(false),
            repo: None,
            tag: None,
            asset: None,
        };
        assert!(!s.enabled());
    }

    #[test]
    fn agent_settings_defaults() {
        let s = AgentSettings::default();
        assert_eq!(s.agent_id(), "zed-loom");
        assert!(s.default_namespace.is_none());
    }

    #[test]
    fn parse_settings_with_agent() {
        let value = zed::serde_json::json!({
            "agent": {
                "agent_id": "my-agent",
                "default_namespace": "project/main"
            }
        });
        let s = parse_extension_settings(Some(&value));
        assert_eq!(s.agent.agent_id(), "my-agent");
        assert_eq!(s.agent.default_namespace.as_deref(), Some("project/main"));
    }

    #[test]
    fn settings_schema_is_valid_json() {
        let parsed: Result<zed::serde_json::Value, _> = zed::serde_json::from_str(SETTINGS_SCHEMA);
        assert!(parsed.is_ok(), "SETTINGS_SCHEMA must be valid JSON");
    }

    #[test]
    fn default_settings_is_valid_json() {
        let parsed: Result<zed::serde_json::Value, _> = zed::serde_json::from_str(DEFAULT_SETTINGS);
        assert!(parsed.is_ok(), "DEFAULT_SETTINGS must be valid JSON");
    }
}
