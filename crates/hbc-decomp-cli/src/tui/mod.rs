use std::io;

use crossterm::terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen};
use crossterm::execute;
use ratatui::Terminal;
use hbc_decomp::{BytecodeFile, BytecodeFormat};

pub mod app;
pub mod events;
pub mod formatting;
pub mod ui;

use app::App;
use events::run_loop;

pub fn run_tui(
    file: BytecodeFile, format: BytecodeFormat, path: String, 
    diff_target: Option<(BytecodeFile, BytecodeFormat, String)>
) -> io::Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;

    let backend = ratatui::backend::CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    let mut app = App::new(file, format, path, diff_target);

    let result = run_loop(&mut terminal, &mut app);

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    result
}
