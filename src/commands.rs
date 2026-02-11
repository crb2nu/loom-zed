use zed_extension_api as zed;

use crate::format::CommandResult;

/// Execute a command and capture its output as a structured `CommandResult`.
pub(crate) fn run_command_capture(
    program: &str,
    args: &[String],
    base_env: &[(String, String)],
    extra_env: &[(String, String)],
) -> Result<CommandResult, String> {
    let mut cmd = zed::process::Command::new(program).args(args.iter().cloned());
    for (k, v) in base_env.iter().chain(extra_env.iter()) {
        cmd = cmd.env(k, v);
    }
    let output = cmd.output()?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let exit_code = output
        .status
        .map(|s| s.to_string())
        .unwrap_or_else(|| "unknown".into());

    Ok(CommandResult {
        exit_code,
        stdout: truncate_output(&stdout, 40_000),
        stderr: truncate_output(&stderr, 40_000),
    })
}

pub(crate) fn truncate_output(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        return s.to_string();
    }
    let mut out: String = s.chars().take(max_chars).collect();
    out.push_str("\n\n[output truncated]\n");
    out
}

pub(crate) fn join_args(args: &[String]) -> String {
    if args.is_empty() {
        return "".to_string();
    }
    args.join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn join_args_empty() {
        let args: Vec<String> = vec![];
        assert_eq!(join_args(&args), "");
    }

    #[test]
    fn truncate_within_limit() {
        let s = "hello world";
        let result = truncate_output(s, 100);
        assert_eq!(result, "hello world");
    }

    #[test]
    fn truncate_exceeds_limit() {
        let s = "abcdefghij"; // 10 chars
        let result = truncate_output(s, 5);
        assert!(result.starts_with("abcde"));
        assert!(result.contains("[output truncated]"));
    }
}
