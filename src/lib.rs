use serde::Deserialize;
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    sync::Mutex,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use zed_extension_api as zed;

const DEFAULT_LOOM_CORE_REPO: &str = "crb2nu/loom-core";
const LATEST_RELEASE_TTL: Duration = Duration::from_secs(6 * 60 * 60);

#[derive(Default)]
struct LoomExtension {
    installs: Mutex<HashMap<String, LoomInstall>>,
}

#[derive(Clone, Debug)]
#[allow(dead_code)]
struct LoomInstall {
    release_version: String,
    loom_path: String,
    loomd_path: Option<String>,
    bin_dir: String,
    resolved_at_unix_secs: Option<u64>,
}

#[derive(Clone, Debug, Default, Deserialize)]
struct LoomExtensionSettings {
    #[serde(default)]
    download: LoomDownloadSettings,
}

#[derive(Clone, Debug, Default, Deserialize)]
struct LoomDownloadSettings {
    /// If false, never attempt to download. We'll rely on `loom` being on PATH (or the user
    /// providing `context_servers.loom.command.path`).
    enabled: Option<bool>,
    /// GitHub repo in the form "<owner>/<repo>".
    repo: Option<String>,
    /// GitHub release tag (e.g. "v0.7.0"). If omitted, use latest release.
    tag: Option<String>,
    /// Exact GitHub release asset name to download (advanced override).
    asset: Option<String>,
}

impl LoomDownloadSettings {
    fn enabled(&self) -> bool {
        self.enabled.unwrap_or(true)
    }

    fn repo(&self) -> &str {
        self.repo
            .as_deref()
            .unwrap_or(DEFAULT_LOOM_CORE_REPO)
            .trim()
    }
}

impl zed::Extension for LoomExtension {
    fn new() -> Self {
        Self::default()
    }

    fn context_server_command(
        &mut self,
        context_server_id: &zed::ContextServerId,
        project: &zed::Project,
    ) -> Result<zed::Command, String> {
        if context_server_id.as_ref() != "loom" {
            return Err(format!(
                "unknown context server id {:?} (expected \"loom\")",
                context_server_id.as_ref()
            ));
        }

        let settings = zed::settings::ContextServerSettings::for_project("loom", project)?;
        let env_from_settings = settings
            .command
            .as_ref()
            .and_then(|c| c.env.as_ref())
            .map(env_map_to_vec)
            .unwrap_or_default();

        let args_from_settings = settings
            .command
            .as_ref()
            .and_then(|c| c.arguments.as_ref())
            .cloned()
            .filter(|a| !a.is_empty())
            .unwrap_or_else(|| vec!["proxy".into()]);

        // If the user configured an explicit command path, respect it.
        if let Some(cmd) = settings.command.as_ref() {
            if let Some(path) = cmd.path.as_ref().filter(|p| !p.trim().is_empty()) {
                return Ok(zed::Command {
                    command: path.trim().to_string(),
                    args: args_from_settings,
                    env: env_from_settings,
                });
            }
        }

        let ext_settings = parse_extension_settings(settings.settings.as_ref());
        let download = ext_settings.download;

        let env = env_from_settings;
        let (loom_cmd, env) = if download.enabled() {
            let install = self.ensure_loom_install(&download)?;
            (
                install.loom_path,
                with_path_prefix(env, &install.bin_dir, current_path_sep()),
            )
        } else {
            ("loom".to_string(), env)
        };

        Ok(zed::Command {
            command: loom_cmd,
            args: args_from_settings,
            env,
        })
    }

    fn run_slash_command(
        &self,
        command: zed::SlashCommand,
        args: Vec<String>,
        worktree: Option<&zed::Worktree>,
    ) -> Result<zed::SlashCommandOutput, String> {
        // Slash commands need to be robust when launched from Zed's GUI environment. Prefer Loom's
        // secret store (`loom secrets ...`) over env vars.
        let mut base_env = worktree
            .map(|wt| shell_env_to_vec(&wt.shell_env()))
            .unwrap_or_default();
        if base_env.is_empty() {
            // Fall back to the extension host environment if available.
            if let Ok(path) = std::env::var("PATH") {
                base_env.push(("PATH".to_string(), path));
            }
        }

        // Best-effort: if Loom isn't on PATH, download a suitable Loom build into the extension
        // working directory and run it from there.
        let download_settings = LoomDownloadSettings::default();
        let (program, base_env) = if let Some(wt) = worktree {
            if let Some(path) = wt.which("loom") {
                (path, base_env)
            } else if download_settings.enabled() {
                let install = self.ensure_loom_install(&download_settings)?;
                (
                    install.loom_path,
                    with_path_prefix(base_env, &install.bin_dir, current_path_sep()),
                )
            } else {
                ("loom".to_string(), base_env)
            }
        } else if download_settings.enabled() {
            let install = self.ensure_loom_install(&download_settings)?;
            (
                install.loom_path,
                with_path_prefix(base_env, &install.bin_dir, current_path_sep()),
            )
        } else {
            ("loom".to_string(), base_env)
        };

        let mut cmd_args: Vec<String> = match command.name.as_str() {
            "loom-check" => vec!["check".into()],
            "loom-status" => vec!["status".into()],
            "loom-sync" => vec!["sync".into(), "status".into()],
            "loom-restart" => vec!["restart".into()],
            other => return Err(format!("unknown slash command {:?}", other)),
        };

        // Allow users to append extra args via `/loom-check --json` etc.
        cmd_args.extend(args);

        let output = run_command_capture(&program, &cmd_args, &base_env, &[])?;
        Ok(zed::SlashCommandOutput {
            text: format!("$ {} {}\n\n{}", program, join_args(&cmd_args), output),
            sections: Vec::new(),
        })
    }
}

