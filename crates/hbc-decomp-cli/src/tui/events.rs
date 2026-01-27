use std::io::{self, Stdout};
use std::time::Duration;

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use ratatui::Terminal;

use super::app::{App, ViewMode};
use super::ui::draw_ui;

pub fn run_loop(terminal: &mut Terminal<ratatui::backend::CrosstermBackend<Stdout>>, app: &mut App) -> io::Result<()> {
    loop {
        terminal.draw(|frame| draw_ui(frame, app))?;

        if event::poll(Duration::from_millis(250))? {
            if let Event::Key(key) = event::read()? {
                if handle_key(app, key) {
                    break;
                }
            }
        }
    }

    Ok(())
}

fn handle_key(app: &mut App, key: KeyEvent) -> bool {
    let has_diff = app.file2.is_some();
    match key.code {
        KeyCode::Char('q') => return true,
        KeyCode::Up | KeyCode::Char('k') => {
            if app.selected > 0 {
                app.set_selected(app.selected - 1);
            }
        }
        KeyCode::Down | KeyCode::Char('j') => {
            if app.selected + 1 < app.file.function_headers.len() {
                app.set_selected(app.selected + 1);
            }
        }
        KeyCode::PageUp => {
            app.scroll = app.scroll.saturating_sub(10);
        }
        KeyCode::PageDown => {
            app.scroll = app.scroll.saturating_add(10);
        }
        KeyCode::Home => app.scroll = 0,
        KeyCode::End => app.scroll = u16::MAX,
        KeyCode::Tab => app.set_view(app.view.next(has_diff)),
        KeyCode::Char('1') => app.set_view(ViewMode::Disasm),
        KeyCode::Char('2') => app.set_view(ViewMode::Decompile),
        KeyCode::Char('3') => app.set_view(ViewMode::Info),
        KeyCode::Char('4') => if has_diff { app.set_view(ViewMode::Diff) },
        KeyCode::Char('v') => if app.view == ViewMode::Diff { app.toggle_diff_kind() },
        _ => {}
    }

    if key.modifiers.contains(KeyModifiers::CONTROL) {
        match key.code {
            KeyCode::Char('n') => {
                if app.selected + 1 < app.file.function_headers.len() {
                    app.set_selected(app.selected + 1);
                }
            }
            KeyCode::Char('p') => {
                if app.selected > 0 {
                    app.set_selected(app.selected - 1);
                }
            }
            _ => {}
        }
    }

    false
}
