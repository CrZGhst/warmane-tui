use crate::api::{check_reachability, get_latest_news, get_warmane_status_and_uptime};
use crate::event::{start_event_listener, Event};
use crate::http_client::build_client;
use crossterm::event::KeyCode;
use ratatui::backend::CrosstermBackend;
use ratatui::widgets::ListState;
use ratatui::Terminal;
use std::io::Stdout;
use std::time::Instant;
use std::{collections::HashMap, error::Error, time::Duration};

const WARMANE_WEBSITE_URL: &str = "https://www.warmane.com";

/// Struktur für den Datentransfer zwischen Hintergrund-Task und UI
pub struct UpdatePayload {
    pub warmane_res: Result<(Vec<RealmStatus>, Vec<RealmStatistics>), String>,
    pub news_res: Result<Vec<(String, String)>, String>,
    pub logon_up: bool,
}

#[derive(Debug, Clone, Default)]
pub struct RealmStatus {
    pub name: String,
    pub online_players: u32,
    pub status: String,
}

#[derive(Debug, Clone, Default)]
pub struct RealmStatistics {
    pub name: String,
    pub alliance: u32,
    pub horde: u32,
    pub uptime: String,
    pub latency: String,
}

pub struct App {
    pub client: reqwest::Client,
    pub realm_statuses: Vec<RealmStatus>,
    pub realm_statistics: Vec<RealmStatistics>,
    pub latest_news: Vec<(String, String)>,
    pub news_state: ListState,
    pub player_deltas: HashMap<String, i32>,
    pub prev_players: HashMap<String, u32>,
    pub should_quit: bool,
    pub last_update: Instant,
    pub last_error: Option<String>,
    pub is_loading: bool,
    pub logon_up: bool,
}

impl App {
    pub fn new() -> Result<App, Box<dyn Error>> {
        Ok(App {
            client: build_client()?,
            realm_statuses: vec![],
            realm_statistics: vec![],
            latest_news: vec![],
            news_state: ListState::default(),
            player_deltas: HashMap::new(),
            prev_players: HashMap::new(),
            should_quit: false,
            last_update: Instant::now(),
            last_error: None,
            is_loading: false,
            logon_up: false,
        })
    }

    pub async fn run(
        &mut self,
        terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    ) -> Result<(), Box<dyn Error>> {
        let tick_rate = Duration::from_secs(10);
        let mut rx = start_event_listener(tick_rate);
        let (update_tx, mut update_rx) = tokio::sync::mpsc::channel::<UpdatePayload>(1);

        self.trigger_update(update_tx.clone());

        loop {
            terminal.draw(|f| crate::ui::render(f, self))?; // self ist hier bereits &mut App

            tokio::select! {
                Some(event) = rx.recv() => {
                    match event {
                        Event::Input(code) => {
                            if code == KeyCode::Char('q') { self.should_quit = true; }
                            if code == KeyCode::Down { self.next_news(); }
                            if code == KeyCode::Up { self.previous_news(); }
                        }
                        Event::Tick => {
                            self.trigger_update(update_tx.clone());
                        }
                    }
                }
                Some(payload) = update_rx.recv() => {
                    self.apply_update(payload);
                }
            }

            if self.should_quit {
                break;
            }
        }
        Ok(())
    }

    pub fn next_news(&mut self) {
        if self.latest_news.is_empty() {
            return;
        }
        let i = match self.news_state.selected() {
            Some(i) => {
                if i >= self.latest_news.len() - 1 { 0 } else { i + 1 }
            }
            None => 0,
        };
        self.news_state.select(Some(i));
    }

    pub fn previous_news(&mut self) {
        if self.latest_news.is_empty() {
            return;
        }
        let i = match self.news_state.selected() {
            Some(i) => {
                if i == 0 { self.latest_news.len() - 1 } else { i - 1 }
            }
            None => 0,
        };
        self.news_state.select(Some(i));
    }

    fn trigger_update(&mut self, tx: tokio::sync::mpsc::Sender<UpdatePayload>) {
        if self.is_loading { return; }
        self.is_loading = true;
        let client_clone = self.client.clone();
        
        tokio::spawn(async move {
            let (warmane_res, news_res, reach_res) = tokio::join!(
                get_warmane_status_and_uptime(&client_clone, WARMANE_WEBSITE_URL),
                get_latest_news(&client_clone, WARMANE_WEBSITE_URL),
                check_reachability("145.239.161.30", 8091)
            );

            let payload = UpdatePayload {
                warmane_res: warmane_res.map_err(|e| e.to_string()),
                news_res: news_res.map_err(|e| e.to_string()),
                logon_up: reach_res,
            };
            let _ = tx.send(payload).await;
        });
    }

    fn apply_update(&mut self, payload: UpdatePayload) {
        self.is_loading = false;
        self.logon_up = payload.logon_up;
        self.last_error = None;

        if let Ok((statuses, stats)) = payload.warmane_res {
            for realm in &statuses {
                if let Some(&prev) = self.prev_players.get(&realm.name) {
                    let delta = realm.online_players as i32 - prev as i32;
                    self.player_deltas.insert(realm.name.clone(), delta);
                }
                self.prev_players.insert(realm.name.clone(), realm.online_players);
            }
            self.realm_statuses = statuses;
            self.realm_statistics = stats;
        }

        if let Ok(news) = payload.news_res {
            self.latest_news = news;
            if self.news_state.selected().is_none() && !self.latest_news.is_empty() {
                self.news_state.select(Some(0));
            }
        }

        self.last_update = Instant::now();
    }
}
