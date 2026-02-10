use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    sync::Mutex,
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use zed_extension_api as zed;

use crate::env::install_key;
use crate::settings::LoomDownloadSettings;

const LATEST_RELEASE_TTL: Duration = Duration::from_secs(6 * 60 * 60);

#[derive(Clone, Debug)]
#[allow(dead_code)]
pub(crate) struct LoomInstall {
    pub(crate) release_version: String,
    pub(crate) loom_path: String,
    pub(crate) loomd_path: Option<String>,
    pub(crate) bin_dir: String,
    pub(crate) resolved_at_unix_secs: Option<u64>,
}

const RETRY_BACKOFF_MS: &[u64] = &[500, 1000, 2000];

fn retry_with_backoff<T, F>(mut f: F) -> Result<T, String>
where
    F: FnMut() -> Result<T, String>,
{
    // First attempt without backoff, then retry with each backoff delay
    let mut last_err = match f() {
        Ok(val) => return Ok(val),
        Err(e) => e,
    };
    for &delay_ms in RETRY_BACKOFF_MS {
        thread::sleep(Duration::from_millis(delay_ms));
        match f() {
            Ok(val) => return Ok(val),
            Err(e) => last_err = e,
        }
    }
    Err(last_err)
}

pub(crate) fn ensure_loom_install(
    installs: &Mutex<HashMap<String, LoomInstall>>,
    settings: &LoomDownloadSettings,
) -> Result<LoomInstall, String> {
    let (os, arch) = zed::current_platform();
    let key = install_key(settings, os, arch);
    let now = unix_now_secs();
    let is_latest = settings
        .tag
        .as_ref()
        .map(|t| t.trim().is_empty())
        .unwrap_or(true);

    {
        let installs = installs
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
        let tag = tag.trim().to_string();
        let repo_ref = repo.clone();
        retry_with_backoff(move || zed::github_release_by_tag_name(&repo_ref, &tag))
    } else {
        let repo_ref = repo.clone();
        retry_with_backoff(move || {
            zed::latest_github_release(
                &repo_ref,
                zed::GithubReleaseOptions {
                    require_assets: true,
                    pre_release: false,
                },
            )
        })
    }
    .map_err(|e| {
        format!(
            "{} (hint: check connectivity or pin a version with settings.download.tag)",
            e
        )
    })?;

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
            "no matching release asset found for repo={} version={} os={:?} arch={:?}. \
             available_assets={} (hint: override with settings.download.asset)",
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

    let mut installs = installs
        .lock()
        .map_err(|_| "install cache mutex poisoned")?;
    installs.insert(key, install.clone());
    Ok(install)
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
    fn select_asset_no_matching_platform() {
        let assets = vec![
            zed::GithubReleaseAsset {
                name: "loom-core_0.2.0_linux_amd64.tar.gz".into(),
                download_url: "https://example.invalid/linux".into(),
            },
            zed::GithubReleaseAsset {
                name: "loom-core_0.2.0_linux_arm64.tar.gz".into(),
                download_url: "https://example.invalid/linux-arm64".into(),
            },
        ];

        let selected = select_release_asset(
            &assets,
            "0.2.0",
            zed::Os::Windows,
            zed::Architecture::X8664,
            None,
        );
        assert!(selected.is_none());
    }

    #[test]
    fn select_asset_empty_assets() {
        let assets: Vec<zed::GithubReleaseAsset> = vec![];
        let selected = select_release_asset(
            &assets,
            "0.1.0",
            zed::Os::Mac,
            zed::Architecture::Aarch64,
            None,
        );
        assert!(selected.is_none());
    }

    #[test]
    fn select_asset_windows_prefers_zip() {
        let assets = vec![
            zed::GithubReleaseAsset {
                name: "loom-core_1.0.0_windows_amd64.tar.gz".into(),
                download_url: "https://example.invalid/targz".into(),
            },
            zed::GithubReleaseAsset {
                name: "loom-core_1.0.0_windows_amd64.zip".into(),
                download_url: "https://example.invalid/zip".into(),
            },
        ];

        let selected = select_release_asset(
            &assets,
            "1.0.0",
            zed::Os::Windows,
            zed::Architecture::X8664,
            None,
        )
        .unwrap();
        // The canonical name for Windows uses .zip, so it should match first.
        assert_eq!(selected.download_url, "https://example.invalid/zip");
    }

    #[test]
    fn find_file_named_respects_depth() {
        // Create a temporary directory with no matching file.
        let tmp = std::env::temp_dir().join("loom_zed_test_find_file_depth");
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).unwrap();

        // No file named "loom" exists → should return None.
        let result = find_file_named(&tmp, &["loom"]);
        assert!(result.is_none());

        // Cleanup.
        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn summarize_asset_names_truncation() {
        let assets: Vec<zed::GithubReleaseAsset> = (0..10)
            .map(|i| zed::GithubReleaseAsset {
                name: format!("asset_{}.tar.gz", i),
                download_url: format!("https://example.invalid/{}", i),
            })
            .collect();

        let summary = summarize_asset_names(&assets, 3);
        assert!(summary.ends_with(",..."));
        // Should only contain the first 3 sorted names.
        let parts: Vec<&str> = summary.trim_end_matches(",...").split(',').collect();
        assert_eq!(parts.len(), 3);
    }
}
