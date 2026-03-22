use crate::db::{CommandRecord, Cluster, Database};
use rusqlite::Result;

/// Assign a command to a cluster at record time.
///
/// Returns `Some(cluster_id)` if the command was assigned to a cluster,
/// `None` if the command is noisy and was skipped.
pub fn assign_to_cluster(
    db: &Database,
    record: &CommandRecord,
    command_id: i64,
    gap_minutes: u64,
) -> Result<Option<i64>> {
    if record.noisy {
        return Ok(None);
    }

    let gap_seconds = (gap_minutes * 60) as i64;

    // Find the most recent open cluster for this session
    if let Some(open_cluster) = db.get_latest_open_cluster(&record.session_id)? {
        let cluster_id = open_cluster.id.unwrap();

        // Check if command belongs to this cluster
        if let Some(last_cmd) = db.get_last_meaningful_command_for_cluster(cluster_id)? {
            let within_time = (record.timestamp - last_cmd.timestamp) <= gap_seconds;
            let same_dir = record.directory == last_cmd.directory;

            if within_time && same_dir {
                let pos = db.get_next_position_for_cluster(cluster_id)?;
                db.add_command_to_cluster(cluster_id, command_id, pos)?;
                return Ok(Some(cluster_id));
            }
        }
    }

    // No matching open cluster — create a new one
    let cluster = Cluster {
        id: None,
        alias: None,
        created_at: record.timestamp,
        last_used: None,
        directory: Some(record.directory.clone()),
        notes: None,
    };
    let cluster_id = db.insert_cluster(&cluster)?;
    db.add_command_to_cluster(cluster_id, command_id, 0)?;
    Ok(Some(cluster_id))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn insert_cmd(db: &Database, cmd: &str, ts: i64, dir: &str, session: &str, noisy: bool) -> (i64, CommandRecord) {
        let record = CommandRecord {
            id: None,
            cmd: cmd.to_string(),
            timestamp: ts,
            directory: dir.to_string(),
            exit_code: Some(0),
            session_id: session.to_string(),
            shell: "zsh".to_string(),
            noisy,
        };
        let id = db.insert_command(&record).unwrap();
        (id, record)
    }

    #[test]
    fn test_first_command_creates_new_cluster() {
        let db = Database::open_in_memory().unwrap();
        let (cmd_id, record) = insert_cmd(&db, "cargo build", 1000, "/project", "s1", false);

        let result = assign_to_cluster(&db, &record, cmd_id, 15).unwrap();
        assert!(result.is_some());

        let cluster_id = result.unwrap();
        let cmds = db.get_commands_for_cluster(cluster_id).unwrap();
        assert_eq!(cmds.len(), 1);
        assert_eq!(cmds[0].cmd, "cargo build");
    }

    #[test]
    fn test_noisy_command_skipped() {
        let db = Database::open_in_memory().unwrap();
        let (cmd_id, record) = insert_cmd(&db, "ls", 1000, "/project", "s1", true);

        let result = assign_to_cluster(&db, &record, cmd_id, 15).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_command_joins_existing_cluster() {
        let db = Database::open_in_memory().unwrap();
        let (cmd1_id, record1) = insert_cmd(&db, "cargo build", 1000, "/project", "s1", false);
        let cluster_id = assign_to_cluster(&db, &record1, cmd1_id, 15).unwrap().unwrap();

        let (cmd2_id, record2) = insert_cmd(&db, "cargo test", 1060, "/project", "s1", false);
        let result = assign_to_cluster(&db, &record2, cmd2_id, 15).unwrap();
        assert_eq!(result, Some(cluster_id));

        let cmds = db.get_commands_for_cluster(cluster_id).unwrap();
        assert_eq!(cmds.len(), 2);
        assert_eq!(cmds[0].cmd, "cargo build");
        assert_eq!(cmds[1].cmd, "cargo test");
    }

    #[test]
    fn test_time_gap_creates_new_cluster() {
        let db = Database::open_in_memory().unwrap();
        let (cmd1_id, record1) = insert_cmd(&db, "cargo build", 1000, "/project", "s1", false);
        let cluster1 = assign_to_cluster(&db, &record1, cmd1_id, 15).unwrap().unwrap();

        // 20-minute gap (> 15 min threshold = 900 seconds)
        let (cmd2_id, record2) = insert_cmd(&db, "cargo test", 2200, "/project", "s1", false);
        let cluster2 = assign_to_cluster(&db, &record2, cmd2_id, 15).unwrap().unwrap();

        assert_ne!(cluster1, cluster2);
    }

    #[test]
    fn test_directory_change_creates_new_cluster() {
        let db = Database::open_in_memory().unwrap();
        let (cmd1_id, record1) = insert_cmd(&db, "cargo build", 1000, "/project-a", "s1", false);
        let cluster1 = assign_to_cluster(&db, &record1, cmd1_id, 15).unwrap().unwrap();

        let (cmd2_id, record2) = insert_cmd(&db, "npm install", 1060, "/project-b", "s1", false);
        let cluster2 = assign_to_cluster(&db, &record2, cmd2_id, 15).unwrap().unwrap();

        assert_ne!(cluster1, cluster2);
    }

    #[test]
    fn test_session_change_creates_new_cluster() {
        let db = Database::open_in_memory().unwrap();
        let (cmd1_id, record1) = insert_cmd(&db, "cargo build", 1000, "/project", "s1", false);
        let cluster1 = assign_to_cluster(&db, &record1, cmd1_id, 15).unwrap().unwrap();

        let (cmd2_id, record2) = insert_cmd(&db, "cargo test", 1060, "/project", "s2", false);
        let cluster2 = assign_to_cluster(&db, &record2, cmd2_id, 15).unwrap().unwrap();

        assert_ne!(cluster1, cluster2);
    }

    #[test]
    fn test_multiple_boundaries_correct_cluster_count() {
        let db = Database::open_in_memory().unwrap();

        // Cluster 1: two commands in /project-a
        let (id1, r1) = insert_cmd(&db, "cargo build", 1000, "/project-a", "s1", false);
        let c1 = assign_to_cluster(&db, &r1, id1, 15).unwrap().unwrap();
        let (id2, r2) = insert_cmd(&db, "cargo test", 1060, "/project-a", "s1", false);
        let c1b = assign_to_cluster(&db, &r2, id2, 15).unwrap().unwrap();
        assert_eq!(c1, c1b);

        // Cluster 2: directory change
        let (id3, r3) = insert_cmd(&db, "npm install", 1120, "/project-b", "s1", false);
        let c2 = assign_to_cluster(&db, &r3, id3, 15).unwrap().unwrap();
        assert_ne!(c1, c2);

        // Cluster 3: time gap (20 min)
        let (id4, r4) = insert_cmd(&db, "npm test", 2320, "/project-b", "s1", false);
        let c3 = assign_to_cluster(&db, &r4, id4, 15).unwrap().unwrap();
        assert_ne!(c2, c3);

        // Verify cluster 1 has 2 commands
        let cmds1 = db.get_commands_for_cluster(c1).unwrap();
        assert_eq!(cmds1.len(), 2);

        // Verify cluster 2 has 1 command
        let cmds2 = db.get_commands_for_cluster(c2).unwrap();
        assert_eq!(cmds2.len(), 1);
    }
}
