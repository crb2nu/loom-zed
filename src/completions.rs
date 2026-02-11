use zed_extension_api as zed;

/// Known sync platforms (matches loom CLI targets).
const SYNC_PLATFORMS: &[(&str, &str)] = &[
    ("status", "Show sync status across all platforms"),
    ("zed", "Sync Zed editor config (--regen)"),
    ("vscode", "Sync VS Code config (--regen)"),
    ("claude", "Sync Claude Code config (--regen)"),
    ("gemini", "Sync Gemini CLI config (--regen)"),
    ("codex", "Sync Codex config (--regen)"),
    ("antigravity", "Sync Antigravity config (--regen)"),
    ("kilocode", "Sync Kilocode config (--regen)"),
];

/// Known sub-commands for /loom-tools.
const TOOLS_SUBS: &[(&str, &str)] = &[
    ("list", "List all available tools"),
    ("search", "Search tools by name or description"),
];

/// Known sub-commands for /loom-secrets.
const SECRETS_SUBS: &[(&str, &str)] = &[
    ("list", "List secret names with set/missing status"),
    ("validate", "Validate all secrets are properly configured"),
];

/// Known sub-commands for /loom-session.
const SESSION_SUBS: &[(&str, &str)] = &[
    ("status", "Show current session status"),
    ("start", "Start a new agent session"),
    ("end", "End the current agent session"),
    ("list", "List recent sessions"),
];

/// Known sub-commands for /loom-task.
const TASK_SUBS: &[(&str, &str)] = &[
    ("list", "List agent tasks"),
    ("add", "Add a new task (provide description after)"),
    ("update", "Update a task (provide task ID and status after)"),
];

/// Task status completions (for second arg of /loom-task update).
const TASK_STATUSES: &[(&str, &str)] = &[
    ("pending", "Task is waiting to be started"),
    ("in_progress", "Task is currently being worked on"),
    ("completed", "Task is finished"),
];

/// Known sub-commands for /loom-skills.
const SKILLS_SUBS: &[(&str, &str)] = &[
    ("list", "List all available skills"),
    ("search", "Search skills by keyword"),
    ("categories", "Show skill categories"),
];

/// Known sub-commands for /loom-profile.
const PROFILE_SUBS: &[(&str, &str)] = &[
    ("current", "Show the active profile"),
    ("list", "List all profiles"),
    ("switch", "Switch to a different profile"),
];

/// Dispatch argument completions for any slash command.
pub(crate) fn complete_argument(
    command: &str,
    args: &[String],
) -> Vec<zed::SlashCommandArgumentCompletion> {
    match command {
        "loom-sync" => filter_completions(SYNC_PLATFORMS, query_from_args(args)),
        "loom-tools" => complete_tools(args),
        "loom-secrets" => filter_completions(SECRETS_SUBS, query_from_args(args)),
        "loom-session" => filter_completions(SESSION_SUBS, query_from_args(args)),
        "loom-task" => complete_task(args),
        "loom-skills" => filter_completions(SKILLS_SUBS, query_from_args(args)),
        "loom-profile" => filter_completions(PROFILE_SUBS, query_from_args(args)),
        "loom-help" => complete_help(args),
        _ => Vec::new(),
    }
}

/// Tools: first arg is sub-command, second arg after "search" is free-form.
fn complete_tools(args: &[String]) -> Vec<zed::SlashCommandArgumentCompletion> {
    if args.len() <= 1 {
        filter_completions(TOOLS_SUBS, query_from_args(args))
    } else {
        Vec::new() // free-form search query
    }
}

/// Task: first arg is sub-command, second arg after "update" may be task ID (free-form),
/// third arg after "update <id>" is status.
fn complete_task(args: &[String]) -> Vec<zed::SlashCommandArgumentCompletion> {
    match args.len() {
        0 | 1 => filter_completions(TASK_SUBS, query_from_args(args)),
        3 => {
            if args.first().map(|s| s.as_str()) == Some("update") {
                filter_completions(TASK_STATUSES, query_from_args(&args[2..]))
            } else {
                Vec::new()
            }
        }
        _ => Vec::new(),
    }
}

