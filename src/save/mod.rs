use crate::config::TagInferenceConfig;
use crate::db::{Cluster, CommandRecord, Database};
use crate::refine;
use dialoguer::{Confirm, Input, Select};
use anyhow::Result;

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

fn print_summary(summary: &SaveSummary) {
    let tag_str = if summary.tags.is_empty() {
        "no tags".to_string()
    } else {
        summary.tags.join(", ")
    };
    println!(
        "Saved \"{}\" — {} commands · {}",
        summary.alias, summary.command_count, tag_str
    );
}

fn prompt_collision_menu(alias: &str, existing: &Cluster, db: &Database) -> CollisionResolution {
    let existing_id = existing.id.expect("cluster from DB always has id");
    let existing_cmds = db.get_commands_for_cluster(existing_id).unwrap_or_default();
    let existing_tags = db.get_tags_for_cluster(existing_id).unwrap_or_default();
    let tag_str = if existing_tags.is_empty() {
        "no tags".to_string()
    } else {
        existing_tags.join(", ")
    };

    let prompt = format!(
        "Alias \"{}\" is already in use ({} commands · {})",
        alias,
        existing_cmds.len(),
        tag_str
    );

    let items = &[
        "Save under a different name",
        "Rename the existing cluster, then save",
        "Delete the existing cluster and save",
        "Cancel",
    ];

    let selection = Select::new()
        .with_prompt(&prompt)
        .items(items)
        .default(0)
        .interact()
        .unwrap_or(3); // 3 = Cancel (last item)

    match selection {
        0 => {
            let new_alias: String = Input::new()
                .with_prompt("New alias")
                .interact_text()
                .unwrap_or_default();
            CollisionResolution::SaveUnderNewName(new_alias)
        }
        1 => {
            let new_name: String = Input::new()
                .with_prompt(format!("New name for \"{}\"", alias))
                .interact_text()
                .unwrap_or_default();
            CollisionResolution::RenameExisting(new_name)
        }
        2 => CollisionResolution::DeleteExisting,
        _ => CollisionResolution::Cancel,
    }
}

pub fn run(alias: &str, refine_flag: bool, db: &Database, config: &TagInferenceConfig) -> Result<()> {
    let new_cluster = match db.get_most_recent_open_cluster()? {
        Some(c) => c,
        None => {
            eprintln!(
                "No unsaved commands found. Run some commands first, then `lmc save <alias>`."
            );
            return Ok(());
        }
    };
    let cluster_id = new_cluster.id.expect("cluster from DB always has id");

    // Fetch non-noisy commands for this cluster
    let all_commands = db.get_commands_for_cluster(cluster_id)?;
    let commands: Vec<CommandRecord> = all_commands.into_iter().filter(|c| !c.noisy).collect();

    if commands.is_empty() {
        eprintln!("No meaningful commands in the most recent cluster.");
        return Ok(());
    }

    // Refinement — auto-triggered or explicit
    if refine::should_refine(&commands, refine_flag) {
        match refine::run(alias, commands, config.clone())? {
            refine::RefineResult::Confirmed(refined) => {
                db.replace_cluster_commands(cluster_id, &refined)?;
            }
            refine::RefineResult::Split(top, bottom) => {
                db.replace_cluster_commands(cluster_id, &top)?;

                // Create a new cluster for the bottom half
                let bottom_cluster_id = db.insert_cluster(&Cluster {
                    id: None,
                    alias: None,
                    created_at: bottom.first().map(|c| c.timestamp).unwrap_or(0),
                    last_used: None,
                    directory: bottom.first().map(|c| c.directory.clone()),
                    notes: None,
                })?;
                for (pos, cmd) in bottom.iter().enumerate() {
                    let cmd_id = cmd.id.expect("command must have an id");
                    db.add_command_to_cluster(bottom_cluster_id, cmd_id, pos as i32)?;
                }

                // Prompt user to optionally name the bottom half
                let bottom_alias: String = Input::new()
                    .with_prompt("Name for the second half (press Enter to skip)")
                    .allow_empty(true)
                    .interact_text()
                    .unwrap_or_default();
                if !bottom_alias.is_empty() {
                    db.update_cluster_alias(bottom_cluster_id, &bottom_alias)?;
                    println!("Saved \"{}\" — {} commands", bottom_alias, bottom.len());
                }
                // Top half falls through to alias assignment below
            }
            refine::RefineResult::Cancelled => {
                println!("Save cancelled.");
                return Ok(());
            }
        }
    }

    // Alias collision loop (unchanged logic)
    let mut current_alias = alias.to_string();
    loop {
        match db.get_cluster_by_alias(&current_alias)? {
            None => {
                let summary = save_cluster(cluster_id, &current_alias, db)?;
                print_summary(&summary);
                return Ok(());
            }
            Some(existing) => {
                let resolution = prompt_collision_menu(&current_alias, &existing, db);
                match resolution {
                    CollisionResolution::SaveUnderNewName(new_alias) => {
                        if new_alias.is_empty() {
                            continue;
                        }
                        current_alias = new_alias;
                    }
                    CollisionResolution::RenameExisting(new_name) => {
                        if new_name.is_empty() {
                            continue;
                        }
                        let existing_id = existing.id.expect("cluster from DB always has id");
                        db.update_cluster_alias(existing_id, &new_name)?;
                        let summary = save_cluster(cluster_id, &current_alias, db)?;
                        println!(
                            "Existing cluster renamed: \"{}\" → \"{}\"",
                            current_alias, new_name
                        );
                        print_summary(&summary);
                        return Ok(());
                    }
                    CollisionResolution::DeleteExisting => {
                        let existing_id = existing.id.expect("cluster from DB always has id");
                        let confirmed = Confirm::new()
                            .with_prompt(format!(
                                "This will permanently delete \"{}\". Continue?",
                                current_alias
                            ))
                            .default(false)
                            .interact()
                            .unwrap_or(false);
                        if confirmed {
                            db.delete_cluster(existing_id)?;
                            let summary = save_cluster(cluster_id, &current_alias, db)?;
                            print_summary(&summary);
                            return Ok(());
                        }
                    }
                    CollisionResolution::Cancel => {
                        return Ok(());
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::TagInferenceConfig;
    use crate::db::CommandRecord;

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

    #[test]
    fn test_run_with_no_open_cluster_returns_ok() {
        // run() should return Ok(()) immediately when there are no open clusters
        // (the eprintln message is verified manually; we just ensure no panic/error)
        let db = Database::open_in_memory().unwrap();
        // Insert a saved (aliased) cluster — should be ignored
        let saved_id = db.insert_cluster(&Cluster {
            id: None, alias: Some("already-saved".to_string()), created_at: 1000,
            last_used: None, directory: Some("/p".to_string()), notes: None,
        }).unwrap();
        let cmd_id = db.insert_command(&CommandRecord {
            id: None, cmd: "cargo build".to_string(), timestamp: 1000,
            directory: "/p".to_string(), exit_code: Some(0),
            session_id: "s1".to_string(), shell: "zsh".to_string(), noisy: false,
        }).unwrap();
        db.add_command_to_cluster(saved_id, cmd_id, 0).unwrap();

        // No open (unaliased) clusters — run should return Ok immediately
        let result = run("new-alias", false, &db, &TagInferenceConfig::default());
        assert!(result.is_ok());
        // Alias was NOT saved (no open cluster to save)
        assert!(db.get_cluster_by_alias("new-alias").unwrap().is_none());
    }
}
