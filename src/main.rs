mod cli;
mod cluster;
mod config;
mod db;
mod filter;
mod save;
mod shell;
mod tags;
mod ui;

use clap::Parser;
use cli::{Cli, Command};

fn main() {
    let cfg = match config::load_config() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Error loading config: {e}");
            std::process::exit(1);
        }
    };

    let db_path = config::resolve_db_path(&cfg);
    let cli = Cli::parse();

    match cli.command {
        Some(Command::Save { alias, from }) => {
            println!("TODO: save cluster as '{alias}' (from: {from:?})");
        }
        Some(Command::Init { shell }) => {
            match shell.as_str() {
                "zsh" => print!("{}", shell::init_zsh()),
                "bash" => print!("{}", shell::init_bash()),
                "fish" => print!("{}", shell::init_fish()),
                _ => {
                    eprintln!("Unsupported shell: {shell}. Supported: zsh, bash, fish");
                    std::process::exit(1);
                }
            }
        }
        Some(Command::Record { cmd, dir, exit_code, session_id, shell }) => {
            let timestamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as i64;

            let noisy = filter::is_noisy(&cmd, &cfg.noise_filter);

            let record = db::CommandRecord {
                id: None,
                cmd,
                timestamp,
                directory: dir,
                exit_code,
                session_id,
                shell,
                noisy,
            };

            // Silent operation: never interfere with the user's shell
            let _ = db::Database::open(&db_path).and_then(|db| {
                let command_id = db.insert_command(&record)?;
                if let Ok(Some(cluster_id)) = cluster::assign_to_cluster(&db, &record, command_id, cfg.general.cluster_gap_minutes) {
                    let inferred = tags::infer_tags_for_command(&record.cmd, &cfg.tag_inference);
                    for tag in &inferred {
                        let _ = db.add_tag_to_cluster(cluster_id, tag);
                    }
                }
                Ok(())
            });
        }
        None => {
            match cli.query {
                Some(query) => {
                    println!("TODO: look up alias or search for '{query}'");
                }
                None => {
                    println!("TODO: show index of all saved aliases");
                }
            }
        }
    }


}
