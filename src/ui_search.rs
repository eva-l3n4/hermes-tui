use ratatui::prelude::*;
use ratatui::widgets::Paragraph;

use crate::app::{App, ModalState};
use crate::ui::palette;

pub fn draw_reverse_search(f: &mut Frame, app: &App) {
    let ModalState::ReverseSearch {
        query,
        match_index,
        ..
    } = &app.modal
    else {
        return;
    };

    let area = f.area();
    let bar_area = Rect::new(
        area.x,
        area.y.saturating_add(area.height.saturating_sub(2)),
        area.width,
        1,
    );

    let preview = match_index
        .and_then(|i| app.input_history.get(i))
        .map(String::as_str)
        .unwrap_or("");

    let line = Line::from(vec![
        Span::styled(
            "(reverse-i-search)'",
            Style::default().fg(palette::DIM),
        ),
        Span::styled(query.as_str(), Style::default().fg(palette::ACCENT_ASSISTANT)),
        Span::styled("': ", Style::default().fg(palette::DIM)),
        Span::raw(preview),
    ]);

    f.render_widget(Paragraph::new(line), bar_area);
}
