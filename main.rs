use crossterm::{
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use log::LevelFilter;
use ratatui::{backend::CrosstermBackend, Terminal};
use std::{error::Error, io}; // Importiere LevelFilter für die Logger-Konfiguration

mod api;
mod app;
mod event;
mod http_client;
mod ui;

use crate::app::App;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // Initialisiere den Logger.
    // Standardmäßig werden Info-Level und höher protokolliert.
    // Die Ausgabe erfolgt auf stderr.
    env_logger::builder().filter_level(LevelFilter::Info).init();
    // setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // create app and run it
    let mut app = App::new()?;
    let res = app.run(&mut terminal).await;

    // restore terminal
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    if let Err(err) = res {
        eprintln!("Application error: {:?}", err);
    }

    Ok(())
}