impl LoomExtension {
    fn ensure_loom_install(&self, settings: &LoomDownloadSettings) -> Result<LoomInstall, String> {
        let (os, arch) = zed::current_platform();
        let key = install_key(settings, os, arch);
        let now = unix_now_secs();
        let is_latest = settings
            .tag
            .as_ref()
            .map(|t| t.trim().is_empty())
            .unwrap_or(true);

        {
            let installs = self
                .installs
                .lock()
                .map_err(|_| "install cache mutex poisoned")?;
            if let Some(found) = installs.get(&key) {
                if Path::new(&found.loom_path).exists() {
                    // Avoid spamming GitHub for latest unless TTL elapsed.
                    if !is_latest {
                        return Ok(found.clone());
                    }
                    if let Some(resolved_at) = found.resolved_at_unix_secs {
                        if now.saturating_sub(resolved_at) < LATEST_RELEASE_TTL.as_secs() {
                            return Ok(found.clone());
                        }
                    }
                    return Ok(found.clone());
                }
            }
        }

        let repo = settings.repo().to_string();
        let release = if let Some(tag) = settings.tag.as_ref().filter(|t| !t.trim().is_empty()) {
            zed::github_release_by_tag_name(&repo, tag.trim())?
        } else {
            zed::latest_github_release(
                &repo,
                zed::GithubReleaseOptions {
                    require_assets: true,
                    pre_release: false,
                },
            )?
        };

        let asset = select_release_asset(
            &release.assets,
            &release.version,
            os,
            arch,
            settings.asset.as_deref(),
        )
            .ok_or_else(|| {
                let available = summarize_asset_names(&release.assets, 40);
                format!(
                    "no matching release asset found for repo={} version={} os={:?} arch={:?}. available_assets={}",
                    repo, release.version, os, arch, available
                )
            })?;

        let install_dir = PathBuf::from("loom-core").join(&release.version);
        fs::create_dir_all(&install_dir).map_err(|e| e.to_string())?;

        let file_type = infer_downloaded_file_type(&asset.name);
        let dest_file = install_dir.join(&asset.name);
        let dest_file_str = dest_file.to_string_lossy().to_string();
        zed::download_file(&asset.download_url, &dest_file_str, file_type)?;

        let (loom_name, loomd_name) = match os {
            zed::Os::Windows => ("loom.exe", "loomd.exe"),
            _ => ("loom", "loomd"),
        };

        let loom_path = find_file_named(&install_dir, &[loom_name, "loom"]).ok_or_else(|| {
            format!(
                "download succeeded but could not find {} under {:?}",
                loom_name, install_dir
            )
        })?;
        let loomd_path = find_file_named(&install_dir, &[loomd_name, "loomd"])
            .map(|p| p.to_string_lossy().to_string());

        // Ensure the binaries are executable (no-op on Windows).
        if os != zed::Os::Windows {
            let loom_path_str = loom_path.to_string_lossy().to_string();
            zed::make_file_executable(&loom_path_str)?;
            if let Some(ref p) = loomd_path {
                zed::make_file_executable(p)?;
            }
        }

        let bin_dir = loom_path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .to_string_lossy()
            .to_string();

        let install = LoomInstall {
            release_version: release.version,
            loom_path: loom_path.to_string_lossy().to_string(),
            loomd_path,
            bin_dir,
            resolved_at_unix_secs: if is_latest { Some(now) } else { None },
        };

        let mut installs = self
            .installs
            .lock()
            .map_err(|_| "install cache mutex poisoned")?;
        installs.insert(key, install.clone());
        Ok(install)
    }
}

fn parse_extension_settings(raw: Option<&zed::serde_json::Value>) -> LoomExtensionSettings {
    let Some(value) = raw else {
        return LoomExtensionSettings::default();
    };
    zed::serde_json::from_value::<LoomExtensionSettings>(value.clone()).unwrap_or_default()
}

