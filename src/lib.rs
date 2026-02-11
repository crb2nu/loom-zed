mod commands;
mod completions;
mod download;
mod env;
mod format;
mod log;
mod settings;

use std::{collections::HashMap, sync::Mutex};
use zed_extension_api as zed;

use commands::{join_args, run_command_capture};
use completions::complete_argument;
use download::LoomInstall;
use env::{current_path_sep, env_map_to_vec, shell_env_to_vec, with_path_prefix};
use format::{
    format_daemon_action, format_diagnostic_report, format_generic, format_status_report,
    format_sync_report, FormattedOutput,
};
use log::{log_msg, LogLevel};
use settings::{
    parse_extension_settings, LoomDownloadSettings, DEFAULT_SETTINGS, INSTALL_INSTRUCTIONS,
    SETTINGS_SCHEMA,
};

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

// ---------------------------------------------------------------------------
// Binary resolution (shared between context server + slash commands)
// ---------------------------------------------------------------------------

/// Resolve the loom binary path and build the base environment.
fn resolve_binary(
    installs: &Mutex<HashMap<String, LoomInstall>>,
    worktree: Option<&zed::Worktree>,
) -> Result<(String, Vec<(String, String)>), String> {
    let mut base_env = worktree
        .map(|wt| shell_env_to_vec(&wt.shell_env()))
        .unwrap_or_default();
    if base_env.is_empty() {
        if let Ok(path) = std::env::var("PATH") {
            base_env.push(("PATH".to_string(), path));
        }
    }

    let download_settings = LoomDownloadSettings::default();
    if let Some(wt) = worktree {
        if let Some(path) = wt.which("loom") {
            return Ok((path, base_env));
        }
    }

    if download_settings.enabled() {
        let install = download::ensure_loom_install(installs, &download_settings)?;
        Ok((
            install.loom_path,
            with_path_prefix(base_env, &install.bin_dir, current_path_sep()),
        ))
    } else {
        Ok(("loom".to_string(), base_env))
    }
}

// ---------------------------------------------------------------------------
// Command dispatch and formatting
// ---------------------------------------------------------------------------

/// Map a slash command name + args to CLI args, run it, and format the output.
fn dispatch_command(
    command_name: &str,
    args: &[String],
    program: &str,
    base_env: &[(String, String)],
) -> Result<FormattedOutput, String> {
    match command_name {
        "loom-check" => {
            let result = run_command_capture(program, &["check".into()], base_env, &[])?;
            Ok(format_diagnostic_report(&result))
        }
        "loom-status" => {
            let result = run_command_capture(program, &["status".into()], base_env, &[])?;
            Ok(format_status_report(&result))
        }
        "loom-sync" => dispatch_sync(args, program, base_env),
        "loom-restart" => {
            let result = run_command_capture(program, &["restart".into()], base_env, &[])?;
            Ok(format_daemon_action(&result, "restart"))
        }
        "loom-start" => {
            let result = run_command_capture(program, &["start".into()], base_env, &[])?;
            Ok(format_daemon_action(&result, "start"))
        }
        "loom-stop" => {
            let result = run_command_capture(program, &["stop".into()], base_env, &[])?;
            Ok(format_daemon_action(&result, "stop"))
        }
        "loom-tools" => dispatch_tools(args, program, base_env),
        "loom-servers" => {
            let result =
                run_command_capture(program, &["servers".into(), "list".into()], base_env, &[])?;
            Ok(format::format_servers_list(&result))
        }
        "loom-ping" => {
            let result = run_command_capture(program, &["status".into()], base_env, &[])?;
            Ok(format::format_ping(&result))
        }
        "loom-secrets" => dispatch_secrets(args, program, base_env),
        "loom-session" => dispatch_session(args, program, base_env),
        "loom-heartbeat" => {
            let result = run_command_capture(
                program,
                &[
                    "agent".into(),
                    "heartbeat".into(),
                    "--agent-id".into(),
                    "zed-loom".into(),
                    "--status".into(),
                    "active".into(),
                ],
                base_env,
                &[],
            )?;
            Ok(format_generic(&result, "Heartbeat"))
        }
        "loom-task" => dispatch_task(args, program, base_env),
        "loom-recall" => dispatch_recall(args, program, base_env),
        "loom-skills" => dispatch_skills(args, program, base_env),
        "loom-search" => dispatch_search(args, program, base_env),
        "loom-profile" => dispatch_profile(args, program, base_env),
        "loom-call" => dispatch_call(args, program, base_env),
        "loom-dashboard" => dispatch_dashboard(program, base_env),
        "loom-help" => Ok(dispatch_help(args)),
        other => Err(format!("unknown slash command {:?}", other)),
    }
}

