use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState};

use crate::app::{App, ModalState};
use crate::ui::palette;

pub fn draw_file_popup(f: &mut Frame, app: &App) {
    let ModalState::FileAutocomplete {
        selected,
        entries,
        loading,
        ..
    } = &app.modal
    else {
        return;
    };

    let area = f.area();
    // Height: cap at 10 rows of entries so the popup doesn't eat the
    // whole screen, but also never allocate more rows than we have.
    let max_items = entries.len().min(10) as u16;
    let height = (max_items + 2).max(3); // at least 3 for "Scanning…" state
    let width = area.width.min(60);
    let y = area.height.saturating_sub(height + 4); // above input
    let x = 2;
    let rect = Rect::new(x, y, width, height);

    f.render_widget(Clear, rect);

    let title = if *loading { " Scanning… " } else { " Files " };
    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(palette::ACCENT_ASSISTANT));
    let inner = block.inner(rect);
    f.render_widget(block, rect);

    let items: Vec<ListItem> = entries
        .iter()
        .map(|path| ListItem::new(format!(" {}", path)))
        .collect();

    let list = List::new(items).highlight_style(
        Style::default()
            .fg(Color::Black)
            .bg(palette::ACCENT_ASSISTANT),
    );
    let mut state = ListState::default().with_selected(Some(*selected));
    f.render_stateful_widget(list, inner, &mut state);
}
