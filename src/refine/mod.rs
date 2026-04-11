use crate::config::TagInferenceConfig;
use crate::db::CommandRecord;
use crate::tags::infer_tags_for_command;

const AUTO_REFINE_THRESHOLD: usize = 10;

pub enum RefineResult {
    Confirmed(Vec<CommandRecord>),
    Split(Vec<CommandRecord>, Vec<CommandRecord>),
    Cancelled,
}

pub struct RefineApp {
    pub alias: String,
    pub commands: Vec<CommandRecord>,
    pub selected: usize,
    pub tags: Vec<String>,
    undo_stack: Vec<(Vec<CommandRecord>, usize)>,
    config: TagInferenceConfig,
}

pub fn should_refine(commands: &[CommandRecord], force: bool) -> bool {
    if force {
        return true;
    }
    if commands.len() > AUTO_REFINE_THRESHOLD {
        return true;
    }
    let first_dir = commands.first().map(|c| c.directory.as_str());
    commands.iter().any(|c| Some(c.directory.as_str()) != first_dir)
}

impl RefineApp {
    pub fn new(alias: String, commands: Vec<CommandRecord>, config: TagInferenceConfig) -> Self {
        let tags = Self::compute_tags(&commands, &config);
        Self {
            alias,
            commands,
            selected: 0,
            tags,
            undo_stack: Vec::new(),
            config,
        }
    }

