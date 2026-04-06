use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct Config {
    #[serde(default = "default_general")]
    pub general: GeneralConfig,
    #[serde(default)]
    pub noise_filter: NoiseFilterConfig,
    #[serde(default)]
    pub tag_inference: TagInferenceConfig,
    #[serde(default)]
    pub ui: UiConfig,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct GeneralConfig {
    #[serde(default = "default_cluster_gap")]
    pub cluster_gap_minutes: u64,
    #[serde(default)]
    pub db_path: String,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct NoiseFilterConfig {
    #[serde(default = "default_ignored_commands")]
    pub ignored_commands: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TagInferenceMapping {
    pub tools: Vec<String>,
    pub tags: Vec<String>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq)]
pub struct TagInferenceConfig {
    #[serde(default)]
    pub custom: Vec<TagInferenceMapping>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct UiConfig {
    #[serde(default = "default_action")]
    pub default_action: String,
}

fn default_general() -> GeneralConfig {
    GeneralConfig {
        cluster_gap_minutes: 15,
        db_path: String::new(),
    }
}

fn default_cluster_gap() -> u64 {
    15
}

fn default_ignored_commands() -> Vec<String> {
    vec![
        "cd", "ls", "ll", "la", "pwd", "clear", "reset", "history",
    ]
    .into_iter()
    .map(String::from)
    .collect()
}

fn default_action() -> String {
    "copy".to_string()
}

impl Default for GeneralConfig {
    fn default() -> Self {
        default_general()
    }
}

impl Default for NoiseFilterConfig {
    fn default() -> Self {
        NoiseFilterConfig {
            ignored_commands: default_ignored_commands(),
        }
    }
}

impl Default for UiConfig {
    fn default() -> Self {
        UiConfig {
            default_action: default_action(),
        }
    }
}

fn project_dirs() -> Option<ProjectDirs> {
    ProjectDirs::from("", "", "lmc")
}

pub fn config_path() -> Option<PathBuf> {
    project_dirs().map(|dirs| dirs.config_dir().join("config.toml"))
}

pub fn default_db_path() -> Option<PathBuf> {
    project_dirs().map(|dirs| dirs.data_dir().join("lmc.db"))
}

/// Resolve the database path: LMC_DB_PATH env > config db_path > platform default.
pub fn resolve_db_path(config: &Config) -> PathBuf {
    if let Ok(env_path) = env::var("LMC_DB_PATH")
        && !env_path.is_empty()
    {
        return PathBuf::from(env_path);
    }
    if !config.general.db_path.is_empty() {
        return PathBuf::from(&config.general.db_path);
    }
    default_db_path().expect("Could not determine data directory for lmc")
}

/// Load config from disk, creating a default file if none exists.
pub fn load_config() -> Result<Config, Box<dyn std::error::Error>> {
    let path = match config_path() {
        Some(p) => p,
        None => return Ok(Config::default()),
    };

    if !path.exists() {
        let config = Config::default();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let toml_str = toml::to_string_pretty(&config)?;
        fs::write(&path, toml_str)?;
        return Ok(config);
    }

    let contents = fs::read_to_string(&path)?;
    let config: Config = toml::from_str(&contents)?;
    Ok(config)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert_eq!(config.general.cluster_gap_minutes, 15);
        assert_eq!(config.general.db_path, "");
        assert!(config.noise_filter.ignored_commands.contains(&"cd".to_string()));
        assert!(config.noise_filter.ignored_commands.contains(&"ls".to_string()));
        assert_eq!(config.noise_filter.ignored_commands.len(), 8);
        assert!(config.tag_inference.custom.is_empty());
        assert_eq!(config.ui.default_action, "copy");
    }

    #[test]
    fn test_deserialize_full_config() {
        let toml_str = r#"
[general]
cluster_gap_minutes = 30
db_path = "/tmp/test.db"

[noise_filter]
ignored_commands = ["cd", "ls"]

[[tag_inference.custom]]
tools = ["myctl"]
tags = ["myproject"]

[ui]
default_action = "print"
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.general.cluster_gap_minutes, 30);
        assert_eq!(config.general.db_path, "/tmp/test.db");
        assert_eq!(config.noise_filter.ignored_commands, vec!["cd", "ls"]);
        assert_eq!(config.tag_inference.custom.len(), 1);
        assert_eq!(config.tag_inference.custom[0].tools, vec!["myctl"]);
        assert_eq!(config.tag_inference.custom[0].tags, vec!["myproject"]);
        assert_eq!(config.ui.default_action, "print");
    }

    #[test]
    fn test_deserialize_partial_config_uses_defaults() {
        let toml_str = r#"
[general]
cluster_gap_minutes = 10
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.general.cluster_gap_minutes, 10);
        assert_eq!(config.general.db_path, "");
        // Sections not present get their defaults
        assert_eq!(config.noise_filter.ignored_commands, default_ignored_commands());
        assert_eq!(config.ui.default_action, "copy");
    }

    #[test]
    fn test_deserialize_empty_config_uses_defaults() {
        let config: Config = toml::from_str("").unwrap();
        assert_eq!(config, Config::default());
    }

    #[test]
    fn test_roundtrip_serialize_deserialize() {
        let config = Config::default();
        let serialized = toml::to_string_pretty(&config).unwrap();
        let deserialized: Config = toml::from_str(&serialized).unwrap();
        assert_eq!(config, deserialized);
    }

    #[test]
    fn test_resolve_db_path_env_override() {
        let config = Config::default();
        unsafe { env::set_var("LMC_DB_PATH", "/tmp/env-override.db") };
        let path = resolve_db_path(&config);
        unsafe { env::remove_var("LMC_DB_PATH") };
        assert_eq!(path, PathBuf::from("/tmp/env-override.db"));
    }

    #[test]
    fn test_resolve_db_path_config_override() {
        unsafe { env::remove_var("LMC_DB_PATH") };
        let mut config = Config::default();
        config.general.db_path = "/tmp/config-override.db".to_string();
        let path = resolve_db_path(&config);
        assert_eq!(path, PathBuf::from("/tmp/config-override.db"));
    }

    #[test]
    fn test_resolve_db_path_platform_default() {
        unsafe { env::remove_var("LMC_DB_PATH") };
        let config = Config::default();
        let path = resolve_db_path(&config);
        // Should end with lmc.db in a platform-appropriate directory
        assert!(path.ends_with("lmc.db"));
    }

    #[test]
    fn test_load_config_from_file() {
        let mut tmpfile = NamedTempFile::new().unwrap();
        write!(
            tmpfile,
            r#"
[general]
cluster_gap_minutes = 20
db_path = ""

[noise_filter]
ignored_commands = ["cd"]

[ui]
default_action = "run"
"#
        )
        .unwrap();

        let contents = fs::read_to_string(tmpfile.path()).unwrap();
        let config: Config = toml::from_str(&contents).unwrap();
        assert_eq!(config.general.cluster_gap_minutes, 20);
        assert_eq!(config.noise_filter.ignored_commands, vec!["cd"]);
        assert_eq!(config.ui.default_action, "run");
    }
}
