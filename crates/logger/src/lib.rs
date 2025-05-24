use reth_tracing::tracing::{ self, info_span };
use reth_tracing::tracing_appender::{ non_blocking, rolling::daily };
use reth_tracing::tracing_subscriber;

pub fn init(service: &str) -> non_blocking::WorkerGuard {
    let file_appender = daily(&format!("/var/log/{}", service), &format!("{}.log", service));
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