// ---------------------------------------------------------------------------
// Sub-command dispatchers
// ---------------------------------------------------------------------------

fn dispatch_sync(
    args: &[String],
    program: &str,
    base_env: &[(String, String)],
) -> Result<FormattedOutput, String> {
    let sub = args.first().map(|s| s.as_str()).unwrap_or("status");

    if sub == "status" || sub.is_empty() {
        let result =
            run_command_capture(program, &["sync".into(), "status".into()], base_env, &[])?;
        Ok(format_sync_report(&result, None))
    } else {
        if !completions::is_valid_sync_platform(sub) {
            return Err(format!(
                "unknown sync platform {:?}. Valid: status, zed, vscode, claude, gemini, codex, antigravity, kilocode",
                sub
            ));
        }
        let result = run_command_capture(
            program,
            &["sync".into(), sub.to_string(), "--regen".into()],
            base_env,
            &[],
        )?;
        Ok(format_sync_report(&result, Some(sub)))
    }
}

fn dispatch_tools(
    args: &[String],
    program: &str,
    base_env: &[(String, String)],
) -> Result<FormattedOutput, String> {
    let sub = args.first().map(|s| s.as_str()).unwrap_or("list");
    match sub {
        "search" => {
            let query = args.get(1).map(|s| s.as_str()).unwrap_or("");
            if query.is_empty() {
                return Err("usage: /loom-tools search <query>".to_string());
            }
            let result = run_command_capture(
                program,
                &["tools".into(), "search".into(), query.to_string()],
                base_env,
                &[],
            )?;
            Ok(format::format_tools_table(&result))
        }
        _ => {
            let result =
                run_command_capture(program, &["tools".into(), "list".into()], base_env, &[])?;
            Ok(format::format_tools_table(&result))
        }
    }
}

fn dispatch_secrets(
    args: &[String],
    program: &str,
    base_env: &[(String, String)],
) -> Result<FormattedOutput, String> {
    let sub = args.first().map(|s| s.as_str()).unwrap_or("list");
    let cmd_args: Vec<String> = match sub {
        "validate" => vec!["secrets".into(), "validate".into()],
        _ => vec!["secrets".into(), "list".into()],
    };
    let result = run_command_capture(program, &cmd_args, base_env, &[])?;
    Ok(format::format_secrets(&result, sub))
}

fn dispatch_session(
    args: &[String],
    program: &str,
    base_env: &[(String, String)],
) -> Result<FormattedOutput, String> {
    let sub = args.first().map(|s| s.as_str()).unwrap_or("status");
    let cmd_args: Vec<String> = match sub {
        "start" => {
            let mut a = vec![
                "agent".into(),
                "session-start".into(),
                "--agent-id".into(),
                "zed-loom".into(),
            ];
            if let Some(ns) = args.get(1) {
                a.push("--namespace".into());
                a.push(ns.clone());
            }
            a.push("--auto-recall".into());
            a
        }
        "end" => vec![
            "agent".into(),
            "session-end".into(),
            "--agent-id".into(),
            "zed-loom".into(),
            "--summarize".into(),
        ],
        "list" => vec!["agent".into(), "session-list".into()],
        _ => vec![
            "agent".into(),
            "session".into(),
            "--agent-id".into(),
            "zed-loom".into(),
        ],
    };
    let result = run_command_capture(program, &cmd_args, base_env, &[])?;
    Ok(format::format_session(&result, sub))
}