/// Help: complete with known command names.
fn complete_help(args: &[String]) -> Vec<zed::SlashCommandArgumentCompletion> {
    let commands: &[(&str, &str)] = &[
        ("check", "Run diagnostics"),
        ("status", "Show daemon status"),
        ("sync", "Config sync"),
        ("restart", "Restart daemon"),
        ("start", "Start daemon"),
        ("stop", "Stop daemon"),
        ("tools", "List/search tools"),
        ("servers", "List servers"),
        ("ping", "Health check"),
        ("secrets", "Manage secrets"),
        ("session", "Agent sessions"),
        ("heartbeat", "Agent heartbeat"),
        ("task", "Agent tasks"),
        ("recall", "Context recall"),
        ("skills", "Browse skills"),
        ("search", "Deep search"),
        ("profile", "Profile management"),
        ("call", "Invoke MCP tool"),
        ("dashboard", "Overview dashboard"),
    ];
    filter_completions(commands, query_from_args(args))
}

/// Extract the query string from the args (the last partial arg being typed).
fn query_from_args(args: &[String]) -> &str {
    args.last().map(|s| s.as_str()).unwrap_or("")
}

/// Filter a static list of (label, description) pairs by query prefix.
fn filter_completions(
    options: &[(&str, &str)],
    query: &str,
) -> Vec<zed::SlashCommandArgumentCompletion> {
    let q = query.to_lowercase();
    options
        .iter()
        .filter(|(label, _)| q.is_empty() || label.starts_with(&q))
        .map(|(label, _desc)| zed::SlashCommandArgumentCompletion {
            label: label.to_string(),
            new_text: label.to_string(),
            run_command: true,
        })
        .collect()
}

/// Validate that a platform name is known for sync operations.
pub(crate) fn is_valid_sync_platform(platform: &str) -> bool {
    SYNC_PLATFORMS
        .iter()
        .any(|(label, _)| *label == platform.to_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sync_completions_no_query() {
        let results = complete_argument("loom-sync", &[]);
        assert_eq!(results.len(), SYNC_PLATFORMS.len());
        assert_eq!(results[0].label, "status");
    }

    #[test]
    fn sync_completions_partial_query() {
        let results = complete_argument("loom-sync", &["cl".to_string()]);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].label, "claude");
    }

    #[test]
    fn sync_completions_no_match() {
        let results = complete_argument("loom-sync", &["xyz".to_string()]);
        assert!(results.is_empty());
    }

    #[test]
    fn tools_first_arg_completions() {
        let results = complete_argument("loom-tools", &[]);
        assert_eq!(results.len(), TOOLS_SUBS.len());
    }

    #[test]
    fn tools_search_no_further_completions() {
        let results = complete_argument("loom-tools", &["search".to_string(), "foo".to_string()]);
        assert!(results.is_empty());
    }

    #[test]
    fn secrets_completions() {
        let results = complete_argument("loom-secrets", &[]);
        assert_eq!(results.len(), SECRETS_SUBS.len());
    }

    #[test]
    fn session_completions() {
        let results = complete_argument("loom-session", &[]);
        assert_eq!(results.len(), SESSION_SUBS.len());
    }

    #[test]
    fn task_first_arg() {
        let results = complete_argument("loom-task", &[]);
        assert_eq!(results.len(), TASK_SUBS.len());
    }

    #[test]
    fn task_update_status_completions() {
        let results = complete_argument(
            "loom-task",
            &["update".to_string(), "abc123".to_string(), "".to_string()],
        );
        assert_eq!(results.len(), TASK_STATUSES.len());
    }

    #[test]
    fn unknown_command_no_completions() {
        let results = complete_argument("loom-unknown", &[]);
        assert!(results.is_empty());
    }

    #[test]
    fn valid_sync_platforms() {
        assert!(is_valid_sync_platform("zed"));
        assert!(is_valid_sync_platform("claude"));
        assert!(is_valid_sync_platform("status"));
        assert!(!is_valid_sync_platform("invalid"));
    }

    #[test]
    fn help_completions() {
        let results = complete_argument("loom-help", &[]);
        assert!(!results.is_empty());
        let labels: Vec<&str> = results.iter().map(|c| c.label.as_str()).collect();
        assert!(labels.contains(&"check"));
        assert!(labels.contains(&"sync"));
    }

    #[test]
    fn skills_completions() {
        let results = complete_argument("loom-skills", &[]);
        assert_eq!(results.len(), SKILLS_SUBS.len());
    }

    #[test]
    fn profile_completions() {
        let results = complete_argument("loom-profile", &[]);
        assert_eq!(results.len(), PROFILE_SUBS.len());
    }
}
