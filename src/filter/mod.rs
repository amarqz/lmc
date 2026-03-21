use crate::config::NoiseFilterConfig;
use crate::db::CommandRecord;

/// Check if a command is noisy based on its first token against the ignore list.
/// This is a context-free check — no surrounding commands considered.
pub fn is_noisy(cmd: &str, config: &NoiseFilterConfig) -> bool {
    let trimmed = cmd.trim();
    if trimmed.is_empty() {
        return true;
    }

    let first_token = trimmed.split_whitespace().next().unwrap_or("");
    // Strip path prefix: /usr/bin/ls -> ls
    let base_command = first_token.rsplit('/').next().unwrap_or(first_token);

    config.ignored_commands.iter().any(|ignored| ignored == base_command)
}

/// Evaluate a sequence of commands (one session) and return noisy flags.
/// Rules (in order):
/// 1. Non-zero exit code → never noisy
/// 2. Not in ignored list → not noisy
/// 3. Sandwich rule: ignored command with meaningful commands both before AND after → not noisy
/// 4. Otherwise → noisy
pub fn mark_noisy(commands: &[CommandRecord], config: &NoiseFilterConfig) -> Vec<bool> {
    if commands.is_empty() {
        return vec![];
    }

    let n = commands.len();
    let is_ignored: Vec<bool> = commands.iter().map(|c| is_noisy(&c.cmd, config)).collect();
    let is_failed: Vec<bool> = commands
        .iter()
        .map(|c| c.exit_code.is_some_and(|code| code != 0))
        .collect();

    // Rules 1 & 2: a command is "kept" (not noisy) if it failed OR is not on the ignore list
    let mut kept: Vec<bool> = (0..n)
        .map(|i| is_failed[i] || !is_ignored[i])
        .collect();

    // Rule 3 (sandwich): an ignored non-failed command is kept if there's a kept command
    // before AND after it, using only rules-1&2 anchors for the sandwich check
    let has_kept_before: Vec<bool> = {
        let mut seen = false;
        kept.iter()
            .map(|&k| {
                let result = seen;
                if k {
                    seen = true;
                }
                result
            })
            .collect()
    };

    let has_kept_after: Vec<bool> = {
        let mut seen = false;
        kept.iter()
            .rev()
            .map(|&k| {
                let result = seen;
                if k {
                    seen = true;
                }
                result
            })
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect()
    };

    for i in 0..n {
        if !kept[i] && has_kept_before[i] && has_kept_after[i] {
            kept[i] = true; // sandwiched — keep it
        }
    }

    // Rule 4 (neighbor spreading): a failed ignored command spreads "kept" to its
    // immediate neighbors, since failure context makes adjacent commands relevant
    for i in 0..n {
        if is_failed[i] && is_ignored[i] {
            if i > 0 {
                kept[i - 1] = true;
            }
            if i + 1 < n {
                kept[i + 1] = true;
            }
        }
    }

    // noisy = not kept
    kept.iter().map(|&k| !k).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_config() -> NoiseFilterConfig {
        NoiseFilterConfig::default()
    }

    #[test]
    fn test_ignored_command_is_noisy() {
        assert!(is_noisy("ls", &default_config()));
        assert!(is_noisy("cd", &default_config()));
        assert!(is_noisy("pwd", &default_config()));
        assert!(is_noisy("clear", &default_config()));
    }

    #[test]
    fn test_meaningful_command_not_noisy() {
        assert!(!is_noisy("cargo build", &default_config()));
        assert!(!is_noisy("kubectl get pods", &default_config()));
        assert!(!is_noisy("git status", &default_config()));
    }

    #[test]
    fn test_first_token_extraction_with_args() {
        assert!(is_noisy("ls -la /tmp", &default_config()));
        assert!(is_noisy("cd /home/user", &default_config()));
    }

    #[test]
    fn test_path_prefixed_command() {
        assert!(is_noisy("/usr/bin/ls", &default_config()));
        assert!(is_noisy("/bin/ls -la", &default_config()));
    }

    #[test]
    fn test_empty_command_is_noisy() {
        assert!(is_noisy("", &default_config()));
        assert!(is_noisy("   ", &default_config()));
    }

    #[test]
    fn test_custom_ignored_list() {
        let config = NoiseFilterConfig {
            ignored_commands: vec!["foo".to_string(), "bar".to_string()],
        };
        assert!(is_noisy("foo", &config));
        assert!(is_noisy("bar --flag", &config));
        assert!(!is_noisy("ls", &config)); // not in custom list
    }

    fn make_cmd(cmd: &str, exit_code: Option<i32>) -> CommandRecord {
        CommandRecord {
            id: None,
            cmd: cmd.to_string(),
            timestamp: 0,
            directory: "/tmp".to_string(),
            exit_code,
            session_id: "s1".to_string(),
            shell: "zsh".to_string(),
            noisy: false,
        }
    }

    #[test]
    fn test_mark_isolated_noisy_command() {
        let commands = vec![make_cmd("ls", Some(0))];
        let result = mark_noisy(&commands, &default_config());
        assert_eq!(result, vec![true]);
    }

    #[test]
    fn test_mark_sandwiched_noisy_command_kept() {
        let commands = vec![
            make_cmd("cargo build", Some(0)),
            make_cmd("ls", Some(0)),
            make_cmd("cargo test", Some(0)),
        ];
        let result = mark_noisy(&commands, &default_config());
        assert_eq!(result, vec![false, false, false]);
    }

    #[test]
    fn test_mark_failed_noisy_command_kept() {
        let commands = vec![make_cmd("cd /nonexistent", Some(1))];
        let result = mark_noisy(&commands, &default_config());
        assert_eq!(result, vec![false]);
    }

    #[test]
    fn test_mark_noisy_at_start_of_session() {
        let commands = vec![
            make_cmd("ls", Some(0)),
            make_cmd("cargo build", Some(0)),
            make_cmd("cargo test", Some(0)),
        ];
        let result = mark_noisy(&commands, &default_config());
        assert_eq!(result, vec![true, false, false]);
    }

    #[test]
    fn test_mark_noisy_at_end_of_session() {
        let commands = vec![
            make_cmd("cargo build", Some(0)),
            make_cmd("cargo test", Some(0)),
            make_cmd("ls", Some(0)),
        ];
        let result = mark_noisy(&commands, &default_config());
        assert_eq!(result, vec![false, false, true]);
    }

    #[test]
    fn test_mark_all_noisy_session() {
        let commands = vec![
            make_cmd("ls", Some(0)),
            make_cmd("cd /tmp", Some(0)),
            make_cmd("pwd", Some(0)),
        ];
        let result = mark_noisy(&commands, &default_config());
        assert_eq!(result, vec![true, true, true]);
    }

    #[test]
    fn test_mark_meaningful_command_unchanged() {
        let commands = vec![make_cmd("cargo build", Some(0))];
        let result = mark_noisy(&commands, &default_config());
        assert_eq!(result, vec![false]);
    }

    #[test]
    fn test_mark_noisy_multiple_sandwiched() {
        let commands = vec![
            make_cmd("cargo build", Some(0)),
            make_cmd("ls", Some(0)),
            make_cmd("cd src", Some(0)),
            make_cmd("cargo test", Some(0)),
        ];
        let result = mark_noisy(&commands, &default_config());
        assert_eq!(result, vec![false, false, false, false]);
    }

    #[test]
    fn test_mark_empty_input() {
        let commands: Vec<CommandRecord> = vec![];
        let result = mark_noisy(&commands, &default_config());
        assert!(result.is_empty());
    }

    #[test]
    fn test_mark_failed_noisy_among_all_noisy() {
        let commands = vec![
            make_cmd("ls", Some(0)),
            make_cmd("cd /bad", Some(1)),
            make_cmd("pwd", Some(0)),
        ];
        let result = mark_noisy(&commands, &default_config());
        // cd failed → kept (not noisy). ls has meaningful after (cd), pwd has meaningful before (cd).
        // So all three are kept!
        assert_eq!(result, vec![false, false, false]);
    }
}