fn env_map_to_vec(env: &HashMap<String, String>) -> Vec<(String, String)> {
    // Keep ordering stable-ish for reproducibility.
    let mut pairs: Vec<(String, String)> =
        env.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
    pairs.sort_by(|a, b| a.0.cmp(&b.0));
    pairs
}

fn shell_env_to_vec(env: &zed::EnvVars) -> Vec<(String, String)> {
    env.clone()
}

fn current_path_sep() -> &'static str {
    let (os, _) = zed::current_platform();
    match os {
        zed::Os::Windows => ";",
        _ => ":",
    }
}

fn with_path_prefix(
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

fn upsert_env(env: &mut Vec<(String, String)>, key: &str, value: &str) {
    if let Some((_, v)) = env.iter_mut().find(|(k, _)| k == key) {
        *v = value.to_string();
        return;
    }
    env.push((key.to_string(), value.to_string()));
}

fn install_key(settings: &LoomDownloadSettings, os: zed::Os, arch: zed::Architecture) -> String {
    format!(
        "repo={} tag={} asset={} os={:?} arch={:?}",
        settings.repo(),
        settings.tag.as_deref().unwrap_or(""),
        settings.asset.as_deref().unwrap_or(""),
        os,
        arch
    )
}

fn infer_downloaded_file_type(asset_name: &str) -> zed::DownloadedFileType {
    let name = asset_name.to_ascii_lowercase();
    if name.ends_with(".tar.gz") || name.ends_with(".tgz") {
        return zed::DownloadedFileType::GzipTar;
    }
    if name.ends_with(".zip") {
        return zed::DownloadedFileType::Zip;
    }
    if name.ends_with(".gz") {
        return zed::DownloadedFileType::Gzip;
    }
    zed::DownloadedFileType::Uncompressed
}

fn select_release_asset<'a>(
    assets: &'a [zed::GithubReleaseAsset],
    version: &str,
    os: zed::Os,
    arch: zed::Architecture,
    exact_name_override: Option<&str>,
) -> Option<&'a zed::GithubReleaseAsset> {
    if let Some(override_name) = exact_name_override.map(str::trim).filter(|s| !s.is_empty()) {
        return assets.iter().find(|a| a.name == override_name);
    }

    // Preferred: exact match to our canonical loom-core release asset naming.
    let os_str = match os {
        zed::Os::Mac => "darwin",
        zed::Os::Linux => "linux",
        zed::Os::Windows => "windows",
    };
    let arch_str = match arch {
        zed::Architecture::Aarch64 => "arm64",
        zed::Architecture::X8664 => "amd64",
        zed::Architecture::X86 => "x86",
    };
    let expected = if os == zed::Os::Windows {
        format!("loom-core_{}_{}_{}.zip", version, os_str, arch_str)
    } else {
        format!("loom-core_{}_{}_{}.tar.gz", version, os_str, arch_str)
    };
    if let Some(asset) = assets.iter().find(|a| a.name == expected) {
        return Some(asset);
    }

    let os_tokens: &[&str] = match os {
        zed::Os::Mac => &["darwin", "macos", "mac"],
        zed::Os::Linux => &["linux"],
        zed::Os::Windows => &["windows", "win"],
    };
    let arch_tokens: &[&str] = match arch {
        zed::Architecture::Aarch64 => &["arm64", "aarch64"],
        zed::Architecture::X8664 => &["x86_64", "x8664", "amd64"],
        zed::Architecture::X86 => &["x86", "386", "i386"],
    };

    let mut matches: Vec<&zed::GithubReleaseAsset> = assets
        .iter()
        .filter(|a| {
            let n = a.name.to_ascii_lowercase();
            // Prefer archives.
            let looks_like_archive =
                n.ends_with(".tar.gz") || n.ends_with(".tgz") || n.ends_with(".zip");
            looks_like_archive
                && os_tokens.iter().any(|t| n.contains(t))
                && arch_tokens.iter().any(|t| n.contains(t))
                && n.contains("loom")
        })
        .collect();

    // Choose the most specific-looking candidate.
    matches.sort_by(|a, b| a.name.len().cmp(&b.name.len()));
    matches.into_iter().next()
}

fn find_file_named(root: &Path, names: &[&str]) -> Option<PathBuf> {
    fn walk(dir: &Path, names: &[&str], depth: usize) -> Option<PathBuf> {
        if depth > 8 {
            return None;
        }
        let entries = fs::read_dir(dir).ok()?;
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if let Some(found) = walk(&path, names, depth + 1) {
                    return Some(found);
                }
                continue;
            }
            let Some(file_name) = path.file_name().and_then(|s| s.to_str()) else {
                continue;
            };
            if names.contains(&file_name) {
                return Some(path);
            }
        }
        None
    }

    walk(root, names, 0)
}

