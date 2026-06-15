use tracing::info;
use tracing_subscriber::{filter, fmt, prelude::*, reload};

fn main() {
    let filter = filter::LevelFilter::WARN;
    let (filter, reload_handle) = reload::Layer::new(filter);
    tracing_subscriber::registry()
        .with(filter)
        .with(fmt::Layer::default())
        .init();
    info!("This will be ignored");
    reload_handle
        .modify(|filter| *filter = filter::LevelFilter::INFO)
        .unwrap();
    info!("This will be logged");
}
