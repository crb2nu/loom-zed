mod commands;
mod completions;
mod dispatch;
mod download;
mod env;
mod format;
mod help;
mod log;
mod settings;

use std::{collections::HashMap, sync::Mutex};
use zed_extension_api as zed;

use commands::join_args;
use completions::complete_argument;
use dispatch::{dispatch_command, resolve_binary};
use download::LoomInstall;
use env::{current_path_sep, env_map_to_vec, with_path_prefix};
use log::{log_msg, LogLevel};
use settings::{
    parse_extension_settings, LoomRuntimeSettings, DEFAULT_SETTINGS, INSTALL_INSTRUCTIONS,
    SETTINGS_SCHEMA,
};

#[derive(Default)]
struct LoomExtension {
    installs: Mutex<HashMap<String, LoomInstall>>,
    runtime_settings: Mutex<Option<LoomRuntimeSettings>>,
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

        let ext_settings = parse_extension_settings(settings.settings.as_ref());
        let dl = ext_settings.download.clone();

        // Cache the last-known Zed context server settings so slash commands can reuse
        // the same command/env/download config (best-effort; slash commands can run
        // without the context server being started yet).
        {
            let mut rt = self
                .runtime_settings
                .lock()
                .map_err(|_| "runtime settings mutex poisoned")?;
            *rt = Some(LoomRuntimeSettings {
                command_path: settings
                    .command
                    .as_ref()
                    .and_then(|c| c.path.as_ref())
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty()),
                command_env: env_from_settings.clone(),
                extension: ext_settings.clone(),
            });
        }

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

        log_msg(
            LogLevel::Info,
            &format!(
                "settings: command={}, download.enabled={}, settings.present={}",
                settings.command.is_some(),
                dl.enabled(),
                settings.settings.is_some(),
            ),
        );

        let env = env_from_settings;

        // Always try to resolve a local binary first — this avoids blocking
        // on slow/failing GitHub API calls when loom is already installed.
        let local_path = resolve_loom_path();
        let have_local = local_path != "loom";

        let (loom_cmd, env) = if dl.enabled() && !have_local {
            log_msg(
                LogLevel::Info,
                &format!("downloading loom-core from {}", dl.repo()),
            );
            let install = download::ensure_loom_install(&self.installs, &dl)?;
            log_msg(
                LogLevel::Info,
                &format!("using downloaded loom at {}", install.loom_path),
            );
            (
                install.loom_path,
                with_path_prefix(env, &install.bin_dir, current_path_sep()),
            )
        } else {
            log_msg(LogLevel::Info, &format!("using loom at: {local_path}"));
            (local_path, env)
        };

        Ok(zed::Command {
            command: loom_cmd,
            args: args_from_settings,
            env,
        })
    }

    fn context_server_configuration(
        &mut self,
        context_server_id: &zed::ContextServerId,
        _project: &zed::Project,
    ) -> Result<Option<zed::ContextServerConfiguration>, String> {
        if context_server_id.as_ref() != "loom" {
            return Ok(None);
        }

        Ok(Some(zed::ContextServerConfiguration {
            installation_instructions: INSTALL_INSTRUCTIONS.to_string(),
            settings_schema: SETTINGS_SCHEMA.to_string(),
            default_settings: DEFAULT_SETTINGS.to_string(),
        }))
    }

    fn complete_slash_command_argument(
        &self,
        command: zed::SlashCommand,
        args: Vec<String>,
    ) -> Result<Vec<zed::SlashCommandArgumentCompletion>, String> {
        Ok(complete_argument(&command.name, &args))
    }

    fn run_slash_command(
        &self,
        command: zed::SlashCommand,
        args: Vec<String>,
        worktree: Option<&zed::Worktree>,
    ) -> Result<zed::SlashCommandOutput, String> {
        let rt = self
            .runtime_settings
            .lock()
            .map_err(|_| "runtime settings mutex poisoned")?;
        let (program, base_env) = resolve_binary(&self.installs, worktree, rt.as_ref())?;

        log_msg(
            LogLevel::Info,
            &format!("slash command: {} {}", command.name, join_args(&args)),
        );

        let formatted = dispatch_command(&command.name, &args, &program, &base_env)?;

        Ok(zed::SlashCommandOutput {
            text: formatted.text,
            sections: formatted.sections,
        })
    }
}

/// Resolve the absolute path to the `loom` binary.
///
/// Zed may not search the system PATH when spawning extension-provided context
/// servers, so we need to return an absolute path.  We try, in order:
///   1. `which loom` via the host process API
///   2. Well-known install locations
///   3. Bare `"loom"` as a last resort
fn resolve_loom_path() -> String {
    // Try `which loom` through the host
    if let Ok(output) = zed::process::Command::new("which").arg("loom").output() {
        if output.status == Some(0) {
            let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !path.is_empty() {
                return path;
            }
        }
    }

    // Check well-known locations
    let home = std::env::var("HOME").unwrap_or_default();
    let candidates = [
        format!("{home}/.local/bin/loom"),
        "/usr/local/bin/loom".to_string(),
        "/opt/homebrew/bin/loom".to_string(),
    ];
    for candidate in &candidates {
        if std::path::Path::new(candidate).exists() {
            return candidate.clone();
        }
    }

    "loom".to_string()
}

zed::register_extension!(LoomExtension);
