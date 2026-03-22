use crate::config::TagInferenceConfig;

/// Built-in CLI tool to tags mapping, from the design document.
const BUILTIN_MAPPINGS: &[(&[&str], &[&str])] = &[
    (&["helm"], &["kubernetes", "helm"]),
    (&["kubectl"], &["kubernetes"]),
    (&["docker", "docker-compose"], &["docker"]),
    (&["git", "gh"], &["git", "github"]),
    (&["psql", "pg_dump", "pg_restore"], &["postgres"]),
    (&["aws"], &["aws"]),
    (&["terraform", "tofu"], &["terraform", "infra"]),
    (&["ansible", "ansible-playbook"], &["ansible", "infra"]),
    (&["cargo", "rustc"], &["rust"]),
    (&["npm", "yarn", "pnpm"], &["node"]),
];

/// Infer tags for a single command based on its first token.
pub fn infer_tags_for_command(cmd: &str, config: &TagInferenceConfig) -> Vec<String> {
    let trimmed = cmd.trim();
    if trimmed.is_empty() {
        return vec![];
    }

    let first_token = trimmed.split_whitespace().next().unwrap_or("");
    let base_command = first_token.rsplit('/').next().unwrap_or(first_token);

    let mut tags: Vec<String> = Vec::new();

    for (tools, inferred_tags) in BUILTIN_MAPPINGS {
        if tools.contains(&base_command) {
            tags.extend(inferred_tags.iter().map(|t| t.to_string()));
        }
    }

    for mapping in &config.custom {
        if mapping.tools.iter().any(|t| t == base_command) {
            tags.extend(mapping.tags.iter().cloned());
        }
    }

    tags.sort();
    tags.dedup();
    tags
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::TagInferenceMapping;

    fn empty_config() -> TagInferenceConfig {
        TagInferenceConfig { custom: vec![] }
    }

    #[test]
    fn test_kubectl_infers_kubernetes() {
        let tags = infer_tags_for_command("kubectl get pods -n production", &empty_config());
        assert!(tags.contains(&"kubernetes".to_string()));
    }

    #[test]
    fn test_helm_infers_kubernetes_and_helm() {
        let tags = infer_tags_for_command("helm list -n production", &empty_config());
        assert!(tags.contains(&"kubernetes".to_string()));
        assert!(tags.contains(&"helm".to_string()));
    }

    #[test]
    fn test_docker_infers_docker() {
        let tags = infer_tags_for_command("docker ps", &empty_config());
        assert!(tags.contains(&"docker".to_string()));
    }

    #[test]
    fn test_git_infers_git_and_github() {
        let tags = infer_tags_for_command("git status", &empty_config());
        assert!(tags.contains(&"git".to_string()));
        assert!(tags.contains(&"github".to_string()));
    }

    #[test]
    fn test_cargo_infers_rust() {
        let tags = infer_tags_for_command("cargo build", &empty_config());
        assert!(tags.contains(&"rust".to_string()));
    }

    #[test]
    fn test_unknown_command_returns_empty() {
        let tags = infer_tags_for_command("some-unknown-tool --flag", &empty_config());
        assert!(tags.is_empty());
    }

    #[test]
    fn test_empty_command_returns_empty() {
        let tags = infer_tags_for_command("", &empty_config());
        assert!(tags.is_empty());
    }

    #[test]
    fn test_whitespace_command_returns_empty() {
        let tags = infer_tags_for_command("   ", &empty_config());
        assert!(tags.is_empty());
    }

    #[test]
    fn test_path_prefixed_command() {
        let tags = infer_tags_for_command("/usr/local/bin/docker ps", &empty_config());
        assert!(tags.contains(&"docker".to_string()));
    }

    #[test]
    fn test_tags_are_sorted() {
        let tags = infer_tags_for_command("helm install my-release", &empty_config());
        let mut sorted = tags.clone();
        sorted.sort();
        assert_eq!(tags, sorted);
    }
}
