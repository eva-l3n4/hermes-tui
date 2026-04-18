use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

use crate::app::{App, ModalState};
use crate::ui::palette;

pub fn draw_effort_slider(f: &mut Frame, app: &App) {
    let level = match app.modal {
        ModalState::EffortSlider { level } => level,
        _ => return,
    };

    let area = f.area();
    let popup = centered_rect(40, 5, area);

    f.render_widget(Clear, popup);

    let block = Block::default()
        .title(" Effort ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(palette::ACCENT_ASSISTANT));

    let inner = block.inner(popup);
    f.render_widget(block, popup);

    let levels = ["low", "medium", "high"];
    let selected = level.min(2) as usize;

    let mut spans: Vec<Span> = Vec::new();
    for (idx, name) in levels.iter().enumerate() {
        if idx > 0 {
            spans.push(Span::styled(" ── ", Style::default().fg(palette::DIM)));
        }

        let is_selected = idx == selected;
        spans.push(Span::styled(
            if is_selected { "●" } else { "○" },
            if is_selected {
                Style::default()
                    .fg(palette::ACCENT_ASSISTANT)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(palette::DIM)
            },
        ));
        spans.push(Span::styled(
            format!(" {name} "),
            if is_selected {
                Style::default()
                    .fg(palette::ACCENT_ASSISTANT)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(palette::DIM)
            },
        ));
    }

    let slider = Line::from(spans).alignment(Alignment::Center);
    let hint = Line::styled(
        "←/→ select  Enter confirm  Esc cancel",
        Style::default().fg(palette::DIM),
    )
    .alignment(Alignment::Center);

    let text = Text::from(vec![slider, hint]);
    let paragraph = Paragraph::new(text).alignment(Alignment::Center);
    f.render_widget(paragraph, inner);
}

fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let x = area.x + area.width.saturating_sub(width) / 2;
    let y = area.y + area.height.saturating_sub(height) / 2;
    Rect::new(x, y, width.min(area.width), height.min(area.height))
}
