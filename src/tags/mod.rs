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

    #[test]
    fn test_custom_mapping_applies() {
        let config = TagInferenceConfig {
            custom: vec![TagInferenceMapping {
                tools: vec!["myctl".to_string()],
                tags: vec!["myproject".to_string()],
            }],
        };
        let tags = infer_tags_for_command("myctl deploy --env prod", &config);
        assert_eq!(tags, vec!["myproject"]);
    }

    #[test]
    fn test_custom_mapping_merges_with_builtin() {
        let config = TagInferenceConfig {
            custom: vec![TagInferenceMapping {
                tools: vec!["kubectl".to_string()],
                tags: vec!["production".to_string()],
            }],
        };
        let tags = infer_tags_for_command("kubectl get pods", &config);
        assert!(tags.contains(&"kubernetes".to_string()));
        assert!(tags.contains(&"production".to_string()));
    }

    #[test]
    fn test_custom_mapping_multiple_tools() {
        let config = TagInferenceConfig {
            custom: vec![TagInferenceMapping {
                tools: vec!["deploy-cli".to_string(), "rollout".to_string()],
                tags: vec!["deployment".to_string()],
            }],
        };
        let tags1 = infer_tags_for_command("deploy-cli push", &config);
        let tags2 = infer_tags_for_command("rollout status", &config);
        assert_eq!(tags1, vec!["deployment"]);
        assert_eq!(tags2, vec!["deployment"]);
    }

    #[test]
    fn test_duplicate_tags_deduplicated() {
        let config = TagInferenceConfig {
            custom: vec![TagInferenceMapping {
                tools: vec!["kubectl".to_string()],
                tags: vec!["kubernetes".to_string()],
            }],
        };
        let tags = infer_tags_for_command("kubectl apply -f .", &config);
        let k8s_count = tags.iter().filter(|t| *t == "kubernetes").count();
        assert_eq!(k8s_count, 1);
    }

    #[test]
    fn test_all_builtin_mappings() {
        let cfg = empty_config();

        // helm, kubectl → kubernetes
        assert!(infer_tags_for_command("helm install", &cfg).contains(&"kubernetes".to_string()));
        assert!(infer_tags_for_command("kubectl apply", &cfg).contains(&"kubernetes".to_string()));

        // docker, docker-compose → docker
        assert!(infer_tags_for_command("docker build .", &cfg).contains(&"docker".to_string()));
        assert!(infer_tags_for_command("docker-compose up", &cfg).contains(&"docker".to_string()));

        // git, gh → git, github
        assert!(infer_tags_for_command("git push", &cfg).contains(&"git".to_string()));
        assert!(infer_tags_for_command("git push", &cfg).contains(&"github".to_string()));
        assert!(infer_tags_for_command("gh pr create", &cfg).contains(&"git".to_string()));
        assert!(infer_tags_for_command("gh pr create", &cfg).contains(&"github".to_string()));

        // psql, pg_dump, pg_restore → postgres
        assert!(infer_tags_for_command("psql -U admin", &cfg).contains(&"postgres".to_string()));
        assert!(infer_tags_for_command("pg_dump mydb", &cfg).contains(&"postgres".to_string()));
        assert!(infer_tags_for_command("pg_restore dump.sql", &cfg).contains(&"postgres".to_string()));

        // aws → aws
        assert!(infer_tags_for_command("aws s3 ls", &cfg).contains(&"aws".to_string()));

        // terraform, tofu → terraform, infra
        let tf_tags = infer_tags_for_command("terraform plan", &cfg);
        assert!(tf_tags.contains(&"terraform".to_string()));
        assert!(tf_tags.contains(&"infra".to_string()));
        let tofu_tags = infer_tags_for_command("tofu apply", &cfg);
        assert!(tofu_tags.contains(&"terraform".to_string()));
        assert!(tofu_tags.contains(&"infra".to_string()));

        // ansible, ansible-playbook → ansible, infra
        assert!(infer_tags_for_command("ansible all -m ping", &cfg).contains(&"ansible".to_string()));
        assert!(infer_tags_for_command("ansible all -m ping", &cfg).contains(&"infra".to_string()));
        let ap_tags = infer_tags_for_command("ansible-playbook site.yml", &cfg);
        assert!(ap_tags.contains(&"ansible".to_string()));
        assert!(ap_tags.contains(&"infra".to_string()));

        // cargo, rustc → rust
        assert!(infer_tags_for_command("cargo test", &cfg).contains(&"rust".to_string()));
        assert!(infer_tags_for_command("rustc main.rs", &cfg).contains(&"rust".to_string()));

        // npm, yarn, pnpm → node
        assert!(infer_tags_for_command("npm install", &cfg).contains(&"node".to_string()));
        assert!(infer_tags_for_command("yarn add react", &cfg).contains(&"node".to_string()));
        assert!(infer_tags_for_command("pnpm run build", &cfg).contains(&"node".to_string()));
    }
}