fn dispatch_task(
    args: &[String],
    program: &str,
    base_env: &[(String, String)],
) -> Result<FormattedOutput, String> {
    let sub = args.first().map(|s| s.as_str()).unwrap_or("list");
    let cmd_args: Vec<String> = match sub {
        "add" => {
            let desc = args.get(1..).map(|a| a.join(" ")).unwrap_or_default();
            if desc.is_empty() {
                return Err("usage: /loom-task add <description>".to_string());
            }
            vec![
                "tools".into(),
                "call".into(),
                "agent_task_add".into(),
                "--".into(),
                format!(r#"{{"description":"{}"}}"#, desc),
            ]
        }
        "update" => {
            let task_id = args
                .get(1)
                .ok_or("usage: /loom-task update <id> <status>")?;
            let status = args
                .get(2)
                .ok_or("usage: /loom-task update <id> <status>")?;
            vec![
                "agent".into(),
                "task-update".into(),
                "--task-id".into(),
                task_id.clone(),
                "--status".into(),
                status.clone(),
            ]
        }
        _ => vec!["tools".into(), "call".into(), "agent_task_list".into()],
    };
    let result = run_command_capture(program, &cmd_args, base_env, &[])?;
    Ok(format::format_task(&result, sub))
}

fn dispatch_recall(
    args: &[String],
    program: &str,
    base_env: &[(String, String)],
) -> Result<FormattedOutput, String> {
    let query = args.join(" ");
    if query.trim().is_empty() {
        return Err("usage: /loom-recall <query>".to_string());
    }
    let result = run_command_capture(
        program,
        &[
            "tools".into(),
            "call".into(),
            "agent_context_recall_enhanced".into(),
            "--".into(),
            format!(r#"{{"query":"{}"}}"#, query),
        ],
        base_env,
        &[],
    )?;
    Ok(format::format_recall(&result))
}

fn dispatch_skills(
    args: &[String],
    program: &str,
    base_env: &[(String, String)],
) -> Result<FormattedOutput, String> {
    let sub = args.first().map(|s| s.as_str()).unwrap_or("list");
    let cmd_args: Vec<String> = match sub {
        "search" => {
            let query = args.get(1).map(|s| s.as_str()).unwrap_or("");
            if query.is_empty() {
                return Err("usage: /loom-skills search <query>".to_string());
            }
            vec![
                "tools".into(),
                "call".into(),
                "skills_search".into(),
                "--".into(),
                format!(r#"{{"query":"{}"}}"#, query),
            ]
        }
        "categories" => {
            vec!["tools".into(), "call".into(), "skills_categories".into()]
        }
        _ => {
            vec!["tools".into(), "call".into(), "skills_list".into()]
        }
    };
    let result = run_command_capture(program, &cmd_args, base_env, &[])?;
    Ok(format::format_skills(&result))
}

fn dispatch_search(
    args: &[String],
    program: &str,
    base_env: &[(String, String)],
) -> Result<FormattedOutput, String> {
    let query = args.join(" ");
    if query.trim().is_empty() {
        return Err("usage: /loom-search <query>".to_string());
    }
    let result = run_command_capture(
        program,
        &[
            "tools".into(),
            "call".into(),
            "deep_search".into(),
            "--".into(),
            format!(r#"{{"query":"{}"}}"#, query),
        ],
        base_env,
        &[],
    )?;
    Ok(format::format_search(&result))
}

fn dispatch_profile(
    args: &[String],
    program: &str,
    base_env: &[(String, String)],
) -> Result<FormattedOutput, String> {
    let sub = args.first().map(|s| s.as_str()).unwrap_or("current");
    let cmd_args: Vec<String> = match sub {
        "list" => vec!["profile".into(), "list".into()],
        "switch" => {
            let name = args.get(1).ok_or("usage: /loom-profile switch <name>")?;
            vec!["profile".into(), "switch".into(), name.clone()]
        }
        _ => vec!["profile".into(), "current".into()],
    };
    let result = run_command_capture(program, &cmd_args, base_env, &[])?;
    Ok(format::format_profile(&result, sub))
}

fn dispatch_call(
    args: &[String],
    program: &str,
    base_env: &[(String, String)],
) -> Result<FormattedOutput, String> {
    let tool_name = args
        .first()
        .ok_or("usage: /loom-call <tool_name> [json_args]")?;
    let mut cmd_args = vec!["tools".into(), "call".into(), tool_name.clone()];
    if args.len() > 1 {
        cmd_args.push("--".into());
        cmd_args.push(args[1..].join(" "));
    }
    let result = run_command_capture(program, &cmd_args, base_env, &[])?;
    Ok(format::format_tool_call(&result, tool_name))
}

fn dispatch_dashboard(
    program: &str,
    base_env: &[(String, String)],
) -> Result<FormattedOutput, String> {
    let status = run_command_capture(program, &["status".into()], base_env, &[])?;
    let servers = run_command_capture(program, &["servers".into(), "list".into()], base_env, &[])?;
    let tools = run_command_capture(program, &["tools".into(), "list".into()], base_env, &[])?;
    let sync = run_command_capture(program, &["sync".into(), "status".into()], base_env, &[])?;
    let session = run_command_capture(
        program,
        &[
            "agent".into(),
            "session".into(),
            "--agent-id".into(),
            "zed-loom".into(),
        ],
        base_env,
        &[],
    )?;

    let parts: Vec<(&str, &format::CommandResult)> = vec![
        ("Status", &status),
        ("Servers", &servers),
        ("Tools", &tools),
        ("Sync", &sync),
        ("Session", &session),
    ];
    Ok(format::format_dashboard(&parts))
}

fn dispatch_help(args: &[String]) -> FormattedOutput {
    let sub = args.first().map(|s| s.as_str()).unwrap_or("");

    if !sub.is_empty() {
        return command_help(sub);
    }

    let text = r#"## 📖 Loom Commands

| Command | Description |
| --- | --- |
| `/loom-check` | Run `loom check` diagnostics |
| `/loom-status` | Show daemon and server status |
| `/loom-sync [platform]` | Sync config (status, zed, vscode, claude, gemini, codex, antigravity, kilocode) |
| `/loom-restart` | Restart the Loom daemon |
| `/loom-start` | Start the Loom daemon |
| `/loom-stop` | Stop the Loom daemon |
| `/loom-tools [list\|search <q>]` | List or search available MCP tools |
| `/loom-servers` | List registered MCP servers |
| `/loom-ping` | Quick health check |
| `/loom-secrets [list\|validate]` | Manage secrets |
| `/loom-session [start\|end\|status\|list]` | Agent session management |
| `/loom-heartbeat` | Send agent heartbeat |
| `/loom-task [list\|add\|update]` | Agent task management |
| `/loom-recall <query>` | Recall context from agent memory |
| `/loom-skills [list\|search\|categories]` | Browse available skills |
| `/loom-search <query>` | Deep search across sources |
| `/loom-profile [current\|list\|switch]` | Profile management |
| `/loom-call <tool> [json]` | Invoke any MCP tool directly |
| `/loom-dashboard` | Composite overview dashboard |
| `/loom-help [command]` | Show this help or command details |

Use `/loom-help <command>` for detailed usage.
"#
    .to_string();

    FormattedOutput::plain(text)
}

fn command_help(cmd: &str) -> FormattedOutput {
    let text = match cmd {
        "check" => "## `/loom-check`\n\nRun `loom check` and return a diagnostic report.\n\n**Usage**: `/loom-check`\n\nNo arguments required.\n",
        "status" => "## `/loom-status`\n\nShow Loom daemon and server status.\n\n**Usage**: `/loom-status`\n\nNo arguments required.\n",
        "sync" => "## `/loom-sync`\n\nRun Loom config sync.\n\n**Usage**:\n- `/loom-sync` — show sync status\n- `/loom-sync status` — show sync status\n- `/loom-sync <platform>` — sync a specific platform (`--regen`)\n\n**Platforms**: zed, vscode, claude, gemini, codex, antigravity, kilocode\n",
        "restart" => "## `/loom-restart`\n\nRestart the Loom daemon.\n\n**Usage**: `/loom-restart`\n",
        "start" => "## `/loom-start`\n\nStart the Loom daemon.\n\n**Usage**: `/loom-start`\n",
        "stop" => "## `/loom-stop`\n\nStop the Loom daemon.\n\n**Usage**: `/loom-stop`\n",
        "tools" => "## `/loom-tools`\n\nList or search available MCP tools.\n\n**Usage**:\n- `/loom-tools` — list all tools\n- `/loom-tools list` — list all tools\n- `/loom-tools search <query>` — search by name or description\n",
        "servers" => "## `/loom-servers`\n\nList registered MCP servers with status.\n\n**Usage**: `/loom-servers`\n",
        "ping" => "## `/loom-ping`\n\nQuick daemon + hub reachability check.\n\n**Usage**: `/loom-ping`\n",
        "secrets" => "## `/loom-secrets`\n\nManage secrets.\n\n**Usage**:\n- `/loom-secrets` — list secret names (never values)\n- `/loom-secrets list` — list secret names\n- `/loom-secrets validate` — validate all secrets are set\n",
        "session" => "## `/loom-session`\n\nAgent session management.\n\n**Usage**:\n- `/loom-session` — show current session\n- `/loom-session status` — show current session\n- `/loom-session start [namespace]` — start a new session\n- `/loom-session end` — end current session\n- `/loom-session list` — list recent sessions\n",
        "heartbeat" => "## `/loom-heartbeat`\n\nSend an agent heartbeat signal.\n\n**Usage**: `/loom-heartbeat`\n",
        "task" => "## `/loom-task`\n\nAgent task management.\n\n**Usage**:\n- `/loom-task` — list tasks\n- `/loom-task list` — list tasks\n- `/loom-task add <description>` — add a new task\n- `/loom-task update <id> <status>` — update task status (pending/in_progress/completed)\n",
        "recall" => "## `/loom-recall`\n\nRecall context from agent memory.\n\n**Usage**: `/loom-recall <query>`\n\nRequires a search query.\n",
        "skills" => "## `/loom-skills`\n\nBrowse available skills.\n\n**Usage**:\n- `/loom-skills` — list all skills\n- `/loom-skills list` — list all skills\n- `/loom-skills search <query>` — search by keyword\n- `/loom-skills categories` — show categories\n",
        "search" => "## `/loom-search`\n\nDeep search across configured sources.\n\n**Usage**: `/loom-search <query>`\n\nRequires a search query.\n",
        "profile" => "## `/loom-profile`\n\nProfile management.\n\n**Usage**:\n- `/loom-profile` — show current profile\n- `/loom-profile current` — show current profile\n- `/loom-profile list` — list all profiles\n- `/loom-profile switch <name>` — switch profile\n",
        "call" => "## `/loom-call`\n\nInvoke any MCP tool directly.\n\n**Usage**: `/loom-call <tool_name> [json_args]`\n\nExample: `/loom-call agent_memory_recall {\"query\": \"auth\"}`\n",
        "dashboard" => "## `/loom-dashboard`\n\nComposite overview combining status, servers, tools, sync, and session info.\n\n**Usage**: `/loom-dashboard`\n\nNo arguments required.\n",
        "help" => "## `/loom-help`\n\nShow help for all commands or a specific command.\n\n**Usage**:\n- `/loom-help` — list all commands\n- `/loom-help <command>` — show details for one command\n",
        _ => &format!("Unknown command `{}`. Use `/loom-help` to see all commands.\n", cmd),
    };

    FormattedOutput::plain(text.to_string())
}

zed::register_extension!(LoomExtension);
