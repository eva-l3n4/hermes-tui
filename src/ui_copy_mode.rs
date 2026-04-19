use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, List, ListItem, ListState, Paragraph},
};

use crate::app::{App, CopyScope, Role};

pub fn draw_copy_mode(f: &mut Frame, app: &App, selected: usize, scope: &CopyScope) {
    let area = centered_rect(80, 70, f.area());
    f.render_widget(Clear, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // title
            Constraint::Min(5),    // message list
            Constraint::Length(1), // hints
        ])
        .split(area);

    let scope_label = match scope {
        CopyScope::Message => "message",
        CopyScope::CodeBlock => "code block",
    };
    let title = Paragraph::new(Line::from(vec![
        Span::styled(
            "  Copy ",
            Style::default()
                .fg(Color::Magenta)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("({})", scope_label),
            Style::default().fg(Color::DarkGray),
        ),
    ]))
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded),
    );
    f.render_widget(title, chunks[0]);

    let items: Vec<ListItem> = app
        .messages
        .iter()
        .map(|m| {
            let role_icon = match m.role {
                Role::User => "›",
                Role::Assistant => "◆",
                Role::System => "·",
                Role::Tool => "▸",
                Role::Thought => "◌",
            };
            let preview: String = m
                .content
                .lines()
                .find(|l| !l.trim().is_empty())
                .unwrap_or("")
                .chars()
                .filter(|&c| c != '\0')
                .take(80)
                .collect();
            let nlines = m.content.lines().count();
            let line = Line::from(vec![
                Span::styled(
                    format!(" {} ", role_icon),
                    Style::default().fg(Color::DarkGray),
                ),
                Span::raw(preview),
                Span::styled(
                    format!("  ({} lines)", nlines),
                    Style::default().fg(Color::DarkGray),
                ),
            ]);
            ListItem::new(line)
        })
        .collect();

    let mut state = ListState::default().with_selected(Some(selected));
    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded),
        )
        .highlight_style(
            Style::default()
                .bg(Color::Magenta)
                .fg(Color::Black)
                .add_modifier(Modifier::BOLD),
        );
    f.render_stateful_widget(list, chunks[1], &mut state);

    let hints = Paragraph::new(Line::from(vec![Span::styled(
        " ↑↓ select · enter copy · c toggle code/msg · esc cancel ",
        Style::default().fg(Color::DarkGray),
    )]));
    f.render_widget(hints, chunks[2]);
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}
