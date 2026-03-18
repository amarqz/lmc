mod cli;
mod cluster;
mod config;
mod db;
mod shell;
mod ui;

use clap::Parser;
use cli::{Cli, Command};

fn main() {
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
}
