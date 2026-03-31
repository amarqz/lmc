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

pub fn run(_db: &Database) -> Result<()> {
    todo!()
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
}
