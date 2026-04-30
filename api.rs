use log::warn;
use reqwest::Client;
use scraper::{Html, Selector};
use std::error::Error;
use tokio::net::TcpStream;
use tokio::time::Duration;

use crate::app::{RealmStatus, RealmStatistics};

// ───────────────────────── HTTP ─────────────────────────

pub async fn fetch_with_retry(client: &Client, url: &str) -> Result<String, Box<dyn Error + Send + Sync>> {
    let mut last_err = None;
    for attempt in 1..=2 {
        match client.get(url).send().await {
            Ok(resp) => return Ok(resp.text().await?),
            Err(e) => {
                last_err = Some(e);
                tokio::time::sleep(Duration::from_secs(attempt)).await;
            }
        }
    }
    Err(last_err.unwrap().into())
}

pub async fn check_reachability(ip: &str, port: u16) -> bool {
    let addr = format!("{}:{}", ip, port);

    // 1. Versuch: TCP-Connect auf dem angegebenen Port mit kurzem Timeout
    match tokio::time::timeout(Duration::from_secs(2), TcpStream::connect(&addr)).await {
        Ok(Ok(_)) => {
            // TCP-Connect erfolgreich, Server ist auf diesem Port erreichbar
            return true;
        }
        Ok(Err(e)) => {
            // TCP-Connect fehlgeschlagen (z.B. Verbindung verweigert, Port nicht offen)
            warn!(
                "TCP connect to {}:{} failed: {}. Falling back to ping.",
                ip, port, e
            );
        }
        Err(_) => {
            // TCP-Connect Timeout
            warn!(
                "TCP connect to {}:{} timed out. Falling back to ping.",
                ip, port
            );
        }
    }

    // 2. Fallback: Echter ICMP-Ping via System-Kommando (1 Paket, 1 Sekunde Timeout)
    let mut cmd = tokio::process::Command::new("ping");
    
    #[cfg(not(target_os = "windows"))]
    {
        // Linux/macOS Argumente
        cmd.args(&["-c", "1", "-W", "1", ip]);
    }

    #[cfg(target_os = "windows")]
    {
        // Windows Argumente: -n für Anzahl, -w für Timeout in Millisekunden
        cmd.args(&["-n", "1", "-w", "1000", ip]);
    }

    let ping_check = cmd.stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .await;

    match ping_check {
        Ok(status) => {
            if !status.success() {
                warn!("Ping to {} failed.", ip);
            }
            status.success()
        }
        Err(e) => {
            warn!("Failed to execute ping command for {}: {}", ip, e);
            false
        }
    }
}

// ───────────────────────── API METHODS ─────────────────────────

pub async fn get_warmane_status_and_uptime(
    client: &Client,
    url: &str,
) -> Result<(Vec<RealmStatus>, Vec<RealmStatistics>), Box<dyn Error + Send + Sync>> {
    let html = fetch_with_retry(client, url).await?;
    let (statuses, _) = parse_warmane_status_html(&html)?;
    
    // Versuche Statistiken von der Info-Seite zu holen
    let mut stats = vec![];
    if let Ok(info_html) = fetch_with_retry(client, "https://www.warmane.com/information").await {
        stats = parse_statistics_from_html(&info_html).unwrap_or_default();
    }
    
    Ok((statuses, stats))
}

pub async fn get_latest_news(
    client: &Client,
    url: &str,
) -> Result<Vec<(String, String)>, Box<dyn Error + Send + Sync>> {
    let html = fetch_with_retry(client, url).await?;
    parse_news(&html)
}

// ───────────────────────── PARSERS ─────────────────────────

fn parse_statistics_from_html(html: &str) -> Result<Vec<RealmStatistics>, Box<dyn Error + Send + Sync>> {
    let doc = Html::parse_document(html);
    let mut stats = vec![];
    let mut realm_stats_map = std::collections::HashMap::new();

    // Extrahiere statdata aus den Script-Tags
    let script_selector = Selector::parse("script").map_err(|e| format!("{:?}", e))?;
    for script in doc.select(&script_selector) {
        let text = script.text().collect::<String>();
        if text.contains("var statdata") {
            for id in &["6", "7", "10", "14"] {
                // Wir suchen nach "ID: {" um Zeitstempel wie "16:03" zu ignorieren
                let id_search = format!("{}: {{", id);
                if let Some(id_pos) = text.find(&id_search) {
                    let block = &text[id_pos..];
                    let a_perc = extract_faction_value(block, "Alliance");
                    let h_perc = extract_faction_value(block, "Horde");
                    realm_stats_map.insert(id.to_string(), (a_perc, h_perc));
                }
            }
        }
    }

    // Extrahiere Uptime und Latency aus den Statistics-Divs
    let mut uptime_latency_map = std::collections::HashMap::new();
    let stats_selector = Selector::parse(".wm-ui-statistics").map_err(|e| format!("{:?}", e))?;
    let stats_div_selector = Selector::parse(".stats").map_err(|e| format!("{:?}", e))?;
    let div_selector = Selector::parse("div").map_err(|e| format!("{:?}", e))?;
    
    for stat_div in doc.select(&stats_selector) {
        // Extrahiere Realm-Namen aus dem Span
        let span_selector = Selector::parse("span").map_err(|e| format!("{:?}", e))?;
        let realm_name = stat_div.select(&span_selector)
            .next()
            .map(|s| {
                let text = s.text().collect::<String>();
                // Entferne die x-Notation (z.B. "x5", "x1")
                text.split_whitespace().next().unwrap_or("").to_lowercase()
            })
            .unwrap_or_default();

        // Finde die .stats Div innerhalb dieser Statistik-Section
        if let Some(stats_block) = stat_div.select(&stats_div_selector).next() {
            let divs: Vec<String> = stats_block.select(&div_selector)
                .map(|d| d.text().collect::<String>())
                .collect();

            if !realm_name.is_empty() && divs.len() >= 2 {
                uptime_latency_map.insert(realm_name, (divs[0].clone(), divs[1].clone()));
            }
        }
    }

    // Erstelle Statistiken basierend auf den gescrapten Daten
    let realm_mapping = vec![
        ("onyxia", "14"),
        ("lordaeron", "6"),
        ("icecrown", "7"),
        ("blackrock", "10"),
    ];

    for (name, id) in realm_mapping {
        if let Some(&(a, h)) = realm_stats_map.get(id) {
            let (uptime, latency) = uptime_latency_map.get(name)
                .map(|(u, l)| (u.clone(), l.clone()))
                .unwrap_or_else(|| ("Unknown".to_string(), "Unknown".to_string()));
            
            stats.push(RealmStatistics {
                name: name.to_uppercase(),
                alliance: a,
                horde: h,
                uptime,
                latency,
            });
        }
    }

    Ok(stats)
}

