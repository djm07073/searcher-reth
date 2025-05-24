use reth_tracing::tracing::{ self, info_span };
use tracing_appender::{ non_blocking, rolling::daily };

pub const SERVICE_NAME: &str = "searcher-reth";

pub fn init() -> non_blocking::WorkerGuard {
    let file_appender = daily("/var/log/myapp", &format!("{}.log", SERVICE_NAME));
    let (writer, guard) = non_blocking(file_appender);

    let subscriber = tracing_subscriber
        ::fmt()
        .with_writer(writer)
        .with_thread_ids(true)
        .json()
        .with_current_span(true)
        .with_span_events(tracing_subscriber::fmt::format::FmtSpan::FULL)
        .finish();
    let span = info_span!("app", service = SERVICE_NAME);
    let _entered = span.enter();

    tracing::subscriber::set_global_default(subscriber).expect("Failed to set tracing subscriber");
    guard
}
