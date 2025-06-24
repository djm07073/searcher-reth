use reth_tracing::{
    tracing::{self, info_span},
    tracing_appender::{non_blocking, rolling::daily},
    tracing_subscriber::{self, EnvFilter, layer::SubscriberExt, util::SubscriberInitExt},
};

pub fn init(service: &str) -> Result<non_blocking::WorkerGuard, Box<dyn std::error::Error>> {
    let log_dir = get_log_dir();

    let file_appender = daily(&log_dir, format!("{}.log", service));
    let (writer, guard) = non_blocking(file_appender);

    let env_filter = EnvFilter::try_from_default_env()
        .or_else(|_| EnvFilter::try_new("info")) // 기본값: info
        .unwrap_or_else(|_| EnvFilter::new("debug"));

    let console_layer =
        tracing_subscriber::fmt::layer().with_target(true).with_thread_ids(true).with_ansi(true);

    let file_layer = tracing_subscriber::fmt::layer()
        .with_writer(writer)
        .with_target(true)
        .with_thread_ids(true)
        .with_ansi(false)
        .json();

    tracing_subscriber::registry()
        .with(env_filter)
        .with(console_layer)
        .with(file_layer)
        .try_init()
        .map_err(|e| format!("Failed to set tracing subscriber: {}", e))?;

    let span = info_span!("app", service = service);
    let _entered = span.enter();

    tracing::info!(service = service, log_dir = log_dir, "Logger initialized successfully");

    Ok(guard)
}

fn get_log_dir() -> String {
    if let Ok(log_dir) = std::env::var("LOG_DIR") {
        return log_dir;
    }

    if cfg!(debug_assertions) {
        return "./logs".to_string();
    }

    std::env::var("HOME")
        .map(|home| format!("{}/logs", home))
        .unwrap_or_else(|_| "./logs".to_string())
}

#[cfg(test)]
mod tests {
    use super::get_log_dir;

    #[test]
    fn returns_log_dir_from_env() {
        unsafe { std::env::set_var("LOG_DIR", "/tmp/test_logs") };
        assert_eq!(get_log_dir(), "/tmp/test_logs");
        unsafe { std::env::remove_var("LOG_DIR") };
    }

    #[test]
    fn returns_default_log_dir_in_debug() {
        unsafe { std::env::remove_var("LOG_DIR") };
        assert_eq!(get_log_dir(), "./logs");
    }
}
