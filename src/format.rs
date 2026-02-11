use zed_extension_api as zed;

/// Structured result from running a CLI command.
pub(crate) struct CommandResult {
    pub(crate) exit_code: String,
    pub(crate) stdout: String,
    pub(crate) stderr: String,
}

impl CommandResult {
    pub(crate) fn success(&self) -> bool {
        self.exit_code == "0"
    }
}

/// Formatted output ready for Zed's slash command response.
pub(crate) struct FormattedOutput {
    pub(crate) text: String,
    pub(crate) sections: Vec<zed::SlashCommandOutputSection>,
}

impl FormattedOutput {
    /// Create a simple output with no sections.
    pub(crate) fn plain(text: String) -> Self {
        Self {
            text,
            sections: Vec::new(),
        }
    }
}

/// Helper: append a labeled section and return the byte range.
fn push_section(
    buf: &mut String,
    sections: &mut Vec<zed::SlashCommandOutputSection>,
    label: &str,
    content: &str,
) {
    let start = buf.len() as u32;
    buf.push_str(content);
    let end = buf.len() as u32;
    sections.push(zed::SlashCommandOutputSection {
        range: zed::Range { start, end },
        label: label.to_string(),
    });
}

/// Status indicator emoji.
fn status_icon(ok: bool) -> &'static str {
    if ok {
        "✅"
    } else {
        "❌"
    }
}

// ---------------------------------------------------------------------------
// Per-command formatters
// ---------------------------------------------------------------------------

/// Format `loom check` output as a diagnostic report.
pub(crate) fn format_diagnostic_report(result: &CommandResult) -> FormattedOutput {
    let icon = status_icon(result.success());
    let mut text = String::new();
    let mut sections = Vec::new();

    push_section(
        &mut text,
        &mut sections,
        "Diagnostic Report",
        &format!("## {} Loom Diagnostic Report\n\n", icon),
    );

    if !result.stdout.trim().is_empty() {
        push_section(
            &mut text,
            &mut sections,
            "Details",
            &format!("```\n{}\n```\n\n", result.stdout.trim()),
        );
    }

    if !result.stderr.trim().is_empty() {
        push_section(
            &mut text,
            &mut sections,
            "Warnings",
            &format!(
                "### Warnings / Errors\n\n```\n{}\n```\n\n",
                result.stderr.trim()
            ),
        );
    }

    text.push_str(&format!("**Exit code**: `{}`\n", result.exit_code));

    FormattedOutput { text, sections }
}

/// Format `loom status` output.
pub(crate) fn format_status_report(result: &CommandResult) -> FormattedOutput {
    let icon = status_icon(result.success());
    let mut text = String::new();
    let mut sections = Vec::new();

    push_section(
        &mut text,
        &mut sections,
        "Status",
        &format!("## {} Loom Status\n\n", icon),
    );

    if !result.stdout.trim().is_empty() {
        push_section(
            &mut text,
            &mut sections,
            "Output",
            &format!("```\n{}\n```\n\n", result.stdout.trim()),
        );
    }

    if !result.stderr.trim().is_empty() {
        push_section(
            &mut text,
            &mut sections,
            "Errors",
            &format!("```\n{}\n```\n\n", result.stderr.trim()),
        );
    }

    FormattedOutput { text, sections }
}

/// Format `loom sync` output.
pub(crate) fn format_sync_report(
    result: &CommandResult,
    platform: Option<&str>,
) -> FormattedOutput {
    let icon = status_icon(result.success());
    let mut text = String::new();
    let mut sections = Vec::new();

    let title = match platform {
        Some(p) => format!("## {} Sync: {}\n\n", icon, p),
        None => format!("## {} Sync Status\n\n", icon),
    };
    push_section(&mut text, &mut sections, "Sync", &title);

    if !result.stdout.trim().is_empty() {
        // Try to render sync output as a table if it looks tabular.
        let stdout = result.stdout.trim();
        if looks_tabular(stdout) {
            push_section(
                &mut text,
                &mut sections,
                "Results",
                &format!("{}\n\n", to_markdown_table(stdout)),
            );
        } else {
            push_section(
                &mut text,
                &mut sections,
                "Results",
                &format!("```\n{}\n```\n\n", stdout),
            );
        }
    }

    if !result.stderr.trim().is_empty() {
        push_section(
            &mut text,
            &mut sections,
            "Errors",
            &format!("```\n{}\n```\n\n", result.stderr.trim()),
        );
    }

    FormattedOutput { text, sections }
}