fn parse_warmane_status_html(
    html: &str,
) -> Result<(Vec<RealmStatus>, Vec<RealmStatistics>), Box<dyn Error + Send + Sync>> {
    let doc = Html::parse_document(html);
    let mut statuses = vec![];

    let row_selector = Selector::parse("table tr").map_err(|e| format!("{:?}", e))?;
    let cell_selector = Selector::parse("td").map_err(|e| format!("{:?}", e))?;

    for row in doc.select(&row_selector) {
        let cells: Vec<_> = row.select(&cell_selector).collect();
        if cells.len() < 3 {
            continue;
        }

        let name = cells[1].text().collect::<String>().trim().to_string();
        let player_text = cells[2].text().collect::<String>();

        if name.eq_ignore_ascii_case("Total")
            || name.is_empty()
            || name.eq_ignore_ascii_case("Realm")
        {
            continue;
        }

        let players = player_text
            .chars()
            .filter(|c| c.is_ascii_digit())
            .collect::<String>()
            .parse::<u32>()
            .unwrap_or_else(|e| {
                warn!(
                    "Failed to parse player count for realm '{}': {}. Defaulting to 0.",
                    name, e
                );
                0
            });

        // Extrahiere Status (Online/Offline)
        let status_text = if let Some(title) = cells[0].value().attr("title") {
            if title.to_lowercase().contains("online") {
                "Online".to_string()
            } else if title.to_lowercase().contains("offline") {
                "Offline".to_string()
            } else {
                "Unknown".to_string()
            }
        } else {
            "Unknown".to_string()
        };

        statuses.push(RealmStatus {
            name,
            online_players: players,
            status: status_text,
        });
    }
    
    Ok((statuses, vec![]))
}

/// Hilfsfunktion um gezielt den letzten Wert aus dem data-Feld einer Fraktion zu holen
fn extract_faction_value(block: &str, faction: &str) -> u32 {
    if let Some(pos) = block.find(faction) {
        let faction_area = &block[pos..];
        // Suche nach dem nächsten "data: [" nach dem Fraktionsnamen
        if let Some(d_pos) = faction_area.find("data:") {
            let data_area = &faction_area[d_pos..];
            if let Some(start) = data_area.find('[') {
                if let Some(end) = data_area[start..].find(']') {
                    let values = &data_area[start + 1..start + end];
                    return values
                        .split(',')
                        .filter_map(|v| v.trim().parse::<u32>().ok())
                        .last()
                        .unwrap_or(50);
                }
            }
        }
    }
    50
}

fn parse_news(html: &str) -> Result<Vec<(String, String)>, Box<dyn Error + Send + Sync>> {
    let doc = Html::parse_document(html);
    let mut out = vec![];
    
    // Wir suchen nach den Titel-Boxen, da diese jeden Artikel einleiten
    let title_box_selector = Selector::parse(".wm-ui-article-title").map_err(|e| format!("{:?}", e))?;
    let content_selector = Selector::parse("div.wm-ui-article-content").map_err(|e| format!("{:?}", e))?;
    let p_selector = Selector::parse("p").map_err(|e| format!("{:?}", e))?;

    // Wir iterieren über die Seite und suchen Paare von Titeln und Inhalten
    for (i, title_box) in doc.select(&title_box_selector).enumerate() {
        let mut p_tags = title_box.select(&p_selector);
        let title = p_tags.next().map(|el| el.text().collect::<String>()).unwrap_or_default();
        let date = p_tags.next().map(|el| el.text().collect::<String>()).unwrap_or_default();

        // Suche den passenden Content-Block, der meistens direkt danach kommt
        let content = doc.select(&content_selector).nth(i)
            .map(|el| el.text().collect::<String>().trim().replace('\u{A0}', " "))
            .unwrap_or_else(|| "...".to_string());

        let link = "#".to_string(); // Vereinfacht für die TUI

        if !title.trim().is_empty() {
            let display_text = format!("[{}] {}\n{}", date.trim(), title.trim(), content.trim());
            out.push((display_text, link));
        }
        if i > 15 { break; } // Sicherheitshalber begrenzen, falls die Seite riesig ist
    }

    if out.is_empty() {
        out = vec![("Forum temporarily unavailable".to_string(), "#".to_string())];
    }

    Ok(out)
}
