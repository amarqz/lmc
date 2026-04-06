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
    let relative = app
        .last_used
        .map(|t| format_relative_time(now_secs, t))
        .unwrap_or_default();

    let subtitle = match (app.tags.is_empty(), relative.is_empty()) {
        (true, true) => String::new(),
        (true, false) => relative,
        (false, true) => app.tags.join(", "),
        (false, false) => format!("{} · {}", app.tags.join(", "), relative),
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
            let prefix_len = 4 + number_str.len(); // 2-char gutter + 2-char space before number
            let max_cmd_len = terminal_width.saturating_sub(prefix_len);
            let display_cmd = if cmd.cmd.chars().count() > max_cmd_len && max_cmd_len > 3 {
                let truncate_at = max_cmd_len - 1;
                let byte_pos = cmd.cmd
                    .char_indices()
                    .nth(truncate_at)
                    .map(|(i, _)| i)
                    .unwrap_or(cmd.cmd.len());
                format!("{}…", &cmd.cmd[..byte_pos])
            } else {
                cmd.cmd.clone()
            };

            let is_highlighted = i == app.selected;
            let is_selected = app.selected_items.contains(&i);

            let (line, style) = match (is_highlighted, is_selected) {
                (true, true) => (
                    // highlighted + selected: blue bullet + blue number
                    Line::from(vec![
                        Span::styled("● ", Style::default().fg(Color::Blue)),
                        Span::styled(number_str, Style::default().fg(Color::Blue)),
                        Span::styled(display_cmd, Style::default()),
                    ]),
                    Style::default().bg(Color::Rgb(49, 50, 68)),
                ),
                (true, false) => (
                    // highlighted only: blue bar + blue number
                    Line::from(vec![
                        Span::styled("│ ", Style::default().fg(Color::Blue)),
                        Span::styled(number_str, Style::default().fg(Color::Blue)),
                        Span::styled(display_cmd, Style::default()),
                    ]),
                    Style::default().bg(Color::Rgb(49, 50, 68)),
                ),
                (false, true) => (
                    // selected only: yellow bullet + normal number
                    Line::from(vec![
                        Span::styled("● ", Style::default().fg(Color::Yellow)),
                        Span::styled(number_str, Style::default().fg(Color::DarkGray)),
                        Span::raw(display_cmd),
                    ]),
                    Style::default(),
                ),
                (false, false) => (
                    // normal: empty gutter
                    Line::from(vec![
                        Span::raw("  "),
                        Span::styled(number_str, Style::default().fg(Color::DarkGray)),
                        Span::raw(display_cmd),
                    ]),
                    Style::default(),
                ),
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
        "↑↓/jk navigate · Enter copy · Space select · 1–9 jump · q quit",
        Style::default().fg(Color::DarkGray),
    )));
    frame.render_widget(status, chunks[2]);
}

pub fn draw_index(frame: &mut Frame, app: &crate::index::IndexApp) {
    let now_secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // column headers
            Constraint::Min(1),    // entry list
            Constraint::Length(1), // status bar
        ])
        .split(frame.area());

    let terminal_width = frame.area().width as usize;

    // Fixed column widths
    let gutter_w: usize = 2;
    let last_used_w: usize = 14;
    let cmds_w: usize = 4;
    let gaps: usize = 4 * 2; // four 2-char gaps (after gutter, alias, last_used, cmds)
    let alias_w = terminal_width
        .saturating_sub(gutter_w + last_used_w + cmds_w + gaps)
        .saturating_sub(10) // leave space for tags
        .max(1)
        .min(35);
    let tags_w = terminal_width
        .saturating_sub(gutter_w + alias_w + last_used_w + cmds_w + gaps);

    // --- Column headers ---
    let header = Line::from(vec![
        Span::raw("  "),
        Span::styled(
            pad_right("alias", alias_w),
            Style::default().fg(Color::DarkGray),
        ),
        Span::raw("  "),
        Span::styled(
            pad_right("last used", last_used_w),
            Style::default().fg(Color::DarkGray),
        ),
        Span::raw("  "),
        Span::styled(
            pad_left("cmds", cmds_w),
            Style::default().fg(Color::DarkGray),
        ),
        Span::raw("  "),
        Span::styled("tags", Style::default().fg(Color::DarkGray)),
    ]);
    frame.render_widget(Paragraph::new(header).block(Block::default().borders(Borders::NONE)), chunks[0]);

    // --- Entry list ---
    let items: Vec<ListItem> = app
        .entries
        .iter()
        .enumerate()
        .map(|(i, entry)| {
            let is_highlighted = i == app.selected;

            let alias_display = pad_right(&entry.alias, alias_w);
            let last_used_str = entry
                .last_used
                .map(|t| crate::retrieval::format_relative_time(now_secs, t))
                .unwrap_or_else(|| "never".to_string());
            let last_used_display = pad_right(&last_used_str, last_used_w);
            let cmds_display = pad_left(&entry.command_count.to_string(), cmds_w);
            let tags_str = entry.tags.join(", ");
            let tags_display = if tags_w > 0 {
                truncate_str(&tags_str, tags_w)
            } else {
                String::new()
            };

            let (gutter, gutter_style, row_style) = if is_highlighted {
                (
                    "│ ",
                    Style::default().fg(Color::Blue),
                    Style::default().bg(Color::Rgb(49, 50, 68)),
                )
            } else {
                ("  ", Style::default(), Style::default())
            };

            let line = Line::from(vec![
                Span::styled(gutter, gutter_style),
                Span::styled(
                    alias_display,
                    if is_highlighted {
                        Style::default().fg(Color::Blue)
                    } else {
                        Style::default()
                    },
                ),
                Span::raw("  "),
                Span::styled(last_used_display, Style::default().fg(Color::DarkGray)),
                Span::raw("  "),
                Span::styled(cmds_display, Style::default().fg(Color::DarkGray)),
                Span::raw("  "),
                Span::raw(tags_display),
            ]);

            ListItem::new(line).style(row_style)
        })
        .collect();

    let mut list_state = ListState::default();
    list_state.select(Some(app.selected));
    let list = List::new(items).block(Block::default().borders(Borders::NONE));
    frame.render_stateful_widget(list, chunks[1], &mut list_state);

    // --- Status bar ---
    let status = Paragraph::new(Line::from(Span::styled(
        "↑↓/jk navigate · Enter open · q quit",
        Style::default().fg(Color::DarkGray),
    )));
    frame.render_widget(status, chunks[2]);
}

