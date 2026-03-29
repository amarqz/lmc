use crate::db::{CommandRecord, Database};
use anyhow::Result;

pub struct App {
    pub alias: String,
    pub commands: Vec<CommandRecord>,
    pub tags: Vec<String>,
    pub last_used: Option<i64>,
    pub selected: usize,
}

impl App {
    pub fn new(
        alias: String,
        commands: Vec<CommandRecord>,
        tags: Vec<String>,
        last_used: Option<i64>,
    ) -> Self {
        Self {
            alias,
            commands,
            tags,
            last_used,
            selected: 0,
        }
    }

    pub fn selected_command(&self) -> Option<&CommandRecord> {
        self.commands.get(self.selected)
    }

    pub fn move_up(&mut self) {
        if self.commands.is_empty() {
            return;
        }
        if self.selected == 0 {
            self.selected = self.commands.len() - 1;
        } else {
            self.selected -= 1;
        }
    }

    pub fn move_down(&mut self) {
        if self.commands.is_empty() {
            return;
        }
        self.selected = (self.selected + 1) % self.commands.len();
    }
}

pub fn format_relative_time(now_secs: i64, then_secs: i64) -> String {
    let diff = now_secs.saturating_sub(then_secs);
    if diff < 60 {
        "just now".to_string()
    } else if diff < 3600 {
        let n = diff / 60;
        format!("{} minute{} ago", n, if n == 1 { "" } else { "s" })
    } else if diff < 86400 {
        let n = diff / 3600;
        format!("{} hour{} ago", n, if n == 1 { "" } else { "s" })
    } else if diff < 86400 * 7 {
        let n = diff / 86400;
        format!("{} day{} ago", n, if n == 1 { "" } else { "s" })
    } else if diff < 86400 * 30 {
        let n = diff / (86400 * 7);
        format!("{} week{} ago", n, if n == 1 { "" } else { "s" })
    } else {
        let n = diff / (86400 * 30);
        format!("{} month{} ago", n, if n == 1 { "" } else { "s" })
    }
}

pub fn alias_not_found_message(alias: &str, saved_aliases: &[String]) -> String {
    if saved_aliases.is_empty() {
        format!(
            "lmc: no alias \"{}\" found\nNo aliases saved yet. Run `lmc save <alias>` after a session.",
            alias
        )
    } else {
        let names = saved_aliases.join(", ");
        format!(
            "lmc: no alias \"{}\" found\nSaved aliases: {}\nRun `lmc` to browse all aliases.",
            alias, names
        )
    }
}

pub fn run(alias: &str, db: &Database) -> Result<()> {
    let cluster = db.get_cluster_by_alias(alias)?;
    let cluster = match cluster {
        Some(c) => c,
        None => {
            let all = db.get_all_clusters()?;
            let saved: Vec<String> = all
                .into_iter()
                .filter_map(|c| c.alias)
                .collect();
            eprintln!("{}", alias_not_found_message(alias, &saved));
            std::process::exit(1);
        }
    };

    let cluster_id = cluster.id.expect("cluster from DB always has id");
    let all_commands = db.get_commands_for_cluster(cluster_id)?;
    let commands: Vec<_> = all_commands.into_iter().filter(|c| !c.noisy).collect();

    if commands.is_empty() {
        eprintln!("lmc: cluster \"{}\" has no commands.", alias);
        return Ok(());
    }

    let tags = db.get_tags_for_cluster(cluster_id)?;
    let mut app = App::new(alias.to_string(), commands, tags, cluster.last_used);

    run_tui(&mut app)
}