/// Format `loom restart` / `loom start` / `loom stop` output.
pub(crate) fn format_daemon_action(result: &CommandResult, action: &str) -> FormattedOutput {
    let icon = status_icon(result.success());
    let mut text = String::new();
    let mut sections = Vec::new();

    push_section(
        &mut text,
        &mut sections,
        action,
        &format!("## {} Daemon {}\n\n", icon, capitalize(action),),
    );

    if !result.stdout.trim().is_empty() {
        push_section(
            &mut text,
            &mut sections,
            "Output",
            &format!("```\n{}\n```\n\n", result.stdout.trim()),
        );
    }

    if !result.stderr.trim().is_empty() && !result.success() {
        push_section(
            &mut text,
            &mut sections,
            "Errors",
            &format!("```\n{}\n```\n\n", result.stderr.trim()),
        );
    }

    FormattedOutput { text, sections }
}

/// Generic fallback formatter.
pub(crate) fn format_generic(result: &CommandResult, title: &str) -> FormattedOutput {
    let icon = status_icon(result.success());
    let mut text = String::new();
    let mut sections = Vec::new();

    push_section(
        &mut text,
        &mut sections,
        title,
        &format!("## {} {}\n\n", icon, title),
    );

    if !result.stdout.trim().is_empty() {
        push_section(
            &mut text,
            &mut sections,
            "Output",
            &format!("```\n{}\n```\n\n", result.stdout.trim()),
        );
    }

    if !result.stderr.trim().is_empty() {
        push_section(
            &mut text,
            &mut sections,
            "Errors",
            &format!("```\n{}\n```\n\n", result.stderr.trim()),
        );
    }

    text.push_str(&format!("**Exit code**: `{}`\n", result.exit_code));

    FormattedOutput { text, sections }
}

/// Format a Markdown table for tools listing.
pub(crate) fn format_tools_table(result: &CommandResult) -> FormattedOutput {
    let icon = status_icon(result.success());
    let mut text = String::new();
    let mut sections = Vec::new();

    push_section(
        &mut text,
        &mut sections,
        "Tools",
        &format!("## {} Loom Tools\n\n", icon),
    );

    if !result.stdout.trim().is_empty() {
        let stdout = result.stdout.trim();
        if looks_tabular(stdout) {
            push_section(
                &mut text,
                &mut sections,
                "Tool List",
                &format!("{}\n\n", to_markdown_table(stdout)),
            );
        } else {
            push_section(
                &mut text,
                &mut sections,
                "Tool List",
                &format!("```\n{}\n```\n\n", stdout),
            );
        }
    }

    if !result.stderr.trim().is_empty() {
        push_section(
            &mut text,
            &mut sections,
            "Errors",
            &format!("```\n{}\n```\n\n", result.stderr.trim()),
        );
    }

    FormattedOutput { text, sections }
}

/// Format server listing.
pub(crate) fn format_servers_list(result: &CommandResult) -> FormattedOutput {
    format_generic(result, "Loom Servers")
}

/// Format health/ping check.
pub(crate) fn format_ping(result: &CommandResult) -> FormattedOutput {
    let icon = status_icon(result.success());
    let mut text = String::new();
    let mut sections = Vec::new();

    push_section(
        &mut text,
        &mut sections,
        "Health",
        &format!("## {} Loom Health\n\n", icon),
    );

    if result.success() {
        text.push_str("Daemon is **reachable** and responding.\n\n");
    } else {
        text.push_str("Daemon is **not reachable**.\n\n");
    }

    if !result.stdout.trim().is_empty() {
        push_section(
            &mut text,
            &mut sections,
            "Details",
            &format!("```\n{}\n```\n\n", result.stdout.trim()),
        );
    }

    FormattedOutput { text, sections }
}

/// Format secrets listing.
pub(crate) fn format_secrets(result: &CommandResult, sub: &str) -> FormattedOutput {
    let title = match sub {
        "validate" => "Secrets Validation",
        _ => "Secrets",
    };
    format_generic(result, title)
}

/// Format session command output.
pub(crate) fn format_session(result: &CommandResult, sub: &str) -> FormattedOutput {
    let title = match sub {
        "start" => "Session Started",
        "end" => "Session Ended",
        "list" => "Sessions",
        _ => "Session Status",
    };
    format_generic(result, title)
}

/// Format task command output.
pub(crate) fn format_task(result: &CommandResult, sub: &str) -> FormattedOutput {
    let title = match sub {
        "add" => "Task Added",
        "update" => "Task Updated",
        _ => "Tasks",
    };
    format_generic(result, title)
}

