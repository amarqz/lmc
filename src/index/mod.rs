use crate::db::Database;
use anyhow::Result;

pub struct IndexEntry {
    pub alias: String,
    pub last_used: Option<i64>,
    pub command_count: usize,
    pub tags: Vec<String>,
}

pub struct IndexApp {
    pub entries: Vec<IndexEntry>,
    pub selected: usize,
}

impl IndexApp {
    pub fn new(entries: Vec<IndexEntry>) -> Self {
        Self { entries, selected: 0 }
    }

    pub fn selected_entry(&self) -> Option<&IndexEntry> {
        self.entries.get(self.selected)
    }

    pub fn move_up(&mut self) {
        if self.entries.is_empty() {
            return;
        }
        if self.selected == 0 {
            self.selected = self.entries.len() - 1;
        } else {
            self.selected -= 1;
        }
    }

    pub fn move_down(&mut self) {
        if self.entries.is_empty() {
            return;
        }
        self.selected = (self.selected + 1) % self.entries.len();
    }
}

pub fn run(db: &Database) -> Result<()> {
    let clusters = db.get_all_clusters()?;
    let aliased: Vec<_> = clusters.into_iter().filter(|c| c.alias.is_some()).collect();

    if aliased.is_empty() {
        eprintln!(
            "No aliases saved yet. Run `lmc save <alias>` to save your first cluster."
        );
        return Ok(());
    }

    let mut entries = Vec::new();
    for cluster in &aliased {
        let id = cluster.id.expect("cluster from DB always has id");
        let tags = db.get_tags_for_cluster(id)?;
        let command_count = db.get_command_count_for_cluster(id)?;
        entries.push(IndexEntry {
            alias: cluster.alias.clone().unwrap(),
            last_used: cluster.last_used,
            command_count,
            tags,
        });
    }

    let mut app = IndexApp::new(entries);
    let selected_alias = run_tui(&mut app)?;

    if let Some(alias) = selected_alias {
        crate::retrieval::run(&alias, db)?;
    }

    Ok(())
}

pub fn run_tui(app: &mut IndexApp) -> Result<Option<String>> {
    use crossterm::terminal::{disable_raw_mode, enable_raw_mode};

    enable_raw_mode()?;
    let result = run_tui_inner(app);
    let _ = disable_raw_mode();
    result
}

fn run_tui_inner(app: &mut IndexApp) -> Result<Option<String>> {
    use crossterm::{
        cursor::{MoveToColumn, MoveUp},
        event::{self, Event, KeyCode, KeyEventKind},
        execute,
        terminal::{size as terminal_size, Clear, ClearType},
    };
    use ratatui::{backend::CrosstermBackend, Terminal, TerminalOptions, Viewport};
    use std::io;

    // header (1) + entries + status bar (1) + 1 padding
    let desired = (app.entries.len() + 3) as u16;
    let max_height = terminal_size()
        .map(|(_, rows)| rows.saturating_sub(2))
        .unwrap_or(20);
    let height = desired.min(max_height);

    let backend = CrosstermBackend::new(io::stdout());
    let mut terminal = Terminal::with_options(
        backend,
        TerminalOptions {
            viewport: Viewport::Inline(height),
        },
    )?;

    let result = loop {
        terminal.draw(|frame| crate::ui::draw_index(frame, app))?;

        if let Event::Key(key) = event::read()? {
            if key.kind != KeyEventKind::Press {
                continue;
            }
            match key.code {
                KeyCode::Up | KeyCode::Char('k') => app.move_up(),
                KeyCode::Down | KeyCode::Char('j') => app.move_down(),
                KeyCode::Enter => {
                    if let Some(entry) = app.selected_entry() {
                        break Some(entry.alias.clone());
                    }
                }
                KeyCode::Char('q') | KeyCode::Esc => break None,
                _ => {}
            }
        }
    };

    let _ = execute!(
        io::stdout(),
        MoveUp(height),
        MoveToColumn(0),
        Clear(ClearType::FromCursorDown)
    );

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_entry(alias: &str) -> IndexEntry {
        IndexEntry {
            alias: alias.to_string(),
            last_used: Some(1000),
            command_count: 3,
            tags: vec!["git".to_string()],
        }
    }

    #[test]
    fn test_new_starts_at_zero() {
        let app = IndexApp::new(vec![make_entry("a"), make_entry("b")]);
        assert_eq!(app.selected, 0);
    }

    #[test]
    fn test_selected_entry_returns_correct() {
        let mut app = IndexApp::new(vec![make_entry("a"), make_entry("b")]);
        app.selected = 1;
        assert_eq!(app.selected_entry().unwrap().alias, "b");
    }

    #[test]
    fn test_selected_entry_empty_returns_none() {
        let app = IndexApp::new(vec![]);
        assert!(app.selected_entry().is_none());
    }

    #[test]
    fn test_move_down_advances() {
        let mut app = IndexApp::new(vec![make_entry("a"), make_entry("b"), make_entry("c")]);
        app.move_down();
        assert_eq!(app.selected, 1);
    }

    #[test]
    fn test_move_down_wraps() {
        let mut app = IndexApp::new(vec![make_entry("a"), make_entry("b"), make_entry("c")]);
        app.selected = 2;
        app.move_down();
        assert_eq!(app.selected, 0);
    }

    #[test]
    fn test_move_up_decrements() {
        let mut app = IndexApp::new(vec![make_entry("a"), make_entry("b"), make_entry("c")]);
        app.selected = 2;
        app.move_up();
        assert_eq!(app.selected, 1);
    }

    #[test]
    fn test_move_up_wraps() {
        let mut app = IndexApp::new(vec![make_entry("a"), make_entry("b"), make_entry("c")]);
        app.move_up();
        assert_eq!(app.selected, 2);
    }

    #[test]
    fn test_move_on_empty_does_nothing() {
        let mut app = IndexApp::new(vec![]);
        app.move_up();
        app.move_down();
        assert_eq!(app.selected, 0);
    }

    #[test]
    fn test_run_empty_db_returns_ok() {
        let db = crate::db::Database::open_in_memory().unwrap();
        // No clusters in DB — run should return Ok without launching TUI
        let result = run(&db);
        assert!(result.is_ok());
    }
}