pub fn draw_refine(frame: &mut Frame, app: &crate::refine::RefineApp) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // alias title
            Constraint::Length(1), // blank separator
            Constraint::Min(1),    // command list
            Constraint::Length(1), // tags line
            Constraint::Length(1), // status bar
        ])
        .split(frame.area());

    // Title
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            app.alias.clone(),
            Style::default().fg(Color::Blue).add_modifier(Modifier::BOLD),
        )))
        .block(Block::default().borders(Borders::NONE)),
        chunks[0],
    );

    // chunks[1] is blank — nothing rendered

    // Command list
    let terminal_width = chunks[2].width as usize;
    let items: Vec<ListItem> = app
        .commands
        .iter()
        .enumerate()
        .map(|(i, cmd)| {
            let number_str = format!("{}  ", i + 1);
            let prefix_len = 4 + number_str.len();
            let max_cmd_len = terminal_width.saturating_sub(prefix_len);
            let display_cmd = truncate_str(&cmd.cmd, max_cmd_len);

            let is_highlighted = i == app.selected;
            let (line, style) = if is_highlighted {
                (
                    Line::from(vec![
                        Span::styled("│ ", Style::default().fg(Color::Blue)),
                        Span::styled(number_str, Style::default().fg(Color::Blue)),
                        Span::styled(display_cmd, Style::default()),
                    ]),
                    Style::default().bg(Color::Rgb(49, 50, 68)),
                )
            } else {
                (
                    Line::from(vec![
                        Span::raw("  "),
                        Span::styled(number_str, Style::default().fg(Color::DarkGray)),
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
    frame.render_stateful_widget(list, chunks[2], &mut list_state);

    // Tags line
    let tags_text = if app.tags.is_empty() {
        "tags: none".to_string()
    } else {
        format!("tags: {}", app.tags.join(", "))
    };
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            tags_text,
            Style::default().fg(Color::DarkGray),
        ))),
        chunks[3],
    );

    // Status bar — hint changes when no commands remain
    let status_msg = if !app.can_confirm() {
        "no commands left · u undo · q quit"
    } else {
        "↑↓/jk navigate · d delete · s split · u undo · Enter confirm · q quit"
    };
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            status_msg,
            Style::default().fg(Color::DarkGray),
        ))),
        chunks[4],
    );
}

fn truncate_str(s: &str, max_width: usize) -> String {
    if max_width == 0 {
        return String::new();
    }
    let count = s.chars().count();
    if count <= max_width {
        s.to_string()
    } else if max_width == 1 {
        "…".to_string()
    } else {
        let byte_pos = s
            .char_indices()
            .nth(max_width - 1)
            .map(|(i, _)| i)
            .unwrap_or(s.len());
        format!("{}…", &s[..byte_pos])
    }
}

fn pad_right(s: &str, width: usize) -> String {
    let count = s.chars().count();
    if count >= width {
        truncate_str(s, width)
    } else {
        format!("{}{}", s, " ".repeat(width - count))
    }
}

fn pad_left(s: &str, width: usize) -> String {
    let count = s.chars().count();
    if count >= width {
        truncate_str(s, width)
    } else {
        format!("{}{}", " ".repeat(width - count), s)
    }
}
