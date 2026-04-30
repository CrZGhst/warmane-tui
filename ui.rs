use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Span, Text},
    widgets::{Block, Borders, Cell, List, ListItem, Paragraph, Row, Table},
    Frame,
};

use crate::app::App;

pub fn render(f: &mut Frame, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(
            [
                Constraint::Length(3),
                Constraint::Min(0),
                Constraint::Length(1),
            ]
            .as_ref(),
        )
        .split(f.size());

    render_title(f, chunks[0]);
    render_main_content(f, chunks[1], app);
    render_footer(f, chunks[2], app);
}

fn render_title(f: &mut Frame, area: Rect) {
    let title = Paragraph::new(Span::styled(
        "Warmane TUI Dashboard (Optimized by CrZ)",
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    ))
    .block(Block::default().borders(Borders::ALL))
    .alignment(ratatui::layout::Alignment::Center);
    f.render_widget(title, area);
}

fn render_main_content(f: &mut Frame, area: Rect, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)].as_ref())
        .split(area);

    let top_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)].as_ref())
        .split(chunks[0]);

    render_server_status(f, top_chunks[0], app);
    render_realm_statistics(f, top_chunks[1], app);
    render_latest_news(f, chunks[1], app);
}

fn render_server_status(f: &mut Frame, area: Rect, app: &App) {
    let header = Row::new(vec!["St.", "Realm", "Players", "Delta"])
        .style(Style::default().fg(Color::Yellow))
        .bottom_margin(1);

    let rows = app.realm_statuses.iter().map(|s| {
        let delta = app.player_deltas.get(&s.name).cloned().unwrap_or(0);
        let delta_color = if delta > 0 {
            Color::Green
        } else if delta < 0 {
            Color::Red
        } else {
            Color::Gray
        };
        let delta_str = if delta >= 0 {
            format!("+{}", delta)
        } else {
            delta.to_string()
        };

        let status_icon = "●";

        // Vereinfachte Logik:
        // Wenn der Logon-Server (145.239.161.30) erreichbar ist -> GRÜN.
        // Wenn nicht erreichbar -> ROT.
        let status_color = if app.logon_up {
            Color::Green
        } else {
            Color::Red
        };

        Row::new(vec![
            Cell::from(Span::styled(status_icon, Style::default().fg(status_color))),
            Cell::from(s.name.as_str()),
            Cell::from(s.online_players.to_string()),
            Cell::from(Span::styled(delta_str, Style::default().fg(delta_color))),
        ])
    });

    let table = Table::new(
        rows,
        [
            Constraint::Length(3),
            Constraint::Percentage(37),
            Constraint::Percentage(30),
            Constraint::Percentage(30),
        ],
    )
    .header(header)
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title("Server Status"),
    );

    f.render_widget(table, area);
}

fn render_realm_statistics(f: &mut Frame, area: Rect, app: &App) {
    let rows = app.realm_statistics.iter().map(|s| {
        let a_str = format!("{}%", s.alliance);
        let h_str = format!("{}%", s.horde);
        Row::new(vec![
            Cell::from(s.name.as_str()),
            Cell::from(Span::styled(a_str, Style::default().fg(Color::Blue))),
            Cell::from(Span::styled(h_str, Style::default().fg(Color::Red))),
            Cell::from(s.uptime.as_str()),
            Cell::from(s.latency.as_str()),
        ])
    });

    let table = Table::new(
        rows,
        [
            Constraint::Percentage(20),
            Constraint::Percentage(15),
            Constraint::Percentage(15),
            Constraint::Percentage(35),
            Constraint::Percentage(15),
        ],
    )
    .header(
        Row::new(vec!["Realm", "Alliance", "Horde", "Uptime", "Latency"])
            .style(Style::default().fg(Color::Yellow)),
    )
    .block(Block::default().borders(Borders::ALL).title("Population Statistics (A/H Balance)"));

    f.render_widget(table, area);
}

fn render_latest_news(f: &mut Frame, area: Rect, app: &mut App) {
    let news: Vec<ListItem> = app
        .latest_news
        .iter()
        .map(|(t, _)| {
            let text_with_highlights = create_highlighted_text(t);
            ListItem::new(text_with_highlights)
        })
        .collect();

    let list = List::new(news)
        .block(Block::default().borders(Borders::ALL).title("Latest News (Scroll with Arrow Keys)"))
        .highlight_style(Style::default().bg(Color::DarkGray).add_modifier(Modifier::BOLD))
        .highlight_symbol(">> ")
        .repeat_highlight_symbol(true);

    f.render_stateful_widget(list, area, &mut app.news_state);
}

fn create_highlighted_text(text: &str) -> Text<'_> {
    // Kritische Keywords (rot - CRITICAL)
    let critical_keywords = vec![
        "maintenance",
        "offline",
        "all realms will be taken offline",
        "down",
        "shutdown",
        "unavailable",
    ];

    // Wichtige Keywords (gelb/orange - WARNING)
    let warning_keywords = vec![
        "gold squish",
        "nerf",
        "exploit",
        "wipe",
        "reset",
        "rollback",
        "issue",
    ];

    let lowercase_text = text.to_lowercase();
    let mut has_critical = false;
    let mut has_warning = false;

    // Prüfe ob critical Keywords vorhanden sind
    for keyword in &critical_keywords {
        if lowercase_text.contains(keyword) {
            has_critical = true;
            break;
        }
    }

    // Prüfe ob warning Keywords vorhanden sind
    if !has_critical {
        for keyword in &warning_keywords {
            if lowercase_text.contains(keyword) {
                has_warning = true;
                break;
            }
        }
    }

    // Highlight gesamte News basierend auf Priorität
    if has_critical {
        Text::styled(text, Style::default().fg(Color::Red).add_modifier(Modifier::BOLD))
    } else if has_warning {
        Text::styled(text, Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD))
    } else {
        Text::from(text)
    }
}

fn render_footer(f: &mut Frame, area: Rect, app: &App) {
    let (msg, style) = if let Some(err) = &app.last_error {
        (err.clone(), Style::default().fg(Color::Red))
    } else {
        (
            format!(
                "Last update: {:?} | Press 'q' to quit",
                app.last_update.elapsed()
            ),
            Style::default(),
        )
    };

    f.render_widget(Paragraph::new(Span::styled(msg, style)), area);
}
