use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, BorderType, Borders, List, ListItem, ListState, Paragraph, Wrap};
use ratatui::Frame;

use super::app::{App, ViewMode};

pub fn draw_ui(frame: &mut Frame, app: &mut App) {
    let size = frame.size();
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(2), Constraint::Length(1)])
        .split(size);

    let diff_title = match app.diff_kind {
        ViewMode::Disasm => "Assembly",
        _ => "Source",
    };
    
    let header_text = if let Some(p2) = &app.path2 {
        if app.view == ViewMode::Diff {
             format!("Hermes Decompiler | Diff ({}) | {} vs {}", diff_title, app.path, p2)
        } else {
             format!("Hermes Decompiler | {} vs {}", app.path, p2)
        }
    } else {
        format!("Hermes Decompiler | {} | v{}", app.path, app.file.header.version)
    };

    let header = Paragraph::new(Line::from(vec![
        Span::styled(header_text, Style::default().add_modifier(Modifier::BOLD)),
    ]));
    frame.render_widget(header, layout[0]);

    let body = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(30), Constraint::Percentage(70)])
        .split(layout[1]);

    draw_function_list(frame, app, body[0]);
    
    // Check if in diff mode for split view content
    if app.view == ViewMode::Diff {
         let (left_content, right_content) = app.content();
         
         let split = Layout::default()
             .direction(Direction::Horizontal)
             .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
             .split(body[1]);
             
         draw_content_pane(frame, app, split[0], left_content, "Left (v1)");
         if let Some(right) = right_content {
              draw_content_pane(frame, app, split[1], right, "Right (v2)");
         }
    } else {
         let (content, _) = app.content();
         draw_content_pane(frame, app, body[1], content, app.view.title());
    }

    let footer = Paragraph::new("q quit | j/k move | Tab view | v toggle diff mode | 1/2/3/4 direct")
        .style(Style::default().fg(Color::DarkGray));
    frame.render_widget(footer, layout[2]);
}

fn draw_function_list(frame: &mut Frame, app: &mut App, area: Rect) {
    let items: Vec<ListItem> = app
        .function_names
        .iter()
        .enumerate()
        .map(|(idx, name)| {
            let label = format!("{idx:>4} {name}");
            // Check diff status if in Diff mode?
            if idx == app.selected {
                 ListItem::new(Line::from(Span::styled(label, Style::default().fg(Color::Black).bg(Color::Yellow))))
            } else {
                 ListItem::new(Line::from(Span::raw(label)))
            }
        })
        .collect();

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Plain)
                .title("Functions"),
        );

    let mut state = ListState::default();
    state.select(Some(app.selected));
    frame.render_stateful_widget(list, area, &mut state);
}

fn draw_content_pane(frame: &mut Frame, app: &mut App, area: Rect, content: Text<'static>, title: &str) {
    let max_scroll = content
        .lines
        .len()
        .saturating_sub(area.height as usize)
        .min(u16::MAX as usize) as u16;
        
    if app.scroll > max_scroll {
        app.scroll = max_scroll;
    }

    let paragraph = Paragraph::new(content)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Plain)
                .title(title),
        )
        .wrap(Wrap { trim: false })
        .scroll((app.scroll, 0));
    frame.render_widget(paragraph, area);
}
