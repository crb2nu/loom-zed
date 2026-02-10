use std::collections::HashMap;
use zed_extension_api as zed;

use crate::settings::LoomDownloadSettings;

pub(crate) fn env_map_to_vec(env: &HashMap<String, String>) -> Vec<(String, String)> {
    // Keep ordering stable-ish for reproducibility.
    let mut pairs: Vec<(String, String)> =
        env.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
    pairs.sort_by(|a, b| a.0.cmp(&b.0));
    pairs
}

pub(crate) fn shell_env_to_vec(env: &zed::EnvVars) -> Vec<(String, String)> {
    env.clone()
}

pub(crate) fn current_path_sep() -> &'static str {
    let (os, _) = zed::current_platform();
    match os {
        zed::Os::Windows => ";",
        _ => ":",
    }
}

pub(crate) fn with_path_prefix(
    mut env: Vec<(String, String)>,
    prefix: &str,
    sep: &str,
) -> Vec<(String, String)> {
    let existing = env
        .iter()
        .find(|(k, _)| k == "PATH")
        .map(|(_, v)| v.clone())
        .or_else(|| std::env::var("PATH").ok())
        .unwrap_or_default();

    let new_path = if existing.trim().is_empty() {
        prefix.to_string()
    } else {
        format!("{}{}{}", prefix, sep, existing)
    };

    upsert_env(&mut env, "PATH", &new_path);
    env
}

pub(crate) fn upsert_env(env: &mut Vec<(String, String)>, key: &str, value: &str) {
    if let Some((_, v)) = env.iter_mut().find(|(k, _)| k == key) {
        *v = value.to_string();
        return;
    }
    env.push((key.to_string(), value.to_string()));
}

pub(crate) fn install_key(
    settings: &LoomDownloadSettings,
    os: zed::Os,
    arch: zed::Architecture,
) -> String {
    format!(
        "repo={} tag={} asset={} os={:?} arch={:?}",
        settings.repo(),
        settings.tag.as_deref().unwrap_or(""),
        settings.asset.as_deref().unwrap_or(""),
        os,
        arch
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upsert_env_existing_key() {
        let mut env = vec![
            ("HOME".to_string(), "/home/user".to_string()),
            ("PATH".to_string(), "/usr/bin".to_string()),
        ];
        upsert_env(&mut env, "PATH", "/usr/local/bin:/usr/bin");
        let path_val = env.iter().find(|(k, _)| k == "PATH").unwrap();
        assert_eq!(path_val.1, "/usr/local/bin:/usr/bin");
        // Length should remain the same (updated in place, not appended).
        assert_eq!(env.len(), 2);
    }

    #[test]
    fn upsert_env_new_key() {
        let mut env = vec![("HOME".to_string(), "/home/user".to_string())];
        upsert_env(&mut env, "EDITOR", "vim");
        assert_eq!(env.len(), 2);
        let editor_val = env.iter().find(|(k, _)| k == "EDITOR").unwrap();
        assert_eq!(editor_val.1, "vim");
    }
}