fn unix_now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_else(|_| Duration::from_secs(0))
        .as_secs()
}

fn summarize_asset_names(assets: &[zed::GithubReleaseAsset], max_items: usize) -> String {
    let mut names: Vec<&str> = assets.iter().map(|a| a.name.as_str()).collect();
    names.sort();
    names.truncate(max_items);
    let mut out = names.join(",");
    if assets.len() > max_items {
        out.push_str(",...");
    }
    out
}

fn run_command_capture(
    program: &str,
    args: &[String],
    base_env: &[(String, String)],
    extra_env: &[(String, String)],
) -> Result<String, String> {
    let mut cmd = zed::process::Command::new(program).args(args.iter().cloned());
    for (k, v) in base_env.iter().chain(extra_env.iter()) {
        cmd = cmd.env(k, v);
    }
    let output = cmd.output()?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let status = output
        .status
        .map(|s| s.to_string())
        .unwrap_or_else(|| "unknown".into());

    let mut combined = String::new();
    combined.push_str(&format!("exit_status: {}\n", status));
    if !stdout.trim().is_empty() {
        combined.push_str("\nstdout:\n");
        combined.push_str(stdout.trim_end());
        combined.push('\n');
    }
    if !stderr.trim().is_empty() {
        combined.push_str("\nstderr:\n");
        combined.push_str(stderr.trim_end());
        combined.push('\n');
    }

    Ok(truncate_output(&combined, 40_000))
}

fn truncate_output(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        return s.to_string();
    }
    let mut out: String = s.chars().take(max_chars).collect();
    out.push_str("\n\n[output truncated]\n");
    out
}

fn join_args(args: &[String]) -> String {
    if args.is_empty() {
        return "".to_string();
    }
    args.join(" ")
}

zed::register_extension!(LoomExtension);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn infer_file_type_tar_gz() {
        assert!(matches!(
            infer_downloaded_file_type("loom-core_0.1.0_darwin_arm64.tar.gz"),
            zed::DownloadedFileType::GzipTar
        ));
        assert!(matches!(
            infer_downloaded_file_type("loom-core_0.1.0_linux_amd64.tgz"),
            zed::DownloadedFileType::GzipTar
        ));
    }

    #[test]
    fn infer_file_type_zip() {
        assert!(matches!(
            infer_downloaded_file_type("loom-core_0.1.0_windows_amd64.zip"),
            zed::DownloadedFileType::Zip
        ));
    }

    #[test]
    fn select_asset_prefers_matching_platform_tokens() {
        let assets = vec![
            zed::GithubReleaseAsset {
                name: "loom-core_0.1.0_linux_amd64.tar.gz".into(),
                download_url: "https://example.invalid/linux".into(),
            },
            zed::GithubReleaseAsset {
                name: "loom-core_0.1.0_darwin_arm64.tar.gz".into(),
                download_url: "https://example.invalid/mac".into(),
            },
        ];

        let selected = select_release_asset(
            &assets,
            "0.1.0",
            zed::Os::Mac,
            zed::Architecture::Aarch64,
            None,
        )
        .unwrap();
        assert_eq!(selected.download_url, "https://example.invalid/mac");
    }

    #[test]
    fn select_asset_exact_override() {
        let assets = vec![
            zed::GithubReleaseAsset {
                name: "a.tar.gz".into(),
                download_url: "https://example.invalid/a".into(),
            },
            zed::GithubReleaseAsset {
                name: "b.tar.gz".into(),
                download_url: "https://example.invalid/b".into(),
            },
        ];

        let selected = select_release_asset(
            &assets,
            "0.1.0",
            zed::Os::Mac,
            zed::Architecture::Aarch64,
            Some("b.tar.gz"),
        )
        .unwrap();
        assert_eq!(selected.download_url, "https://example.invalid/b");
    }

    #[test]
    fn select_asset_exact_canonical_name() {
        let assets = vec![
            zed::GithubReleaseAsset {
                name: "loom-core_v0.9.1_linux_amd64.tar.gz".into(),
                download_url: "https://example.invalid/linux".into(),
            },
            zed::GithubReleaseAsset {
                name: "loom-core_v0.9.1_linux_arm64.tar.gz".into(),
                download_url: "https://example.invalid/linux-arm64".into(),
            },
        ];

        let selected = select_release_asset(
            &assets,
            "v0.9.1",
            zed::Os::Linux,
            zed::Architecture::X8664,
            None,
        )
        .unwrap();
        assert_eq!(selected.download_url, "https://example.invalid/linux");
    }

    #[test]
    fn parse_extension_settings_default() {
        let s = parse_extension_settings(None);
        assert!(s.download.enabled());
        assert_eq!(s.download.repo(), DEFAULT_LOOM_CORE_REPO);
    }
}