fn run_tui(app: &mut App) -> Result<()> {
    use crossterm::{
        event::{self, Event, KeyCode, KeyEventKind},
        execute,
        terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    };
    use ratatui::{backend::CrosstermBackend, Terminal};
    use std::io;

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let copied_cmd: Option<String> = loop {
        terminal.draw(|frame| crate::ui::draw(frame, app))?;

        if let Event::Key(key) = event::read()? {
            if key.kind != KeyEventKind::Press {
                continue;
            }
            match key.code {
                KeyCode::Up | KeyCode::Char('k') => app.move_up(),
                KeyCode::Down | KeyCode::Char('j') => app.move_down(),
                KeyCode::Enter => {
                    if let Some(cmd) = app.selected_command() {
                        break Some(cmd.cmd.clone());
                    }
                }
                KeyCode::Char(c) if c.is_ascii_digit() && c != '0' => {
                    let idx = (c as u8 - b'1') as usize;
                    if idx < app.commands.len() {
                        app.selected = idx;
                        if let Some(cmd) = app.selected_command() {
                            break Some(cmd.cmd.clone());
                        }
                    }
                }
                KeyCode::Char('q') | KeyCode::Esc => break None,
                _ => {}
            }
        }
    };

    // Always restore terminal before doing anything else
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    if let Some(cmd) = copied_cmd {
        match arboard::Clipboard::new().and_then(|mut c| c.set_text(&cmd)) {
            Ok(_) => println!("Copied: {}", cmd),
            Err(_) => {
                eprintln!("Warning: clipboard not available.");
                println!("{}", cmd);
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::CommandRecord;

    fn make_cmd(cmd: &str) -> CommandRecord {
        CommandRecord {
            id: Some(1),
            cmd: cmd.to_string(),
            timestamp: 1000,
            directory: "/tmp".to_string(),
            exit_code: Some(0),
            session_id: "s1".to_string(),
            shell: "zsh".to_string(),
            noisy: false,
        }
    }

    #[test]
    fn test_app_new_starts_at_zero() {
        let cmds = vec![make_cmd("git status"), make_cmd("git diff")];
        let app = App::new("my-alias".to_string(), cmds, vec![], None);
        assert_eq!(app.selected, 0);
        assert_eq!(app.alias, "my-alias");
        assert_eq!(app.commands.len(), 2);
    }

    #[test]
    fn test_selected_command_returns_correct_item() {
        let cmds = vec![make_cmd("git status"), make_cmd("git diff")];
        let mut app = App::new("x".to_string(), cmds, vec![], None);
        app.selected = 1;
        assert_eq!(app.selected_command().unwrap().cmd, "git diff");
    }

    #[test]
    fn test_selected_command_empty_returns_none() {
        let app = App::new("x".to_string(), vec![], vec![], None);
        assert!(app.selected_command().is_none());
    }

    #[test]
    fn test_move_down_advances_selection() {
        let cmds = vec![make_cmd("a"), make_cmd("b"), make_cmd("c")];
        let mut app = App::new("x".to_string(), cmds, vec![], None);
        app.move_down();
        assert_eq!(app.selected, 1);
    }

    #[test]
    fn test_move_down_wraps_to_zero() {
        let cmds = vec![make_cmd("a"), make_cmd("b"), make_cmd("c")];
        let mut app = App::new("x".to_string(), cmds, vec![], None);
        app.selected = 2;
        app.move_down();
        assert_eq!(app.selected, 0);
    }

    #[test]
    fn test_move_up_decrements_selection() {
        let cmds = vec![make_cmd("a"), make_cmd("b"), make_cmd("c")];
        let mut app = App::new("x".to_string(), cmds, vec![], None);
        app.selected = 2;
        app.move_up();
        assert_eq!(app.selected, 1);
    }

    #[test]
    fn test_move_up_wraps_to_last() {
        let cmds = vec![make_cmd("a"), make_cmd("b"), make_cmd("c")];
        let mut app = App::new("x".to_string(), cmds, vec![], None);
        app.move_up();
        assert_eq!(app.selected, 2);
    }

    #[test]
    fn test_move_on_empty_does_nothing() {
        let mut app = App::new("x".to_string(), vec![], vec![], None);
        app.move_up();
        app.move_down();
        assert_eq!(app.selected, 0);
    }

    #[test]
    fn test_relative_time_just_now() {
        assert_eq!(format_relative_time(100, 95), "just now");
    }

    #[test]
    fn test_relative_time_minutes() {
        assert_eq!(format_relative_time(3700, 100), "1 hour ago");
    }

    #[test]
    fn test_relative_time_days() {
        let now = 86400 * 5;
        assert_eq!(format_relative_time(now, 0), "5 days ago");
    }

    #[test]
    fn test_relative_time_weeks() {
        let now = 86400 * 14;
        assert_eq!(format_relative_time(now, 0), "2 weeks ago");
    }

    #[test]
    fn test_relative_time_plural_vs_singular() {
        assert_eq!(format_relative_time(86400, 0), "1 day ago");
        assert_eq!(format_relative_time(86400 * 2, 0), "2 days ago");
    }

    #[test]
    fn test_alias_not_found_with_suggestions() {
        let msg = alias_not_found_message("helm-dbg", &["helm-debug-prod".to_string(), "db-migrate".to_string()]);
        assert!(msg.contains("helm-dbg"));
        assert!(msg.contains("helm-debug-prod"));
        assert!(msg.contains("db-migrate"));
        assert!(msg.contains("lmc` to browse"));
    }

    #[test]
    fn test_alias_not_found_no_aliases() {
        let msg = alias_not_found_message("helm-dbg", &[]);
        assert!(msg.contains("helm-dbg"));
        assert!(msg.contains("lmc save"));
    }
}
