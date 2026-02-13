use std::collections::HashMap;
use std::sync::Mutex;

use crate::commands::run_command_capture;
use crate::completions;
use crate::download::{self, LoomInstall};
use crate::env::{current_path_sep, shell_env_to_vec, upsert_env, with_path_prefix};
use crate::format::{
    self, format_daemon_action, format_diagnostic_report, format_generic, format_status_report,
    format_sync_report, FormattedOutput,
};
use crate::help::dispatch_help;
use crate::log::{log_msg, LogLevel};
use crate::settings::LoomRuntimeSettings;
use zed_extension_api as zed;

// ---------------------------------------------------------------------------
// Binary resolution (shared between context server + slash commands)
// ---------------------------------------------------------------------------

fn resolve_loom_path_from_host() -> String {
    // Try `which loom` through the host.
    if let Ok(output) = zed::process::Command::new("which").arg("loom").output() {
        if output.status == Some(0) {
            let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !path.is_empty() {
                return path;
            }
        }
    }

    // Check well-known locations.
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

/// Resolve the loom binary path and build the base environment.
pub(crate) fn resolve_binary(
    installs: &Mutex<HashMap<String, LoomInstall>>,
    worktree: Option<&zed_extension_api::Worktree>,
    runtime_settings: Option<&LoomRuntimeSettings>,
) -> Result<(String, Vec<(String, String)>), String> {
    let mut base_env = worktree
        .map(|wt| shell_env_to_vec(&wt.shell_env()))
        .unwrap_or_default();
    if base_env.is_empty() {
        if let Ok(path) = std::env::var("PATH") {
            base_env.push(("PATH".to_string(), path));
        }
    }

    if let Some(rt) = runtime_settings {
        for (k, v) in &rt.command_env {
            upsert_env(&mut base_env, k, v);
        }
    }

    // Resolve local binary candidate.
    let explicit = runtime_settings
        .and_then(|rt| rt.command_path.as_ref())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    if let Some(path) = explicit {
        return Ok((path, base_env));
    }

    if let Some(wt) = worktree {
        if let Some(path) = wt.which("loom") {
            return Ok((path, base_env));
        }
    }

    let local_path = resolve_loom_path_from_host();
    let have_local = local_path != "loom";

    let download_settings = runtime_settings
        .map(|rt| rt.extension.download.clone())
        .unwrap_or_default();

    if have_local {
        Ok((local_path, base_env))
    } else if download_settings.enabled() {
        log_msg(
            LogLevel::Info,
            &format!(
                "slash command: downloading loom-core from {}",
                download_settings.repo()
            ),
        );
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
pub(crate) fn dispatch_command(
    command_name: &str,
    args: &[String],
    program: &str,
    base_env: &[(String, String)],
) -> Result<FormattedOutput, String> {
    match command_name {
        "loom-info" => dispatch_info(program, base_env),
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

fn dispatch_info(program: &str, base_env: &[(String, String)]) -> Result<FormattedOutput, String> {
    // Keep this lightweight and robust: `loom version` might not exist on all builds.
    let version = run_command_capture(program, &["version".into()], base_env, &[])
        .or_else(|_| run_command_capture(program, &["--version".into()], base_env, &[]));

    let mut text = String::new();
    text.push_str("## Loom Extension Info\n\n");
    text.push_str(&format!("**Binary**: `{}`\n\n", program));

    match version {
        Ok(v) => {
            if !v.stdout.trim().is_empty() {
                text.push_str("### Version\n\n");
                text.push_str(&format!("```\n{}\n```\n\n", v.stdout.trim()));
            } else if !v.stderr.trim().is_empty() {
                text.push_str("### Version (stderr)\n\n");
                text.push_str(&format!("```\n{}\n```\n\n", v.stderr.trim()));
            }
        }
        Err(e) => {
            text.push_str("### Version\n\n");
            text.push_str(&format!("Unable to determine version: `{}`\n\n", e));
        }
    }

    Ok(FormattedOutput::plain(text))
}

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
