use std::sync::{Arc, Mutex};

use reth_tracing::tracing;
use tokio::{fs::File, io::AsyncWriteExt};

use chrono::Utc;
use reqwest::Client;
use serde_json::Value;
use tokio::{
    spawn,
    time::{Duration, interval},
};

#[derive(Clone)]
pub struct ProfitReporter {
    buffer: Arc<Mutex<Vec<Value>>>,
    token: String,
    chat_id: String,
    interval_secs: u64,
    client: Client,
}

impl ProfitReporter {
    pub fn new(token: String, chat_id: String, interval_secs: u64) -> Self {
        let reporter = Self {
            buffer: Arc::new(Mutex::new(Vec::new())),
            token,
            chat_id,
            interval_secs,
            client: Client::new(),
        };

        let self_clone = reporter.clone();
        spawn(async move {
            self_clone.send_message("Profit reporter has been created.").await;
        });

        reporter.spawn_task();
        reporter
    }

    pub fn record(&self, info: Value) {
        let mut buf = self.buffer.lock().unwrap();
        buf.push(info);
    }

    async fn send_message(&self, text: &str) {
        let url = format!("https://api.telegram.org/bot{}/sendMessage", self.token);
        let res = self
            .client
            .post(&url)
            .json(&serde_json::json!({"chat_id": &self.chat_id, "text": text}))
            .send()
            .await;
        if let Err(e) = res {
            tracing::error!("Failed to send message to Telegram: {}", e);
        }
    }

    fn spawn_task(&self) {
        let self_clone = self.clone();
        spawn(async move {
            let client = Client::new();
            let mut timer = interval(Duration::from_secs(self_clone.interval_secs));
            loop {
                timer.tick().await;
                let data_to_process: Vec<Value> = {
                    let mut data = self_clone.buffer.lock().unwrap();
                    if data.is_empty() {
                        continue;
                    }
                    data.drain(..).collect()
                };

                let timestamp = Utc::now().format("%Y%m%d_%H%M%S");
                let filename = format!("profits_{}.json", timestamp);
                if let Ok(mut file) = File::create(&filename).await {
                    for entry in data_to_process.iter() {
                        let _ = file.write_all(entry.to_string().as_bytes()).await;
                        let _ = file.write_all(b"\n").await;
                    }
                }

                const MAX_LEN: usize = 4096;
                let mut current_message = String::new();
                let url = format!("https://api.telegram.org/bot{}/sendMessage", self_clone.token);

                for value in data_to_process.iter() {
                    let pretty_entry =
                        serde_json::to_string_pretty(&value).unwrap_or_else(|_| value.to_string());
                    let formatted_entry = format!("```json\n{}\n```\n\n", pretty_entry);

                    if current_message.len() + formatted_entry.len() > MAX_LEN {
                        if !current_message.is_empty() {
                            let res = client
                                .post(&url)
                                .json(
                                    &serde_json::json!({ "chat_id": &self_clone.chat_id, "text": &current_message, "parse_mode": "Markdown" })
                                )
                                .send().await;
                            if let Err(e) = res {
                                tracing::error!(
                                    "Failed to send profit report chunk to Telegram: {}",
                                    e
                                );
                            }
                        }
                        current_message = formatted_entry;
                    } else {
                        current_message.push_str(&formatted_entry);
                    }
                }

                if !current_message.is_empty() {
                    let res = client
                        .post(&url)
                        .json(
                            &serde_json::json!({ "chat_id": &self_clone.chat_id, "text": &current_message, "parse_mode": "Markdown" })
                        )
                        .send().await;
                    if let Err(e) = res {
                        tracing::error!(
                            "Failed to send final profit report chunk to Telegram: {}",
                            e
                        );
                    }
                }
            }
        });
    }
}

use std::sync::OnceLock;

static REPORTER: OnceLock<ProfitReporter> = OnceLock::new();

pub fn init_reporter(token: String, chat_id: String, interval: u64) {
    REPORTER.get_or_init(|| ProfitReporter::new(token, chat_id, interval));
}

pub fn record_profit(info: Value) {
    if let Some(r) = REPORTER.get() {
        r.record(info);
    }
}