/// Format recall output.
pub(crate) fn format_recall(result: &CommandResult) -> FormattedOutput {
    let mut text = String::new();
    let mut sections = Vec::new();

    push_section(
        &mut text,
        &mut sections,
        "Recall",
        "## 🔍 Context Recall\n\n",
    );

    if !result.stdout.trim().is_empty() {
        push_section(
            &mut text,
            &mut sections,
            "Results",
            &format!("{}\n\n", result.stdout.trim()),
        );
    }

    if !result.stderr.trim().is_empty() && !result.success() {
        push_section(
            &mut text,
            &mut sections,
            "Errors",
            &format!("```\n{}\n```\n\n", result.stderr.trim()),
        );
    }

    FormattedOutput { text, sections }
}

/// Format skills listing.
pub(crate) fn format_skills(result: &CommandResult) -> FormattedOutput {
    format_generic(result, "Loom Skills")
}

/// Format search results.
pub(crate) fn format_search(result: &CommandResult) -> FormattedOutput {
    let mut text = String::new();
    let mut sections = Vec::new();

    push_section(
        &mut text,
        &mut sections,
        "Search",
        "## 🔍 Search Results\n\n",
    );

    if !result.stdout.trim().is_empty() {
        push_section(
            &mut text,
            &mut sections,
            "Results",
            &format!("{}\n\n", result.stdout.trim()),
        );
    }

    if !result.stderr.trim().is_empty() && !result.success() {
        push_section(
            &mut text,
            &mut sections,
            "Errors",
            &format!("```\n{}\n```\n\n", result.stderr.trim()),
        );
    }

    FormattedOutput { text, sections }
}

/// Format profile command output.
pub(crate) fn format_profile(result: &CommandResult, sub: &str) -> FormattedOutput {
    let title = match sub {
        "list" => "Profiles",
        "switch" => "Profile Switched",
        _ => "Current Profile",
    };
    format_generic(result, title)
}

/// Format generic tool call output.
pub(crate) fn format_tool_call(result: &CommandResult, tool_name: &str) -> FormattedOutput {
    let icon = status_icon(result.success());
    let mut text = String::new();
    let mut sections = Vec::new();

    push_section(
        &mut text,
        &mut sections,
        tool_name,
        &format!("## {} Tool: `{}`\n\n", icon, tool_name),
    );

    if !result.stdout.trim().is_empty() {
        push_section(
            &mut text,
            &mut sections,
            "Output",
            &format!("```json\n{}\n```\n\n", result.stdout.trim()),
        );
    }

    if !result.stderr.trim().is_empty() && !result.success() {
        push_section(
            &mut text,
            &mut sections,
            "Errors",
            &format!("```\n{}\n```\n\n", result.stderr.trim()),
        );
    }

    FormattedOutput { text, sections }
}

/// Format composite dashboard output from multiple command results.
pub(crate) fn format_dashboard(parts: &[(&str, &CommandResult)]) -> FormattedOutput {
    let mut text = String::new();
    let mut sections = Vec::new();

    push_section(
        &mut text,
        &mut sections,
        "Dashboard",
        "## 📊 Loom Dashboard\n\n",
    );

    for (label, result) in parts {
        let icon = status_icon(result.success());
        push_section(
            &mut text,
            &mut sections,
            label,
            &format!(
                "### {} {}\n\n```\n{}\n```\n\n",
                icon,
                label,
                if result.stdout.trim().is_empty() {
                    result.stderr.trim()
                } else {
                    result.stdout.trim()
                },
            ),
        );
    }

    FormattedOutput { text, sections }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        None => String::new(),
        Some(first) => {
            let upper: String = first.to_uppercase().collect();
            upper + c.as_str()
        }
    }
}

/// Heuristic: output looks tabular if most non-empty lines have 2+ whitespace-separated columns.
fn looks_tabular(s: &str) -> bool {
    let lines: Vec<&str> = s.lines().filter(|l| !l.trim().is_empty()).collect();
    if lines.len() < 2 {
        return false;
    }
    let multi_col = lines
        .iter()
        .filter(|l| l.split_whitespace().count() >= 2)
        .count();
    multi_col * 2 >= lines.len()
}

