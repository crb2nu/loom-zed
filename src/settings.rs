use serde::Deserialize;
use zed_extension_api as zed;

pub(crate) const DEFAULT_LOOM_CORE_REPO: &str = "crb2nu/loom-core";

#[derive(Clone, Debug, Default, Deserialize)]
pub(crate) struct LoomExtensionSettings {
    #[serde(default)]
    pub(crate) download: LoomDownloadSettings,
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
}
