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
}
