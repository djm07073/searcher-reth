use std::sync::{ Arc, Mutex };

use reth_tracing::tracing;
use tokio::{ fs::File, io::AsyncWriteExt };

use chrono::Utc;
use reqwest::Client;
use serde_json::Value;
use tokio::{ spawn, time::{ Duration, interval } };

#[derive(Clone)]
pub struct ProfitReporter {
    buffer: Arc<Mutex<Vec<Value>>>,
    token: String,
    chat_id: String,
    interval_secs: u64,
}

impl ProfitReporter {
    pub fn new(token: String, chat_id: String, interval_secs: u64) -> Self {
        let reporter = Self {
            buffer: Arc::new(Mutex::new(Vec::new())),
            token,
            chat_id,
            interval_secs,
        };
        {
            let client = Client::new();
            let url = format!("https://api.telegram.org/bot{}/sendMessage", reporter.token);
            let text = "Profit reporter has been created.";
            let chat_id = reporter.chat_id.clone();
            spawn(async move {
                let res = client
                    .post(url)
                    .json(&serde_json::json!({"chat_id": chat_id, "text": text}))
                    .send()
                    .await;
                if let Err(e) = res {
                    tracing::error!("Failed to send creation message to Telegram: {}", e);
                }
            });
        }
        reporter.spawn_task();
        reporter
    }

    pub fn record(&self, info: Value) {
        let mut buf = self.buffer.lock().unwrap();
        buf.push(info);
    }

    fn spawn_task(&self) {
        let buffer = self.buffer.clone();
        let token = self.token.clone();
        let chat_id = self.chat_id.clone();
        let interval_secs = self.interval_secs;
        spawn(async move {
            let client = Client::new();
            let mut timer = interval(Duration::from_secs(interval_secs));
            loop {
                timer.tick().await;
                let data_to_process: Vec<Value> = {
                    let mut data = buffer.lock().unwrap();
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
                let text = data_to_process
                    .iter()
                    .map(|v| v.to_string())
                    .collect::<Vec<_>>()
                    .join("\n");
                let url = format!("https://api.telegram.org/bot{}/sendMessage", token);
                let res = client
                    .post(url)
                    .json(&serde_json::json!({"chat_id": chat_id, "text": text}))
                    .send().await;
                if let Err(e) = res {
                    tracing::error!("Failed to send profit report to Telegram: {}", e);
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