    fn compute_tags(commands: &[CommandRecord], config: &TagInferenceConfig) -> Vec<String> {
        let mut set = std::collections::HashSet::new();
        for cmd in commands {
            for tag in infer_tags_for_command(&cmd.cmd, config) {
                set.insert(tag);
            }
        }
        let mut v: Vec<String> = set.into_iter().collect();
        v.sort();
        v
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

    pub fn delete_selected(&mut self) {
        if self.commands.is_empty() {
            return;
        }
        self.undo_stack.push((self.commands.clone(), self.selected));
        self.commands.remove(self.selected);
        if self.selected >= self.commands.len() && !self.commands.is_empty() {
            self.selected = self.commands.len() - 1;
        }
        self.tags = Self::compute_tags(&self.commands, &self.config);
    }

    pub fn undo(&mut self) {
        if let Some((prev_commands, prev_selected)) = self.undo_stack.pop() {
            self.commands = prev_commands;
            self.selected = prev_selected;
            self.tags = Self::compute_tags(&self.commands, &self.config);
        }
    }

    /// Returns `Some((top, bottom))` where `top` is everything before the cursor
    /// and `bottom` starts at the cursor. Returns `None` if cursor is at 0 (nothing to split off).
    pub fn split(&self) -> Option<(Vec<CommandRecord>, Vec<CommandRecord>)> {
        if self.selected == 0 {
            return None;
        }
        let top = self.commands[..self.selected].to_vec();
        let bottom = self.commands[self.selected..].to_vec();
        Some((top, bottom))
    }

    pub fn can_confirm(&self) -> bool {
        !self.commands.is_empty()
    }
}

pub fn run(
    alias: &str,
    commands: Vec<CommandRecord>,
    config: TagInferenceConfig,
) -> anyhow::Result<RefineResult> {
    use crossterm::terminal::{disable_raw_mode, enable_raw_mode};

    let mut app = RefineApp::new(alias.to_string(), commands, config);
    enable_raw_mode()?;
    let result = run_tui_inner(&mut app);
    let _ = disable_raw_mode();
    result
}

fn run_tui_inner(app: &mut RefineApp) -> anyhow::Result<RefineResult> {
    use crossterm::{
        cursor::{MoveToColumn, MoveUp},
        event::{self, Event, KeyCode, KeyEventKind},
        execute,
        terminal::{size as terminal_size, Clear, ClearType},
    };
    use ratatui::{backend::CrosstermBackend, Terminal, TerminalOptions, Viewport};
    use std::io;

    // title(1) + blank(1) + commands + tags(1) + status(1) = commands.len() + 4
    // Height is fixed at TUI start. Deleting commands leaves blank lines at the
    // bottom, and the cleanup MoveUp uses this initial height. Same trade-off as
    // retrieval::run_tui_inner — acceptable for inline viewports.
    let desired = (app.commands.len() + 4) as u16;
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
        terminal.draw(|frame| crate::ui::draw_refine(frame, app))?;

        if let Event::Key(key) = event::read()? {
            if key.kind != KeyEventKind::Press {
                continue;
            }
            match key.code {
                KeyCode::Up | KeyCode::Char('k') => app.move_up(),
                KeyCode::Down | KeyCode::Char('j') => app.move_down(),
                KeyCode::Char('d') => app.delete_selected(),
                KeyCode::Char('u') => app.undo(),
                KeyCode::Char('s') => {
                    if let Some((top, bottom)) = app.split() {
                        break RefineResult::Split(top, bottom);
                    }
                    // selected == 0: no-op, stay in loop
                }
                KeyCode::Enter => {
                    if app.can_confirm() {
                        break RefineResult::Confirmed(app.commands.clone());
                    }
                    // empty list: no-op
                }
                KeyCode::Char('q') | KeyCode::Esc => break RefineResult::Cancelled,
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

    fn make_cmd(id: i64, cmd: &str, dir: &str) -> CommandRecord {
        CommandRecord {
            id: Some(id),
            cmd: cmd.to_string(),
            timestamp: 1000,
            directory: dir.to_string(),
            exit_code: Some(0),
            session_id: "s1".to_string(),
            shell: "zsh".to_string(),
            noisy: false,
        }
    }

    fn default_config() -> TagInferenceConfig {
        TagInferenceConfig { custom: vec![] }
    }

    #[test]
    fn test_new_initializes_correctly() {
        let cmds = vec![make_cmd(1, "kubectl get pods", "/p"), make_cmd(2, "helm list", "/p")];
        let app = RefineApp::new("k8s-debug".to_string(), cmds, default_config());
        assert_eq!(app.alias, "k8s-debug");
        assert_eq!(app.commands.len(), 2);
        assert_eq!(app.selected, 0);
        assert!(app.tags.contains(&"kubernetes".to_string()));
    }

    #[test]
    fn test_move_down_advances_selection() {
        let cmds = vec![make_cmd(1, "a", "/p"), make_cmd(2, "b", "/p")];
        let mut app = RefineApp::new("x".to_string(), cmds, default_config());
        app.move_down();
        assert_eq!(app.selected, 1);
    }

    #[test]
    fn test_move_down_wraps() {
        let cmds = vec![make_cmd(1, "a", "/p"), make_cmd(2, "b", "/p")];
        let mut app = RefineApp::new("x".to_string(), cmds, default_config());
        app.selected = 1;
        app.move_down();
        assert_eq!(app.selected, 0);
    }

    #[test]
    fn test_move_up_wraps() {
        let cmds = vec![make_cmd(1, "a", "/p"), make_cmd(2, "b", "/p")];
        let mut app = RefineApp::new("x".to_string(), cmds, default_config());
        app.move_up();
        assert_eq!(app.selected, 1);
    }

    #[test]
    fn test_delete_removes_command_and_pushes_undo() {
        let cmds = vec![make_cmd(1, "a", "/p"), make_cmd(2, "b", "/p"), make_cmd(3, "c", "/p")];
        let mut app = RefineApp::new("x".to_string(), cmds, default_config());
        app.selected = 1;
        app.delete_selected();
        assert_eq!(app.commands.len(), 2);
        assert_eq!(app.commands[0].cmd, "a");
        assert_eq!(app.commands[1].cmd, "c");
        assert_eq!(app.undo_stack.len(), 1);
    }

    #[test]
    fn test_delete_clamps_cursor_when_at_end() {
        let cmds = vec![make_cmd(1, "a", "/p"), make_cmd(2, "b", "/p")];
        let mut app = RefineApp::new("x".to_string(), cmds, default_config());
        app.selected = 1;
        app.delete_selected();
        assert_eq!(app.selected, 0);
    }

    #[test]
    fn test_delete_on_empty_does_nothing() {
        let mut app = RefineApp::new("x".to_string(), vec![], default_config());
        app.delete_selected();
        assert!(app.commands.is_empty());
        assert!(app.undo_stack.is_empty());
    }

    #[test]
    fn test_undo_restores_previous_state() {
        let cmds = vec![make_cmd(1, "a", "/p"), make_cmd(2, "b", "/p")];
        let mut app = RefineApp::new("x".to_string(), cmds, default_config());
        app.delete_selected();
        assert_eq!(app.commands.len(), 1);
        app.undo();
        assert_eq!(app.commands.len(), 2);
        assert!(app.undo_stack.is_empty());
    }

    #[test]
    fn test_undo_multiple_levels() {
        let cmds = vec![make_cmd(1, "a", "/p"), make_cmd(2, "b", "/p"), make_cmd(3, "c", "/p")];
        let mut app = RefineApp::new("x".to_string(), cmds, default_config());
        app.delete_selected(); // removes "a" → [b, c]
        app.delete_selected(); // removes "b" → [c]
        assert_eq!(app.commands.len(), 1);
        app.undo(); // → [b, c]
        assert_eq!(app.commands.len(), 2);
        app.undo(); // → [a, b, c]
        assert_eq!(app.commands.len(), 3);
    }

    #[test]
    fn test_undo_on_empty_stack_does_nothing() {
        let cmds = vec![make_cmd(1, "a", "/p")];
        let mut app = RefineApp::new("x".to_string(), cmds, default_config());
        app.undo(); // no-op
        assert_eq!(app.commands.len(), 1);
    }

    #[test]
    fn test_split_at_zero_returns_none() {
        let cmds = vec![make_cmd(1, "a", "/p"), make_cmd(2, "b", "/p")];
        let app = RefineApp::new("x".to_string(), cmds, default_config());
        assert!(app.split().is_none());
    }

    #[test]
    fn test_split_produces_correct_halves() {
        let cmds = vec![
            make_cmd(1, "a", "/p"),
            make_cmd(2, "b", "/p"),
            make_cmd(3, "c", "/p"),
        ];
        let mut app = RefineApp::new("x".to_string(), cmds, default_config());
        app.selected = 2;
        let (top, bottom) = app.split().unwrap();
        assert_eq!(top.len(), 2);
        assert_eq!(top[0].cmd, "a");
        assert_eq!(top[1].cmd, "b");
        assert_eq!(bottom.len(), 1);
        assert_eq!(bottom[0].cmd, "c");
    }

    #[test]
    fn test_can_confirm_false_when_empty() {
        let mut app = RefineApp::new("x".to_string(), vec![make_cmd(1, "a", "/p")], default_config());
        app.delete_selected();
        assert!(!app.can_confirm());
    }

    #[test]
    fn test_tags_updated_after_delete() {
        // Use commands with non-overlapping tags: cargo → rust, helm → kubernetes + helm
        let cmds = vec![
            make_cmd(1, "cargo build", "/p"),
            make_cmd(2, "helm list", "/p"),
        ];
        let mut app = RefineApp::new("x".to_string(), cmds, default_config());
        assert!(app.tags.contains(&"rust".to_string()));
        assert!(app.tags.contains(&"helm".to_string()));
        // Delete the cargo command; "rust" tag should disappear, helm tags remain
        app.delete_selected();
        assert!(!app.tags.contains(&"rust".to_string()));
        assert!(app.tags.contains(&"helm".to_string()));
    }

    #[test]
    fn test_should_refine_force_flag() {
        let cmds = vec![make_cmd(1, "a", "/p")];
        assert!(should_refine(&cmds, true));
    }

    #[test]
    fn test_should_refine_large_cluster() {
        let cmds: Vec<_> = (1..=11).map(|i| make_cmd(i, "cmd", "/p")).collect();
        assert!(should_refine(&cmds, false));
    }

    #[test]
    fn test_should_refine_multi_directory() {
        let cmds = vec![make_cmd(1, "a", "/p1"), make_cmd(2, "b", "/p2")];
        assert!(should_refine(&cmds, false));
    }

    #[test]
    fn test_should_not_refine_small_single_dir() {
        let cmds: Vec<_> = (1..=5).map(|i| make_cmd(i, "cmd", "/p")).collect();
        assert!(!should_refine(&cmds, false));
    }

    #[test]
    fn test_should_not_refine_exactly_threshold() {
        let cmds: Vec<_> = (1..=10).map(|i| make_cmd(i, "cmd", "/p")).collect();
        assert!(!should_refine(&cmds, false));
    }

    #[test]
    fn test_should_not_refine_empty_commands() {
        assert!(!should_refine(&[], false));
    }
}
