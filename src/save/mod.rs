use crate::db::{Cluster, CommandRecord, Database};
use rusqlite::Result;

pub struct SaveSummary {
    pub alias: String,
    pub command_count: usize,
    pub tags: Vec<String>,
}

pub enum CollisionResolution {
    SaveUnderNewName(String),
    RenameExisting(String),
    DeleteExisting,
    Cancel,
}

pub fn save_cluster(cluster_id: i64, alias: &str, db: &Database) -> Result<SaveSummary> {
    db.update_cluster_alias(cluster_id, alias)?;
    let commands = db.get_commands_for_cluster(cluster_id)?;
    let tags = db.get_tags_for_cluster(cluster_id)?;
    Ok(SaveSummary {
        alias: alias.to_string(),
        command_count: commands.len(),
        tags,
    })
}

pub fn run(alias: &str, db: &Database) -> Result<()> {
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_open_cluster(db: &Database) -> i64 {
        let cluster_id = db.insert_cluster(&Cluster {
            id: None, alias: None, created_at: 1000, last_used: None,
            directory: Some("/project".to_string()), notes: None,
        }).unwrap();
        let cmd1_id = db.insert_command(&CommandRecord {
            id: None, cmd: "helm list".to_string(), timestamp: 1000,
            directory: "/project".to_string(), exit_code: Some(0),
            session_id: "s1".to_string(), shell: "zsh".to_string(), noisy: false,
        }).unwrap();
        let cmd2_id = db.insert_command(&CommandRecord {
            id: None, cmd: "kubectl get pods".to_string(), timestamp: 1060,
            directory: "/project".to_string(), exit_code: Some(0),
            session_id: "s1".to_string(), shell: "zsh".to_string(), noisy: false,
        }).unwrap();
        db.add_command_to_cluster(cluster_id, cmd1_id, 0).unwrap();
        db.add_command_to_cluster(cluster_id, cmd2_id, 1).unwrap();
        db.add_tag_to_cluster(cluster_id, "kubernetes").unwrap();
        db.add_tag_to_cluster(cluster_id, "helm").unwrap();
        cluster_id
    }

    #[test]
    fn test_save_cluster_assigns_alias_and_returns_summary() {
        let db = Database::open_in_memory().unwrap();
        let cluster_id = setup_open_cluster(&db);

        let summary = save_cluster(cluster_id, "helm-debug", &db).unwrap();

        assert_eq!(summary.alias, "helm-debug");
        assert_eq!(summary.command_count, 2);
        assert!(summary.tags.contains(&"kubernetes".to_string()));
        assert!(summary.tags.contains(&"helm".to_string()));
        assert!(db.get_cluster_by_alias("helm-debug").unwrap().is_some());
    }

    #[test]
    fn test_save_cluster_no_tags_returns_empty_tags() {
        let db = Database::open_in_memory().unwrap();
        let cluster_id = db.insert_cluster(&Cluster {
            id: None, alias: None, created_at: 1000, last_used: None,
            directory: Some("/project".to_string()), notes: None,
        }).unwrap();
        let cmd_id = db.insert_command(&CommandRecord {
            id: None, cmd: "echo hello".to_string(), timestamp: 1000,
            directory: "/project".to_string(), exit_code: Some(0),
            session_id: "s1".to_string(), shell: "zsh".to_string(), noisy: false,
        }).unwrap();
        db.add_command_to_cluster(cluster_id, cmd_id, 0).unwrap();

        let summary = save_cluster(cluster_id, "echo-test", &db).unwrap();

        assert_eq!(summary.command_count, 1);
        assert!(summary.tags.is_empty());
    }

    #[test]
    fn test_delete_and_save_replaces_existing_cluster() {
        let db = Database::open_in_memory().unwrap();

        // Old saved cluster
        let old_id = db.insert_cluster(&Cluster {
            id: None, alias: Some("my-flow".to_string()), created_at: 500, last_used: None,
            directory: Some("/old".to_string()), notes: None,
        }).unwrap();
        let old_cmd = db.insert_command(&CommandRecord {
            id: None, cmd: "old cmd".to_string(), timestamp: 500,
            directory: "/old".to_string(), exit_code: Some(0),
            session_id: "s0".to_string(), shell: "zsh".to_string(), noisy: false,
        }).unwrap();
        db.add_command_to_cluster(old_id, old_cmd, 0).unwrap();

        // New open cluster
        let new_id = setup_open_cluster(&db);

        // Simulate option 3: delete old, save new
        db.delete_cluster(old_id).unwrap();
        let summary = save_cluster(new_id, "my-flow", &db).unwrap();

        assert_eq!(summary.alias, "my-flow");
        assert_eq!(db.get_cluster_by_alias("my-flow").unwrap().unwrap().id, Some(new_id));
    }

    #[test]
    fn test_rename_existing_and_save_new() {
        let db = Database::open_in_memory().unwrap();

        // Old saved cluster
        let old_id = db.insert_cluster(&Cluster {
            id: None, alias: Some("my-flow".to_string()), created_at: 500, last_used: None,
            directory: Some("/old".to_string()), notes: None,
        }).unwrap();
        let old_cmd = db.insert_command(&CommandRecord {
            id: None, cmd: "old cmd".to_string(), timestamp: 500,
            directory: "/old".to_string(), exit_code: Some(0),
            session_id: "s0".to_string(), shell: "zsh".to_string(), noisy: false,
        }).unwrap();
        db.add_command_to_cluster(old_id, old_cmd, 0).unwrap();

        // New open cluster
        let new_id = setup_open_cluster(&db);

        // Simulate option 2: rename old, save new
        db.update_cluster_alias(old_id, "my-flow-old").unwrap();
        let summary = save_cluster(new_id, "my-flow", &db).unwrap();

        assert_eq!(summary.alias, "my-flow");
        assert!(db.get_cluster_by_alias("my-flow-old").unwrap().is_some());
        assert_eq!(db.get_cluster_by_alias("my-flow").unwrap().unwrap().id, Some(new_id));
    }
}
