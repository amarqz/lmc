mod cli;
mod cluster;
mod config;
mod db;
mod shell;
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
            println!("TODO: generate hook for {shell}");
        }
        Some(Command::Record { cmd, dir, exit_code, session_id, shell }) => {
            println!("TODO: record command '{cmd}' in {dir} (exit: {exit_code:?}, session: {session_id}, shell: {shell})");
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

    // Suppress unused variable warning until db_path is used by the storage layer
    let _ = db_path;
}
