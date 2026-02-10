mod commands;
mod download;
mod env;
mod log;
mod settings;

use std::{collections::HashMap, sync::Mutex};
use zed_extension_api as zed;

use commands::{join_args, run_command_capture};
use download::LoomInstall;
use env::{current_path_sep, env_map_to_vec, shell_env_to_vec, with_path_prefix};
use log::{log_msg, LogLevel};
use settings::{parse_extension_settings, LoomDownloadSettings};

#[derive(Default)]
struct LoomExtension {
    installs: Mutex<HashMap<String, LoomInstall>>,
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
        let dl = ext_settings.download;

        let env = env_from_settings;
        let (loom_cmd, env) = if dl.enabled() {
            log_msg(LogLevel::Info, &format!("downloading loom-core from {}", dl.repo()));
            let install = download::ensure_loom_install(&self.installs, &dl)?;
            log_msg(LogLevel::Info, &format!("using loom at {}", install.loom_path));
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
        let mut base_env = worktree
            .map(|wt| shell_env_to_vec(&wt.shell_env()))
            .unwrap_or_default();
        if base_env.is_empty() {
            if let Ok(path) = std::env::var("PATH") {
                base_env.push(("PATH".to_string(), path));
            }
        }

        let download_settings = LoomDownloadSettings::default();
        let (program, base_env) = if let Some(wt) = worktree {
            if let Some(path) = wt.which("loom") {
                (path, base_env)
            } else if download_settings.enabled() {
                let install =
                    download::ensure_loom_install(&self.installs, &download_settings)?;
                (
                    install.loom_path,
                    with_path_prefix(base_env, &install.bin_dir, current_path_sep()),
                )
            } else {
                ("loom".to_string(), base_env)
            }
        } else if download_settings.enabled() {
            let install =
                download::ensure_loom_install(&self.installs, &download_settings)?;
            (
                install.loom_path,
                with_path_prefix(base_env, &install.bin_dir, current_path_sep()),
            )
        } else {
            ("loom".to_string(), base_env)
        };

        log_msg(LogLevel::Info, &format!("slash command: {} {}", command.name, join_args(&args)));
        let mut cmd_args: Vec<String> = match command.name.as_str() {
            "loom-check" => vec!["check".into()],
            "loom-status" => vec!["status".into()],
            "loom-sync" => vec!["sync".into(), "status".into()],
            "loom-restart" => vec!["restart".into()],
            other => return Err(format!("unknown slash command {:?}", other)),
        };

        cmd_args.extend(args);

        let output = run_command_capture(&program, &cmd_args, &base_env, &[])?;
        Ok(zed::SlashCommandOutput {
            text: format!("$ {} {}\n\n{}", program, join_args(&cmd_args), output),
            sections: Vec::new(),
        })
    }
}

zed::register_extension!(LoomExtension);
