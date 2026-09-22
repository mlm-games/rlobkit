use std::sync::OnceLock;

static INIT: OnceLock<()> = OnceLock::new();

pub fn init(tag: &'static str, default_filter: &str) {
    INIT.get_or_init(|| {
        use tracing_subscriber::layer::SubscriberExt as _;
        use tracing_subscriber::util::SubscriberInitExt as _;
        let filter = tracing_subscriber::EnvFilter::try_new(
            std::env::var("RUST_LOG").unwrap_or_else(|_| default_filter.into()),
        )
        .unwrap_or_else(|_| {
            default_filter
                .parse()
                .unwrap_or_else(|_| "warn".parse().unwrap())
        });
        let _ = tracing_subscriber::registry()
            .with(filter)
            .with(paranoid_android::layer(tag))
            .try_init();
        let _ = tracing_log::LogTracer::init();
    });
}
