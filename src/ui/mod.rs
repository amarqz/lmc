use crate::retrieval::{format_relative_time, App};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
};

pub fn draw(frame: &mut Frame, app: &App) {
    let now_secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(1),
            Constraint::Length(1),
        ])
        .split(frame.area());

    // --- Title block ---
    let subtitle = if app.tags.is_empty() {
        String::new()
    } else {
        let relative = app
            .last_used
            .map(|t| format_relative_time(now_secs, t))
            .unwrap_or_default();
        if relative.is_empty() {
            app.tags.join(", ")
        } else {
            format!("{} · {}", app.tags.join(", "), relative)
        }
    };

    let title_widget = Paragraph::new(vec![
        Line::from(Span::styled(
            app.alias.clone(),
            Style::default()
                .fg(Color::Blue)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(
            subtitle,
            Style::default().fg(Color::DarkGray),
        )),
    ])
    .block(Block::default().borders(Borders::NONE));
    frame.render_widget(title_widget, chunks[0]);

    // --- Command list ---
    let terminal_width = chunks[1].width as usize;
    let items: Vec<ListItem> = app
        .commands
        .iter()
        .enumerate()
        .map(|(i, cmd)| {
            let number_str = format!("{}  ", i + 1);
            // Truncate command if it would overflow terminal width
            let prefix_len = 4 + number_str.len(); // "│ " + number
            let max_cmd_len = terminal_width.saturating_sub(prefix_len);
            let display_cmd = if cmd.cmd.len() > max_cmd_len && max_cmd_len > 3 {
                format!("{}…", &cmd.cmd[..max_cmd_len - 1])
            } else {
                cmd.cmd.clone()
            };

            let (line, style) = if i == app.selected {
                (
                    Line::from(vec![
                        Span::styled("│ ", Style::default().fg(Color::Blue)),
                        Span::styled(
                            number_str,
                            Style::default().fg(Color::Blue),
                        ),
                        Span::styled(display_cmd, Style::default()),
                    ]),
                    Style::default().bg(Color::Rgb(49, 50, 68)),
                )
            } else {
                (
                    Line::from(vec![
                        Span::raw("  "),
                        Span::styled(
                            number_str,
                            Style::default().fg(Color::DarkGray),
                        ),
                        Span::raw(display_cmd),
                    ]),
                    Style::default(),
                )
            };
            ListItem::new(line).style(style)
        })
        .collect();

    let mut list_state = ListState::default();
    list_state.select(Some(app.selected));

    let list = List::new(items).block(Block::default().borders(Borders::NONE));
    frame.render_stateful_widget(list, chunks[1], &mut list_state);

    // --- Status bar ---
    let status = Paragraph::new(Line::from(Span::styled(
        "↑↓/jk navigate · Enter copy · 1–9 jump · q quit",
        Style::default().fg(Color::DarkGray),
    )));
    frame.render_widget(status, chunks[2]);
}
