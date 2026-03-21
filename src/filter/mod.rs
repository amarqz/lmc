use crate::config::NoiseFilterConfig;

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
}
