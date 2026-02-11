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
use settings::{parse_extension_settings, DEFAULT_SETTINGS, INSTALL_INSTRUCTIONS, SETTINGS_SCHEMA};

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
            log_msg(
                LogLevel::Info,
                &format!("downloading loom-core from {}", dl.repo()),
            );
            let install = download::ensure_loom_install(&self.installs, &dl)?;
            log_msg(
                LogLevel::Info,
                &format!("using loom at {}", install.loom_path),
            );
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
        let (program, base_env) = resolve_binary(&self.installs, worktree)?;

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

zed::register_extension!(LoomExtension);
