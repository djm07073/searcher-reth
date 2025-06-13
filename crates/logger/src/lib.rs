use reth_tracing::{
    tracing::{ self, info_span },
    tracing_appender::{ non_blocking, rolling::daily },
    tracing_subscriber,
};

pub fn init(service: &str) -> non_blocking::WorkerGuard {
    let log_dir = get_log_dir();
    let file_appender = daily(&log_dir, format!("{}.log", service));
    let (writer, guard) = non_blocking(file_appender);

    let subscriber = tracing_subscriber
        ::fmt()
        .with_writer(writer)
        .with_thread_ids(true)
        .json()
        .with_current_span(true)
        .with_span_events(tracing_subscriber::fmt::format::FmtSpan::FULL)
        .finish();
    let span = info_span!("app", service = service);
    let _entered = span.enter();

    tracing::subscriber::set_global_default(subscriber).expect("Failed to set tracing subscriber");
    guard
}

fn get_log_dir() -> String {
    if let Ok(log_dir) = std::env::var("LOG_DIR") {
        return log_dir;
    }

    if cfg!(debug_assertions) {
        return "./logs".to_string();
    }

    std::env
        ::var("HOME")
        .map(|home| format!("{}/logs", home))
        .unwrap_or_else(|_| "./logs".to_string())
}
