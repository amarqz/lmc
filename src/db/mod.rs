use rusqlite::{params, Connection, Result};
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, PartialEq)]
pub struct CommandRecord {
    pub id: Option<i64>,
    pub cmd: String,
    pub timestamp: i64,
    pub directory: String,
    pub exit_code: Option<i32>,
    pub session_id: String,
    pub shell: String,
    pub noisy: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Cluster {
    pub id: Option<i64>,
    pub alias: Option<String>,
    pub created_at: i64,
    pub last_used: Option<i64>,
    pub directory: Option<String>,
    pub notes: Option<String>,
}

pub struct Database {
    conn: Connection,
}

impl Database {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).ok();
        }
        let conn = Connection::open(path)?;
        let db = Database { conn };
        db.initialize()?;
        Ok(db)
    }

    #[allow(dead_code)]
    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        let db = Database { conn };
        db.initialize()?;
        Ok(db)
    }

    fn initialize(&self) -> Result<()> {
        self.conn.execute_batch("PRAGMA journal_mode=WAL;")?;
        self.conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS commands (
                id          INTEGER PRIMARY KEY,
                cmd         TEXT NOT NULL,
                timestamp   INTEGER NOT NULL,
                directory   TEXT NOT NULL,
                exit_code   INTEGER,
                session_id  TEXT NOT NULL,
                shell       TEXT NOT NULL,
                noisy       INTEGER NOT NULL DEFAULT 0
            );

            CREATE TABLE IF NOT EXISTS clusters (
                id          INTEGER PRIMARY KEY,
                alias       TEXT UNIQUE,
                created_at  INTEGER NOT NULL,
                last_used   INTEGER,
                directory   TEXT,
                notes       TEXT
            );

            CREATE TABLE IF NOT EXISTS cluster_commands (
                cluster_id  INTEGER REFERENCES clusters(id),
                command_id  INTEGER REFERENCES commands(id),
                position    INTEGER NOT NULL,
                PRIMARY KEY (cluster_id, command_id)
            );

            CREATE TABLE IF NOT EXISTS cluster_tags (
                cluster_id  INTEGER REFERENCES clusters(id),
                tag         TEXT NOT NULL,
                PRIMARY KEY (cluster_id, tag)
            );

            CREATE INDEX IF NOT EXISTS idx_commands_timestamp ON commands(timestamp);
            CREATE INDEX IF NOT EXISTS idx_commands_directory ON commands(directory);
            CREATE INDEX IF NOT EXISTS idx_commands_session_id ON commands(session_id);
            CREATE INDEX IF NOT EXISTS idx_clusters_alias ON clusters(alias);
            ",
        )?;
        // Migration: add noisy column to existing databases
        let _ = self.conn.execute_batch(
            "ALTER TABLE commands ADD COLUMN noisy INTEGER NOT NULL DEFAULT 0;"
        );
        Ok(())
    }

    pub fn insert_command(&self, cmd: &CommandRecord) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO commands (cmd, timestamp, directory, exit_code, session_id, shell, noisy)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                cmd.cmd,
                cmd.timestamp,
                cmd.directory,
                cmd.exit_code,
                cmd.session_id,
                cmd.shell,
                cmd.noisy as i32,
            ],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    #[allow(dead_code)]
    pub fn get_recent_commands(&self, limit: i64) -> Result<Vec<CommandRecord>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, cmd, timestamp, directory, exit_code, session_id, shell, noisy
             FROM commands ORDER BY timestamp DESC LIMIT ?1",
        )?;
        let rows = stmt.query_map(params![limit], |row| {
            let noisy_int: i32 = row.get(7)?;
            Ok(CommandRecord {
                id: Some(row.get(0)?),
                cmd: row.get(1)?,
                timestamp: row.get(2)?,
                directory: row.get(3)?,
                exit_code: row.get(4)?,
                session_id: row.get(5)?,
                shell: row.get(6)?,
                noisy: noisy_int != 0,
            })
        })?;
        rows.collect()
    }

    pub fn insert_cluster(&self, cluster: &Cluster) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO clusters (alias, created_at, last_used, directory, notes)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                cluster.alias,
                cluster.created_at,
                cluster.last_used,
                cluster.directory,
                cluster.notes,
            ],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn get_cluster_by_alias(&self, alias: &str) -> Result<Option<Cluster>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, alias, created_at, last_used, directory, notes
             FROM clusters WHERE alias = ?1",
        )?;
        let mut rows = stmt.query_map(params![alias], |row| {
            Ok(Cluster {
                id: Some(row.get(0)?),
                alias: row.get(1)?,
                created_at: row.get(2)?,
                last_used: row.get(3)?,
                directory: row.get(4)?,
                notes: row.get(5)?,
            })
        })?;
        match rows.next() {
            Some(row) => Ok(Some(row?)),
            None => Ok(None),
        }
    }

    pub fn get_all_clusters(&self) -> Result<Vec<Cluster>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, alias, created_at, last_used, directory, notes
             FROM clusters ORDER BY last_used DESC NULLS LAST, created_at DESC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(Cluster {
                id: Some(row.get(0)?),
                alias: row.get(1)?,
                created_at: row.get(2)?,
                last_used: row.get(3)?,
                directory: row.get(4)?,
                notes: row.get(5)?,
            })
        })?;
        rows.collect()
    }

    pub fn get_clusters_by_tags(&self, tags: &[String], require_all: bool) -> Result<Vec<Cluster>> {
        if tags.is_empty() {
            return self.get_all_clusters();
        }
        let placeholders = (0..tags.len()).map(|_| "?").collect::<Vec<_>>().join(", ");
        let sql = if require_all {
            format!(
                "SELECT c.id, c.alias, c.created_at, c.last_used, c.directory, c.notes
                 FROM clusters c
                 JOIN cluster_tags ct ON c.id = ct.cluster_id
                 WHERE ct.tag IN ({})
                 GROUP BY c.id
                 HAVING COUNT(DISTINCT ct.tag) = {}
                 ORDER BY c.last_used DESC NULLS LAST, c.created_at DESC",
                placeholders,
                tags.len()
            )
        } else {
            format!(
                "SELECT DISTINCT c.id, c.alias, c.created_at, c.last_used, c.directory, c.notes
                 FROM clusters c
                 JOIN cluster_tags ct ON c.id = ct.cluster_id
                 WHERE ct.tag IN ({})
                 ORDER BY c.last_used DESC NULLS LAST, c.created_at DESC",
                placeholders
            )
        };
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(rusqlite::params_from_iter(tags.iter()), |row| {
            Ok(Cluster {
                id: Some(row.get(0)?),
                alias: row.get(1)?,
                created_at: row.get(2)?,
                last_used: row.get(3)?,
                directory: row.get(4)?,
                notes: row.get(5)?,
            })
        })?;
        rows.collect()
    }

    pub fn add_command_to_cluster(
        &self,
        cluster_id: i64,
        command_id: i64,
        position: i32,
    ) -> Result<()> {
        self.conn.execute(
            "INSERT INTO cluster_commands (cluster_id, command_id, position)
             VALUES (?1, ?2, ?3)",
            params![cluster_id, command_id, position],
        )?;
        Ok(())
    }

    pub fn replace_cluster_commands(&self, cluster_id: i64, commands: &[CommandRecord]) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "DELETE FROM cluster_commands WHERE cluster_id = ?1",
            params![cluster_id],
        )?;
        for (pos, cmd) in commands.iter().enumerate() {
            let cmd_id = cmd.id.expect("command must have an id to be re-linked");
            tx.execute(
                "INSERT INTO cluster_commands (cluster_id, command_id, position) VALUES (?1, ?2, ?3)",
                params![cluster_id, cmd_id, pos as i32],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn get_commands_for_cluster(&self, cluster_id: i64) -> Result<Vec<CommandRecord>> {
        let mut stmt = self.conn.prepare(
            "SELECT c.id, c.cmd, c.timestamp, c.directory, c.exit_code, c.session_id, c.shell, c.noisy
             FROM commands c
             JOIN cluster_commands cc ON c.id = cc.command_id
             WHERE cc.cluster_id = ?1
             ORDER BY cc.position ASC",
        )?;
        let rows = stmt.query_map(params![cluster_id], |row| {
            let noisy_int: i32 = row.get(7)?;
            Ok(CommandRecord {
                id: Some(row.get(0)?),
                cmd: row.get(1)?,
                timestamp: row.get(2)?,
                directory: row.get(3)?,
                exit_code: row.get(4)?,
                session_id: row.get(5)?,
                shell: row.get(6)?,
                noisy: noisy_int != 0,
            })
        })?;
        rows.collect()
    }

    #[allow(dead_code)]
    pub fn get_session_commands(&self, session_id: &str) -> Result<Vec<CommandRecord>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, cmd, timestamp, directory, exit_code, session_id, shell, noisy
             FROM commands WHERE session_id = ?1 ORDER BY timestamp ASC",
        )?;
        let rows = stmt.query_map(params![session_id], |row| {
            let noisy_int: i32 = row.get(7)?;
            Ok(CommandRecord {
                id: Some(row.get(0)?),
                cmd: row.get(1)?,
                timestamp: row.get(2)?,
                directory: row.get(3)?,
                exit_code: row.get(4)?,
                session_id: row.get(5)?,
                shell: row.get(6)?,
                noisy: noisy_int != 0,
            })
        })?;
        rows.collect()
    }

    #[allow(dead_code)]
    pub fn update_noisy_flag(&self, command_id: i64, noisy: bool) -> Result<()> {
        self.conn.execute(
            "UPDATE commands SET noisy = ?1 WHERE id = ?2",
            params![noisy as i32, command_id],
        )?;
        Ok(())
    }

    pub fn get_latest_open_cluster(&self, session_id: &str) -> Result<Option<Cluster>> {
        let mut stmt = self.conn.prepare(
            "SELECT c.id, c.alias, c.created_at, c.last_used, c.directory, c.notes
             FROM clusters c
             JOIN cluster_commands cc ON c.id = cc.cluster_id
             JOIN commands cmd ON cc.command_id = cmd.id
             WHERE c.alias IS NULL AND cmd.session_id = ?1
             GROUP BY c.id
             ORDER BY c.created_at DESC
             LIMIT 1",
        )?;
        let mut rows = stmt.query_map(params![session_id], |row| {
            Ok(Cluster {
                id: Some(row.get(0)?),
                alias: row.get(1)?,
                created_at: row.get(2)?,
                last_used: row.get(3)?,
                directory: row.get(4)?,
                notes: row.get(5)?,
            })
        })?;
        match rows.next() {
            Some(row) => Ok(Some(row?)),
            None => Ok(None),
        }
    }

    pub fn get_most_recent_open_cluster(&self) -> Result<Option<Cluster>> {
        let mut stmt = self.conn.prepare(
            "SELECT c.id, c.alias, c.created_at, c.last_used, c.directory, c.notes
             FROM clusters c
             JOIN cluster_commands cc ON c.id = cc.cluster_id
             JOIN commands cmd ON cc.command_id = cmd.id
             WHERE c.alias IS NULL
             GROUP BY c.id
             ORDER BY MAX(cmd.timestamp) DESC
             LIMIT 1",
        )?;
        let mut rows = stmt.query_map([], |row| {
            Ok(Cluster {
                id: Some(row.get(0)?),
                alias: row.get(1)?,
                created_at: row.get(2)?,
                last_used: row.get(3)?,
                directory: row.get(4)?,
                notes: row.get(5)?,
            })
        })?;
        match rows.next() {
            Some(row) => Ok(Some(row?)),
            None => Ok(None),
        }
    }

    pub fn update_cluster_alias(&self, cluster_id: i64, alias: &str) -> Result<()> {
        let rows = self.conn.execute(
            "UPDATE clusters SET alias = ?1 WHERE id = ?2",
            params![alias, cluster_id],
        )?;
        if rows == 0 {
            return Err(rusqlite::Error::QueryReturnedNoRows);
        }
        Ok(())
    }

    pub fn update_cluster_last_used(&self, cluster_id: i64, timestamp: i64) -> Result<()> {
        self.conn.execute(
            "UPDATE clusters SET last_used = ?1 WHERE id = ?2",
            params![timestamp, cluster_id],
        )?;
        Ok(())
    }

    pub fn delete_cluster(&self, cluster_id: i64) -> Result<()> {
        self.conn.execute(
            "DELETE FROM cluster_tags WHERE cluster_id = ?1",
            params![cluster_id],
        )?;
        self.conn.execute(
            "DELETE FROM cluster_commands WHERE cluster_id = ?1",
            params![cluster_id],
        )?;
        self.conn.execute(
            "DELETE FROM clusters WHERE id = ?1",
            params![cluster_id],
        )?;
        Ok(())
    }

    pub fn get_last_meaningful_command_for_cluster(
        &self,
        cluster_id: i64,
    ) -> Result<Option<CommandRecord>> {
        let mut stmt = self.conn.prepare(
            "SELECT c.id, c.cmd, c.timestamp, c.directory, c.exit_code, c.session_id, c.shell, c.noisy
             FROM commands c
             JOIN cluster_commands cc ON c.id = cc.command_id
             WHERE cc.cluster_id = ?1 AND c.noisy = 0
             ORDER BY cc.position DESC
             LIMIT 1",
        )?;
        let mut rows = stmt.query_map(params![cluster_id], |row| {
            let noisy_int: i32 = row.get(7)?;
            Ok(CommandRecord {
                id: Some(row.get(0)?),
                cmd: row.get(1)?,
                timestamp: row.get(2)?,
                directory: row.get(3)?,
                exit_code: row.get(4)?,
                session_id: row.get(5)?,
                shell: row.get(6)?,
                noisy: noisy_int != 0,
            })
        })?;
        match rows.next() {
            Some(row) => Ok(Some(row?)),
            None => Ok(None),
        }
    }

    pub fn get_next_position_for_cluster(&self, cluster_id: i64) -> Result<i32> {
        let max_pos: Option<i32> = self.conn.query_row(
            "SELECT MAX(position) FROM cluster_commands WHERE cluster_id = ?1",
            params![cluster_id],
            |row| row.get(0),
        )?;
        Ok(max_pos.map_or(0, |p| p + 1))
    }

    pub fn add_tag_to_cluster(&self, cluster_id: i64, tag: &str) -> Result<()> {
        self.conn.execute(
            "INSERT OR IGNORE INTO cluster_tags (cluster_id, tag) VALUES (?1, ?2)",
            params![cluster_id, tag],
        )?;
        Ok(())
    }

    pub fn get_tags_for_cluster(&self, cluster_id: i64) -> Result<Vec<String>> {
        let mut stmt = self.conn.prepare(
            "SELECT tag FROM cluster_tags WHERE cluster_id = ?1 ORDER BY tag",
        )?;
        let rows = stmt.query_map(params![cluster_id], |row| row.get(0))?;
        rows.collect()
    }

    pub fn get_command_count_for_cluster(&self, cluster_id: i64) -> Result<usize> {
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM cluster_commands cc
             JOIN commands c ON c.id = cc.command_id
             WHERE cc.cluster_id = ?1 AND c.noisy = 0",
            params![cluster_id],
            |row| row.get(0),
        )?;
        Ok(count as usize)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_command(cmd: &str, timestamp: i64) -> CommandRecord {
        CommandRecord {
            id: None,
            cmd: cmd.to_string(),
            timestamp,
            directory: "/home/user/project".to_string(),
            exit_code: Some(0),
            session_id: "session-1".to_string(),
            shell: "zsh".to_string(),
            noisy: false,
        }
    }

    #[test]
    fn test_insert_and_retrieve_command() {
        let db = Database::open_in_memory().unwrap();
        let cmd = sample_command("kubectl get pods", 1700000000);
        let id = db.insert_command(&cmd).unwrap();
        assert!(id > 0);

        let recent = db.get_recent_commands(10).unwrap();
        assert_eq!(recent.len(), 1);
        assert_eq!(recent[0].cmd, "kubectl get pods");
        assert_eq!(recent[0].timestamp, 1700000000);
        assert_eq!(recent[0].id, Some(id));
    }

    #[test]
    fn test_recent_commands_ordering() {
        let db = Database::open_in_memory().unwrap();
        db.insert_command(&sample_command("first", 1000)).unwrap();
        db.insert_command(&sample_command("second", 2000)).unwrap();
        db.insert_command(&sample_command("third", 3000)).unwrap();

        let recent = db.get_recent_commands(2).unwrap();
        assert_eq!(recent.len(), 2);
        assert_eq!(recent[0].cmd, "third");
        assert_eq!(recent[1].cmd, "second");
    }

    #[test]
    fn test_insert_and_retrieve_cluster() {
        let db = Database::open_in_memory().unwrap();
        let cluster = Cluster {
            id: None,
            alias: Some("helm-debug".to_string()),
            created_at: 1700000000,
            last_used: Some(1700001000),
            directory: Some("/home/user/infra".to_string()),
            notes: None,
        };
        let cluster_id = db.insert_cluster(&cluster).unwrap();
        assert!(cluster_id > 0);

        let retrieved = db.get_cluster_by_alias("helm-debug").unwrap().unwrap();
        assert_eq!(retrieved.alias, Some("helm-debug".to_string()));
        assert_eq!(retrieved.created_at, 1700000000);
        assert_eq!(retrieved.last_used, Some(1700001000));
    }

    #[test]
    fn test_get_cluster_by_alias_not_found() {
        let db = Database::open_in_memory().unwrap();
        let result = db.get_cluster_by_alias("nonexistent").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_get_all_clusters_ordered_by_last_used() {
        let db = Database::open_in_memory().unwrap();
        db.insert_cluster(&Cluster {
            id: None,
            alias: Some("old".to_string()),
            created_at: 1000,
            last_used: Some(1000),
            directory: None,
            notes: None,
        })
        .unwrap();
        db.insert_cluster(&Cluster {
            id: None,
            alias: Some("recent".to_string()),
            created_at: 2000,
            last_used: Some(3000),
            directory: None,
            notes: None,
        })
        .unwrap();

        let all = db.get_all_clusters().unwrap();
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].alias, Some("recent".to_string()));
        assert_eq!(all[1].alias, Some("old".to_string()));
    }

    #[test]
    fn test_cluster_commands_relationship() {
        let db = Database::open_in_memory().unwrap();

        let cmd1_id = db.insert_command(&sample_command("helm list", 1000)).unwrap();
        let cmd2_id = db.insert_command(&sample_command("kubectl get pods", 1001)).unwrap();
        let cmd3_id = db.insert_command(&sample_command("kubectl logs pod-xyz", 1002)).unwrap();

        let cluster_id = db
            .insert_cluster(&Cluster {
                id: None,
                alias: Some("debug-flow".to_string()),
                created_at: 1000,
                last_used: None,
                directory: Some("/home/user/infra".to_string()),
                notes: None,
            })
            .unwrap();

        db.add_command_to_cluster(cluster_id, cmd1_id, 0).unwrap();
        db.add_command_to_cluster(cluster_id, cmd2_id, 1).unwrap();
        db.add_command_to_cluster(cluster_id, cmd3_id, 2).unwrap();

        let cmds = db.get_commands_for_cluster(cluster_id).unwrap();
        assert_eq!(cmds.len(), 3);
        assert_eq!(cmds[0].cmd, "helm list");
        assert_eq!(cmds[1].cmd, "kubectl get pods");
        assert_eq!(cmds[2].cmd, "kubectl logs pod-xyz");
    }

    #[test]
    fn test_cluster_tags() {
        let db = Database::open_in_memory().unwrap();
        let cluster_id = db
            .insert_cluster(&Cluster {
                id: None,
                alias: Some("k8s-debug".to_string()),
                created_at: 1000,
                last_used: None,
                directory: None,
                notes: None,
            })
            .unwrap();

        db.add_tag_to_cluster(cluster_id, "kubernetes").unwrap();
        db.add_tag_to_cluster(cluster_id, "helm").unwrap();
        // Duplicate should be ignored
        db.add_tag_to_cluster(cluster_id, "kubernetes").unwrap();

        let tags = db.get_tags_for_cluster(cluster_id).unwrap();
        assert_eq!(tags, vec!["helm", "kubernetes"]);
    }

    #[test]
    fn test_command_with_null_exit_code() {
        let db = Database::open_in_memory().unwrap();
        let cmd = CommandRecord {
            id: None,
            cmd: "some command".to_string(),
            timestamp: 1000,
            directory: "/tmp".to_string(),
            exit_code: None,
            session_id: "s1".to_string(),
            shell: "bash".to_string(),
            noisy: false,
        };
        let id = db.insert_command(&cmd).unwrap();
        let recent = db.get_recent_commands(1).unwrap();
        assert_eq!(recent[0].id, Some(id));
        assert_eq!(recent[0].exit_code, None);
    }

    #[test]
    fn test_open_creates_file_and_parents() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("nested").join("dir").join("lmc.db");
        let db = Database::open(&db_path).unwrap();
        db.insert_command(&sample_command("test", 1000)).unwrap();
        assert!(db_path.exists());
    }

    #[test]
    fn test_wal_mode_enabled() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test-wal.db");
        let db = Database::open(&db_path).unwrap();
        let mode: String = db
            .conn
            .query_row("PRAGMA journal_mode", [], |row| row.get(0))
            .unwrap();
        assert_eq!(mode, "wal");
    }

    #[test]
    fn test_insert_command_with_noisy_flag() {
        let db = Database::open_in_memory().unwrap();
        let mut cmd = sample_command("ls", 1000);
        cmd.noisy = true;
        let id = db.insert_command(&cmd).unwrap();

        let recent = db.get_recent_commands(1).unwrap();
        assert_eq!(recent[0].id, Some(id));
        assert!(recent[0].noisy);
    }

    #[test]
    fn test_noisy_defaults_to_false() {
        let db = Database::open_in_memory().unwrap();
        let cmd = sample_command("cargo build", 1000);
        db.insert_command(&cmd).unwrap();

        let recent = db.get_recent_commands(1).unwrap();
        assert!(!recent[0].noisy);
    }

    #[test]
    fn test_get_session_commands() {
        let db = Database::open_in_memory().unwrap();
        let mut cmd1 = sample_command("cargo build", 1000);
        cmd1.session_id = "sess-A".to_string();
        let mut cmd2 = sample_command("ls", 1001);
        cmd2.session_id = "sess-A".to_string();
        let mut cmd3 = sample_command("cargo test", 1002);
        cmd3.session_id = "sess-B".to_string();

        db.insert_command(&cmd1).unwrap();
        db.insert_command(&cmd2).unwrap();
        db.insert_command(&cmd3).unwrap();

        let session_cmds = db.get_session_commands("sess-A").unwrap();
        assert_eq!(session_cmds.len(), 2);
        assert_eq!(session_cmds[0].cmd, "cargo build");
        assert_eq!(session_cmds[1].cmd, "ls");
    }

    #[test]
    fn test_update_noisy_flag() {
        let db = Database::open_in_memory().unwrap();
        let cmd = sample_command("ls", 1000);
        let id = db.insert_command(&cmd).unwrap();

        // Initially not noisy
        let recent = db.get_recent_commands(1).unwrap();
        assert!(!recent[0].noisy);

        // Mark as noisy
        db.update_noisy_flag(id, true).unwrap();
        let recent = db.get_recent_commands(1).unwrap();
        assert!(recent[0].noisy);

        // Mark back as not noisy
        db.update_noisy_flag(id, false).unwrap();
        let recent = db.get_recent_commands(1).unwrap();
        assert!(!recent[0].noisy);
    }

    #[test]
    fn test_get_latest_open_cluster_returns_most_recent_for_session() {
        let db = Database::open_in_memory().unwrap();

        let cluster_id = db.insert_cluster(&Cluster {
            id: None,
            alias: None,
            created_at: 1000,
            last_used: None,
            directory: Some("/project".to_string()),
            notes: None,
        }).unwrap();
        let cmd_id = db.insert_command(&CommandRecord {
            id: None,
            cmd: "cargo build".to_string(),
            timestamp: 1000,
            directory: "/project".to_string(),
            exit_code: Some(0),
            session_id: "s1".to_string(),
            shell: "zsh".to_string(),
            noisy: false,
        }).unwrap();
        db.add_command_to_cluster(cluster_id, cmd_id, 0).unwrap();

        let result = db.get_latest_open_cluster("s1").unwrap();
        assert!(result.is_some());
        assert_eq!(result.unwrap().id, Some(cluster_id));
    }

    #[test]
    fn test_get_latest_open_cluster_ignores_saved_clusters() {
        let db = Database::open_in_memory().unwrap();

        let cluster_id = db.insert_cluster(&Cluster {
            id: None,
            alias: Some("saved".to_string()),
            created_at: 1000,
            last_used: None,
            directory: Some("/project".to_string()),
            notes: None,
        }).unwrap();
        let cmd_id = db.insert_command(&CommandRecord {
            id: None,
            cmd: "cargo build".to_string(),
            timestamp: 1000,
            directory: "/project".to_string(),
            exit_code: Some(0),
            session_id: "s1".to_string(),
            shell: "zsh".to_string(),
            noisy: false,
        }).unwrap();
        db.add_command_to_cluster(cluster_id, cmd_id, 0).unwrap();

        let result = db.get_latest_open_cluster("s1").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_get_latest_open_cluster_ignores_other_sessions() {
        let db = Database::open_in_memory().unwrap();

        let cluster_id = db.insert_cluster(&Cluster {
            id: None,
            alias: None,
            created_at: 1000,
            last_used: None,
            directory: Some("/project".to_string()),
            notes: None,
        }).unwrap();
        let cmd_id = db.insert_command(&CommandRecord {
            id: None,
            cmd: "cargo build".to_string(),
            timestamp: 1000,
            directory: "/project".to_string(),
            exit_code: Some(0),
            session_id: "s1".to_string(),
            shell: "zsh".to_string(),
            noisy: false,
        }).unwrap();
        db.add_command_to_cluster(cluster_id, cmd_id, 0).unwrap();

        let result = db.get_latest_open_cluster("s2").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_get_last_meaningful_command_for_cluster() {
        let db = Database::open_in_memory().unwrap();
        let cluster_id = db.insert_cluster(&Cluster {
            id: None, alias: None, created_at: 1000, last_used: None,
            directory: Some("/project".to_string()), notes: None,
        }).unwrap();
        let cmd1_id = db.insert_command(&CommandRecord {
            id: None, cmd: "cargo build".to_string(), timestamp: 1000,
            directory: "/project".to_string(), exit_code: Some(0),
            session_id: "s1".to_string(), shell: "zsh".to_string(), noisy: false,
        }).unwrap();
        db.add_command_to_cluster(cluster_id, cmd1_id, 0).unwrap();

        let result = db.get_last_meaningful_command_for_cluster(cluster_id).unwrap();
        assert!(result.is_some());
        assert_eq!(result.unwrap().cmd, "cargo build");
    }

    #[test]
    fn test_get_last_meaningful_command_skips_noisy() {
        let db = Database::open_in_memory().unwrap();
        let cluster_id = db.insert_cluster(&Cluster {
            id: None, alias: None, created_at: 1000, last_used: None,
            directory: Some("/project".to_string()), notes: None,
        }).unwrap();
        let cmd1_id = db.insert_command(&CommandRecord {
            id: None, cmd: "cargo build".to_string(), timestamp: 1000,
            directory: "/project".to_string(), exit_code: Some(0),
            session_id: "s1".to_string(), shell: "zsh".to_string(), noisy: false,
        }).unwrap();
        let cmd2_id = db.insert_command(&CommandRecord {
            id: None, cmd: "ls".to_string(), timestamp: 1010,
            directory: "/project".to_string(), exit_code: Some(0),
            session_id: "s1".to_string(), shell: "zsh".to_string(), noisy: true,
        }).unwrap();
        db.add_command_to_cluster(cluster_id, cmd1_id, 0).unwrap();
        db.add_command_to_cluster(cluster_id, cmd2_id, 1).unwrap();

        let result = db.get_last_meaningful_command_for_cluster(cluster_id).unwrap();
        assert!(result.is_some());
        assert_eq!(result.unwrap().cmd, "cargo build");
    }

    #[test]
    fn test_get_next_position_for_empty_cluster() {
        let db = Database::open_in_memory().unwrap();
        let cluster_id = db.insert_cluster(&Cluster {
            id: None, alias: None, created_at: 1000, last_used: None,
            directory: Some("/project".to_string()), notes: None,
        }).unwrap();
        let pos = db.get_next_position_for_cluster(cluster_id).unwrap();
        assert_eq!(pos, 0);
    }

    #[test]
    fn test_get_next_position_for_cluster_with_commands() {
        let db = Database::open_in_memory().unwrap();
        let cluster_id = db.insert_cluster(&Cluster {
            id: None, alias: None, created_at: 1000, last_used: None,
            directory: Some("/project".to_string()), notes: None,
        }).unwrap();
        let cmd_id = db.insert_command(&sample_command("cargo build", 1000)).unwrap();
        db.add_command_to_cluster(cluster_id, cmd_id, 0).unwrap();
        let pos = db.get_next_position_for_cluster(cluster_id).unwrap();
        assert_eq!(pos, 1);
    }

    #[test]
    fn test_record_roundtrip_file_db() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("roundtrip.db");

        let db = Database::open(&db_path).unwrap();
        let cmd = CommandRecord {
            id: None,
            cmd: "helm list -n production".to_string(),
            timestamp: 1700000000,
            directory: "/home/user/infra".to_string(),
            exit_code: Some(0),
            session_id: "1234_1700000000".to_string(),
            shell: "zsh".to_string(),
            noisy: false,
        };

        let id = db.insert_command(&cmd).unwrap();
        assert!(id > 0);

        let recent = db.get_recent_commands(1).unwrap();
        assert_eq!(recent.len(), 1);
        assert_eq!(recent[0].cmd, "helm list -n production");
        assert_eq!(recent[0].session_id, "1234_1700000000");
        assert_eq!(recent[0].shell, "zsh");
    }

    #[test]
    fn test_get_most_recent_open_cluster_returns_latest() {
        let db = Database::open_in_memory().unwrap();

        // Cluster 1: older commands
        let c1_id = db.insert_cluster(&Cluster {
            id: None, alias: None, created_at: 1000, last_used: None,
            directory: Some("/p1".to_string()), notes: None,
        }).unwrap();
        let cmd1_id = db.insert_command(&CommandRecord {
            id: None, cmd: "cargo build".to_string(), timestamp: 1000,
            directory: "/p1".to_string(), exit_code: Some(0),
            session_id: "s1".to_string(), shell: "zsh".to_string(), noisy: false,
        }).unwrap();
        db.add_command_to_cluster(c1_id, cmd1_id, 0).unwrap();

        // Cluster 2: newer commands
        let c2_id = db.insert_cluster(&Cluster {
            id: None, alias: None, created_at: 2000, last_used: None,
            directory: Some("/p2".to_string()), notes: None,
        }).unwrap();
        let cmd2_id = db.insert_command(&CommandRecord {
            id: None, cmd: "npm install".to_string(), timestamp: 2000,
            directory: "/p2".to_string(), exit_code: Some(0),
            session_id: "s2".to_string(), shell: "zsh".to_string(), noisy: false,
        }).unwrap();
        db.add_command_to_cluster(c2_id, cmd2_id, 0).unwrap();

        let result = db.get_most_recent_open_cluster().unwrap();
        assert!(result.is_some());
        assert_eq!(result.unwrap().id, Some(c2_id));
    }

    #[test]
    fn test_get_most_recent_open_cluster_ignores_aliased() {
        let db = Database::open_in_memory().unwrap();

        let c1_id = db.insert_cluster(&Cluster {
            id: None, alias: Some("saved".to_string()), created_at: 2000, last_used: None,
            directory: Some("/p1".to_string()), notes: None,
        }).unwrap();
        let cmd1_id = db.insert_command(&CommandRecord {
            id: None, cmd: "cargo build".to_string(), timestamp: 2000,
            directory: "/p1".to_string(), exit_code: Some(0),
            session_id: "s1".to_string(), shell: "zsh".to_string(), noisy: false,
        }).unwrap();
        db.add_command_to_cluster(c1_id, cmd1_id, 0).unwrap();

        let result = db.get_most_recent_open_cluster().unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_get_most_recent_open_cluster_none_when_empty() {
        let db = Database::open_in_memory().unwrap();
        let result = db.get_most_recent_open_cluster().unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_update_cluster_alias_sets_alias() {
        let db = Database::open_in_memory().unwrap();
        let cluster_id = db.insert_cluster(&Cluster {
            id: None, alias: None, created_at: 1000, last_used: None,
            directory: Some("/project".to_string()), notes: None,
        }).unwrap();

        db.update_cluster_alias(cluster_id, "my-alias").unwrap();

        let retrieved = db.get_cluster_by_alias("my-alias").unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().id, Some(cluster_id));
    }

    #[test]
    fn test_update_cluster_alias_can_rename_existing() {
        let db = Database::open_in_memory().unwrap();
        let cluster_id = db.insert_cluster(&Cluster {
            id: None, alias: Some("old-name".to_string()), created_at: 1000, last_used: None,
            directory: Some("/project".to_string()), notes: None,
        }).unwrap();

        db.update_cluster_alias(cluster_id, "new-name").unwrap();

        assert!(db.get_cluster_by_alias("old-name").unwrap().is_none());
        assert!(db.get_cluster_by_alias("new-name").unwrap().is_some());
    }

    #[test]
    fn test_update_cluster_alias_nonexistent_id_returns_error() {
        let db = Database::open_in_memory().unwrap();
        let result = db.update_cluster_alias(9999, "ghost");
        assert!(result.is_err());
    }

    #[test]
    fn test_delete_cluster_removes_cluster_tags_and_links() {
        let db = Database::open_in_memory().unwrap();
        let cluster_id = db.insert_cluster(&Cluster {
            id: None, alias: Some("to-delete".to_string()), created_at: 1000, last_used: None,
            directory: Some("/project".to_string()), notes: None,
        }).unwrap();
        let cmd_id = db.insert_command(&CommandRecord {
            id: None, cmd: "cargo build".to_string(), timestamp: 1000,
            directory: "/project".to_string(), exit_code: Some(0),
            session_id: "s1".to_string(), shell: "zsh".to_string(), noisy: false,
        }).unwrap();
        db.add_command_to_cluster(cluster_id, cmd_id, 0).unwrap();
        db.add_tag_to_cluster(cluster_id, "rust").unwrap();

        db.delete_cluster(cluster_id).unwrap();

        assert!(db.get_cluster_by_alias("to-delete").unwrap().is_none());
        assert!(db.get_commands_for_cluster(cluster_id).unwrap().is_empty());
        assert!(db.get_tags_for_cluster(cluster_id).unwrap().is_empty());
    }

    #[test]
    fn test_delete_cluster_preserves_underlying_commands() {
        // Deleting a cluster must NOT delete the raw commands from the commands table
        let db = Database::open_in_memory().unwrap();
        let cluster_id = db.insert_cluster(&Cluster {
            id: None, alias: Some("to-delete".to_string()), created_at: 1000, last_used: None,
            directory: Some("/project".to_string()), notes: None,
        }).unwrap();
        let cmd_id = db.insert_command(&CommandRecord {
            id: None, cmd: "cargo build".to_string(), timestamp: 1000,
            directory: "/project".to_string(), exit_code: Some(0),
            session_id: "s1".to_string(), shell: "zsh".to_string(), noisy: false,
        }).unwrap();
        db.add_command_to_cluster(cluster_id, cmd_id, 0).unwrap();

        db.delete_cluster(cluster_id).unwrap();

        let all_cmds = db.get_recent_commands(10).unwrap();
        assert_eq!(all_cmds.len(), 1);
        assert_eq!(all_cmds[0].cmd, "cargo build");
    }

    #[test]
    fn test_get_command_count_for_cluster() {
        let db = Database::open_in_memory().unwrap();

        let cmd_id = db.insert_command(&CommandRecord {
            id: None,
            cmd: "git status".to_string(),
            timestamp: 1000,
            directory: "/tmp".to_string(),
            exit_code: Some(0),
            session_id: "s1".to_string(),
            shell: "zsh".to_string(),
            noisy: false,
        }).unwrap();

        let cluster_id = db.insert_cluster(&Cluster {
            id: None,
            alias: None,
            created_at: 1000,
            last_used: None,
            directory: None,
            notes: None,
        }).unwrap();
        db.add_command_to_cluster(cluster_id, cmd_id, 0).unwrap();

        let count = db.get_command_count_for_cluster(cluster_id).unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn test_get_command_count_excludes_noisy() {
        let db = Database::open_in_memory().unwrap();

        let noisy_id = db.insert_command(&CommandRecord {
            id: None,
            cmd: "ls".to_string(),
            timestamp: 1000,
            directory: "/tmp".to_string(),
            exit_code: Some(0),
            session_id: "s1".to_string(),
            shell: "zsh".to_string(),
            noisy: true,
        }).unwrap();

        let real_id = db.insert_command(&CommandRecord {
            id: None,
            cmd: "git diff".to_string(),
            timestamp: 1001,
            directory: "/tmp".to_string(),
            exit_code: Some(0),
            session_id: "s1".to_string(),
            shell: "zsh".to_string(),
            noisy: false,
        }).unwrap();

        let cluster_id = db.insert_cluster(&Cluster {
            id: None,
            alias: None,
            created_at: 1000,
            last_used: None,
            directory: None,
            notes: None,
        }).unwrap();
        db.add_command_to_cluster(cluster_id, noisy_id, 0).unwrap();
        db.add_command_to_cluster(cluster_id, real_id, 1).unwrap();

        let count = db.get_command_count_for_cluster(cluster_id).unwrap();
        assert_eq!(count, 1); // only non-noisy
    }

    #[test]
    fn test_update_cluster_last_used() {
        let db = Database::open_in_memory().unwrap();
        let cluster_id = db.insert_cluster(&Cluster {
            id: None, alias: None, created_at: 1000, last_used: None,
            directory: None, notes: None,
        }).unwrap();

        db.update_cluster_last_used(cluster_id, 9999).unwrap();

        let clusters = db.get_all_clusters().unwrap();
        let c = clusters.iter().find(|c| c.id == Some(cluster_id)).unwrap();
        assert_eq!(c.last_used, Some(9999));
    }

    #[test]
    fn test_get_command_count_empty_cluster() {
        let db = Database::open_in_memory().unwrap();
        let cluster_id = db.insert_cluster(&Cluster {
            id: None,
            alias: None,
            created_at: 1000,
            last_used: None,
            directory: None,
            notes: None,
        }).unwrap();
        let count = db.get_command_count_for_cluster(cluster_id).unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn test_get_clusters_by_tags_and_returns_only_full_match() {
        let db = Database::open_in_memory().unwrap();

        // cluster with both kubernetes + helm
        let c1 = db.insert_cluster(&Cluster {
            id: None,
            alias: Some("kube-debug".to_string()),
            created_at: 1000,
            last_used: Some(2000),
            directory: None,
            notes: None,
        }).unwrap();
        db.add_tag_to_cluster(c1, "kubernetes").unwrap();
        db.add_tag_to_cluster(c1, "helm").unwrap();

        // cluster with only kubernetes
        let c2 = db.insert_cluster(&Cluster {
            id: None,
            alias: Some("kube-only".to_string()),
            created_at: 1000,
            last_used: Some(1000),
            directory: None,
            notes: None,
        }).unwrap();
        db.add_tag_to_cluster(c2, "kubernetes").unwrap();

        let results = db.get_clusters_by_tags(
            &["kubernetes".to_string(), "helm".to_string()],
            true,
        ).unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].alias, Some("kube-debug".to_string()));
    }

    #[test]
    fn test_get_clusters_by_tags_or_returns_any_match() {
        let db = Database::open_in_memory().unwrap();

        let c1 = db.insert_cluster(&Cluster {
            id: None,
            alias: Some("kube-debug".to_string()),
            created_at: 1000,
            last_used: Some(3000),
            directory: None,
            notes: None,
        }).unwrap();
        db.add_tag_to_cluster(c1, "kubernetes").unwrap();

        let c2 = db.insert_cluster(&Cluster {
            id: None,
            alias: Some("helm-release".to_string()),
            created_at: 1000,
            last_used: Some(2000),
            directory: None,
            notes: None,
        }).unwrap();
        db.add_tag_to_cluster(c2, "helm").unwrap();

        let c3 = db.insert_cluster(&Cluster {
            id: None,
            alias: Some("docker-build".to_string()),
            created_at: 1000,
            last_used: Some(1000),
            directory: None,
            notes: None,
        }).unwrap();
        db.add_tag_to_cluster(c3, "docker").unwrap();

        let results = db.get_clusters_by_tags(
            &["kubernetes".to_string(), "helm".to_string()],
            false,
        ).unwrap();

        assert_eq!(results.len(), 2);
        assert_eq!(results[0].alias, Some("kube-debug".to_string()));
        assert_eq!(results[1].alias, Some("helm-release".to_string()));
    }

    #[test]
    fn test_get_clusters_by_tags_empty_returns_all() {
        let db = Database::open_in_memory().unwrap();
        db.insert_cluster(&Cluster {
            id: None,
            alias: Some("a".to_string()),
            created_at: 1000,
            last_used: None,
            directory: None,
            notes: None,
        }).unwrap();

        let result_all = db.get_all_clusters().unwrap();
        let result_tags = db.get_clusters_by_tags(&[], true).unwrap();
        assert_eq!(result_all.len(), result_tags.len());
    }

    #[test]
    fn test_get_clusters_by_tags_no_match_returns_empty() {
        let db = Database::open_in_memory().unwrap();
        let c1 = db.insert_cluster(&Cluster {
            id: None,
            alias: Some("kube-debug".to_string()),
            created_at: 1000,
            last_used: Some(1000),
            directory: None,
            notes: None,
        }).unwrap();
        db.add_tag_to_cluster(c1, "kubernetes").unwrap();

        let results = db.get_clusters_by_tags(&["docker".to_string()], true).unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn test_replace_cluster_commands_updates_list() {
        let db = Database::open_in_memory().unwrap();

        let cluster_id = db.insert_cluster(&Cluster {
            id: None, alias: None, created_at: 1000, last_used: None,
            directory: Some("/p".to_string()), notes: None,
        }).unwrap();

        let id1 = db.insert_command(&CommandRecord {
            id: None, cmd: "cmd1".to_string(), timestamp: 1000,
            directory: "/p".to_string(), exit_code: Some(0),
            session_id: "s1".to_string(), shell: "zsh".to_string(), noisy: false,
        }).unwrap();
        let id2 = db.insert_command(&CommandRecord {
            id: None, cmd: "cmd2".to_string(), timestamp: 1010,
            directory: "/p".to_string(), exit_code: Some(0),
            session_id: "s1".to_string(), shell: "zsh".to_string(), noisy: false,
        }).unwrap();
        let id3 = db.insert_command(&CommandRecord {
            id: None, cmd: "cmd3".to_string(), timestamp: 1020,
            directory: "/p".to_string(), exit_code: Some(0),
            session_id: "s1".to_string(), shell: "zsh".to_string(), noisy: false,
        }).unwrap();

        db.add_command_to_cluster(cluster_id, id1, 0).unwrap();
        db.add_command_to_cluster(cluster_id, id2, 1).unwrap();
        db.add_command_to_cluster(cluster_id, id3, 2).unwrap();

        // Replace with only cmd1 and cmd3 (simulating cmd2 deleted)
        let keep = vec![
            CommandRecord { id: Some(id1), cmd: "cmd1".to_string(), timestamp: 1000,
                directory: "/p".to_string(), exit_code: Some(0),
                session_id: "s1".to_string(), shell: "zsh".to_string(), noisy: false },
            CommandRecord { id: Some(id3), cmd: "cmd3".to_string(), timestamp: 1020,
                directory: "/p".to_string(), exit_code: Some(0),
                session_id: "s1".to_string(), shell: "zsh".to_string(), noisy: false },
        ];
        db.replace_cluster_commands(cluster_id, &keep).unwrap();

        let result = db.get_commands_for_cluster(cluster_id).unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].cmd, "cmd1");
        assert_eq!(result[1].cmd, "cmd3");
    }

    #[test]
    fn test_replace_cluster_commands_single_item_survives() {
        let db = Database::open_in_memory().unwrap();

        let cluster_id = db.insert_cluster(&Cluster {
            id: None, alias: None, created_at: 1000, last_used: None,
            directory: Some("/p".to_string()), notes: None,
        }).unwrap();

        let id1 = db.insert_command(&CommandRecord {
            id: None, cmd: "a".to_string(), timestamp: 1000,
            directory: "/p".to_string(), exit_code: Some(0),
            session_id: "s1".to_string(), shell: "zsh".to_string(), noisy: false,
        }).unwrap();
        let id2 = db.insert_command(&CommandRecord {
            id: None, cmd: "b".to_string(), timestamp: 1010,
            directory: "/p".to_string(), exit_code: Some(0),
            session_id: "s1".to_string(), shell: "zsh".to_string(), noisy: false,
        }).unwrap();

        db.add_command_to_cluster(cluster_id, id1, 0).unwrap();
        db.add_command_to_cluster(cluster_id, id2, 1).unwrap();

        let keep = vec![
            CommandRecord { id: Some(id2), cmd: "b".to_string(), timestamp: 1010,
                directory: "/p".to_string(), exit_code: Some(0),
                session_id: "s1".to_string(), shell: "zsh".to_string(), noisy: false },
        ];
        db.replace_cluster_commands(cluster_id, &keep).unwrap();

        let result = db.get_commands_for_cluster(cluster_id).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].cmd, "b");
    }

    #[test]
    fn test_replace_cluster_commands_empty_list() {
        let db = Database::open_in_memory().unwrap();

        let cluster_id = db.insert_cluster(&Cluster {
            id: None, alias: None, created_at: 1000, last_used: None,
            directory: Some("/p".to_string()), notes: None,
        }).unwrap();

        let id1 = db.insert_command(&CommandRecord {
            id: None, cmd: "a".to_string(), timestamp: 1000,
            directory: "/p".to_string(), exit_code: Some(0),
            session_id: "s1".to_string(), shell: "zsh".to_string(), noisy: false,
        }).unwrap();

        db.add_command_to_cluster(cluster_id, id1, 0).unwrap();
        db.replace_cluster_commands(cluster_id, &[]).unwrap();

        let result = db.get_commands_for_cluster(cluster_id).unwrap();
        assert!(result.is_empty());
    }
}
