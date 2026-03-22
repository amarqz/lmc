use crate::config::NoiseFilterConfig;
use crate::db::{CommandRecord, Database};

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

    // noisy = not kept
    kept.iter().map(|&k| !k).collect()
}

/// Re-evaluate noisy flags for all commands in a session using full context.
pub fn remark_session(
    db: &Database,
    session_id: &str,
    config: &NoiseFilterConfig,
) -> rusqlite::Result<()> {
    let commands = db.get_session_commands(session_id)?;
    let noisy_flags = mark_noisy(&commands, config);

    for (cmd, &noisy) in commands.iter().zip(noisy_flags.iter()) {
        if let Some(id) = cmd.id
            && cmd.noisy != noisy
        {
            db.update_noisy_flag(id, noisy)?;
        }
    }
    Ok(())
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
    fn test_remark_session() {
        use crate::db::Database;

        let db = Database::open_in_memory().unwrap();
        let config = default_config();

        // Insert a session: meaningful, noisy, meaningful
        let mut cmd1 = make_cmd("cargo build", Some(0));
        cmd1.session_id = "remark-test".to_string();
        let mut cmd2 = make_cmd("ls", Some(0));
        cmd2.session_id = "remark-test".to_string();
        let mut cmd3 = make_cmd("cargo test", Some(0));
        cmd3.session_id = "remark-test".to_string();

        db.insert_command(&cmd1).unwrap();
        db.insert_command(&cmd2).unwrap();
        db.insert_command(&cmd3).unwrap();

        // Before remark, ls is not marked noisy (default false)
        let cmds = db.get_session_commands("remark-test").unwrap();
        assert!(!cmds[1].noisy);

        // Remark — ls is sandwiched, should stay not noisy
        remark_session(&db, "remark-test", &config).unwrap();
        let cmds = db.get_session_commands("remark-test").unwrap();
        assert!(!cmds[1].noisy);

        // Now test with isolated noisy: just one ls
        let mut lonely = make_cmd("ls", Some(0));
        lonely.session_id = "lonely-session".to_string();
        db.insert_command(&lonely).unwrap();

        remark_session(&db, "lonely-session", &config).unwrap();
        let cmds = db.get_session_commands("lonely-session").unwrap();
        assert!(cmds[0].noisy);
    }

    #[test]
    fn test_mark_failed_noisy_among_all_noisy() {
        let commands = vec![
            make_cmd("ls", Some(0)),
            make_cmd("cd /bad", Some(1)),
            make_cmd("pwd", Some(0)),
        ];
        let result = mark_noisy(&commands, &default_config());
        // cd failed → kept. But strict sandwich requires meaningful on BOTH sides:
        // ls has meaningful after but not before → noisy
        // pwd has meaningful before but not after → noisy
        assert_eq!(result, vec![true, false, true]);
    }
}
