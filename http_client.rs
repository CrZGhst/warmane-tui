use reqwest::Client;
use std::time::Duration;

pub fn build_client() -> Result<Client, Box<dyn std::error::Error>> {
    Ok(Client::builder()
        .timeout(Duration::from_secs(5))
        .user_agent("WarmaneTUI/0.1.1")
        .build()?)
}
