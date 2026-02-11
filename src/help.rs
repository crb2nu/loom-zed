use crate::format::FormattedOutput;

pub(crate) fn dispatch_help(args: &[String]) -> FormattedOutput {
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