/// Best-effort conversion of whitespace-aligned CLI output to a Markdown table.
fn to_markdown_table(s: &str) -> String {
    let lines: Vec<&str> = s.lines().filter(|l| !l.trim().is_empty()).collect();
    if lines.is_empty() {
        return String::new();
    }

    // Use the first line as header.
    let header_cols: Vec<&str> = lines[0].split_whitespace().collect();
    let ncols = header_cols.len();
    if ncols == 0 {
        return format!("```\n{}\n```", s);
    }

    let mut table = String::new();
    table.push_str("| ");
    table.push_str(&header_cols.join(" | "));
    table.push_str(" |\n|");
    for _ in 0..ncols {
        table.push_str(" --- |");
    }
    table.push('\n');

    for line in &lines[1..] {
        let cols: Vec<&str> = line.splitn(ncols, char::is_whitespace).collect();
        let cols: Vec<&str> = cols.iter().map(|c| c.trim()).collect();
        table.push_str("| ");
        // Pad to ncols if needed.
        let mut row = Vec::with_capacity(ncols);
        for i in 0..ncols {
            row.push(cols.get(i).copied().unwrap_or(""));
        }
        table.push_str(&row.join(" | "));
        table.push_str(" |\n");
    }

    table
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mock_result(exit_code: &str, stdout: &str, stderr: &str) -> CommandResult {
        CommandResult {
            exit_code: exit_code.to_string(),
            stdout: stdout.to_string(),
            stderr: stderr.to_string(),
        }
    }

    #[test]
    fn diagnostic_report_success() {
        let r = mock_result("0", "all checks passed", "");
        let out = format_diagnostic_report(&r);
        assert!(out.text.contains("✅"));
        assert!(out.text.contains("all checks passed"));
        assert!(!out.sections.is_empty());
    }

    #[test]
    fn diagnostic_report_failure() {
        let r = mock_result("1", "", "connection refused");
        let out = format_diagnostic_report(&r);
        assert!(out.text.contains("❌"));
        assert!(out.text.contains("connection refused"));
    }

    #[test]
    fn status_report_sections() {
        let r = mock_result("0", "daemon running\nservers: 3", "");
        let out = format_status_report(&r);
        assert!(out.sections.len() >= 2);
        assert_eq!(out.sections[0].label, "Status");
    }

    #[test]
    fn sync_report_with_platform() {
        let r = mock_result("0", "synced 5 servers", "");
        let out = format_sync_report(&r, Some("zed"));
        assert!(out.text.contains("Sync: zed"));
    }

    #[test]
    fn sync_report_no_platform() {
        let r = mock_result("0", "all in sync", "");
        let out = format_sync_report(&r, None);
        assert!(out.text.contains("Sync Status"));
    }

    #[test]
    fn daemon_action_restart() {
        let r = mock_result("0", "restarted", "");
        let out = format_daemon_action(&r, "restart");
        assert!(out.text.contains("Restart"));
        assert!(out.text.contains("✅"));
    }

    #[test]
    fn generic_formatter_includes_exit_code() {
        let r = mock_result("2", "some output", "some error");
        let out = format_generic(&r, "Test");
        assert!(out.text.contains("Exit code"));
        assert!(out.text.contains("`2`"));
    }

    #[test]
    fn section_ranges_are_contiguous() {
        let r = mock_result("0", "output here", "warning here");
        let out = format_diagnostic_report(&r);
        for i in 1..out.sections.len() {
            assert!(
                out.sections[i].range.start >= out.sections[i - 1].range.end
                    || out.sections[i].range.start == out.sections[i - 1].range.end,
                "sections should not overlap"
            );
        }
    }

    #[test]
    fn plain_output_has_no_sections() {
        let out = FormattedOutput::plain("hello".to_string());
        assert!(out.sections.is_empty());
        assert_eq!(out.text, "hello");
    }

    #[test]
    fn looks_tabular_detects_tables() {
        assert!(looks_tabular("NAME  STATUS\nfoo   ok\nbar   fail"));
        assert!(!looks_tabular("just a single line"));
        assert!(!looks_tabular(""));
    }

    #[test]
    fn to_markdown_table_basic() {
        let input = "NAME STATUS\nfoo ok\nbar fail";
        let table = to_markdown_table(input);
        assert!(table.contains("| NAME | STATUS |"));
        assert!(table.contains("| foo | ok |"));
    }

    #[test]
    fn capitalize_works() {
        assert_eq!(capitalize("restart"), "Restart");
        assert_eq!(capitalize(""), "");
        assert_eq!(capitalize("a"), "A");
    }

    #[test]
    fn dashboard_multiple_sections() {
        let r1 = mock_result("0", "running", "");
        let r2 = mock_result("1", "", "unreachable");
        let parts: Vec<(&str, &CommandResult)> = vec![("Status", &r1), ("Hub", &r2)];
        let out = format_dashboard(&parts);
        assert!(out.text.contains("Dashboard"));
        assert!(out.text.contains("Status"));
        assert!(out.text.contains("Hub"));
        assert!(out.sections.len() >= 3); // dashboard header + 2 parts
    }

    #[test]
    fn ping_success() {
        let r = mock_result("0", "ok", "");
        let out = format_ping(&r);
        assert!(out.text.contains("reachable"));
    }

    #[test]
    fn ping_failure() {
        let r = mock_result("1", "", "");
        let out = format_ping(&r);
        assert!(out.text.contains("not reachable"));
    }
}
