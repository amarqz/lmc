use crate::db::CommandRecord;

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
}
