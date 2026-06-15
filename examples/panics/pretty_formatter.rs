// https://github.com/tokio-rs/tracing/issues/3511

use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

fn main() {
    let (reload_layer, reload_handle) = tracing_subscriber::reload::Layer::new(compact_layer());

    tracing_subscriber::registry().with(reload_layer).init();

    let span = tracing::info_span!("Operation");
    tracing::info!(parent: &span, "Compact");
    _ = reload_handle.modify(|fmt_layer| *fmt_layer = pretty_layer());
    tracing::info!(parent: &span, "Oh no...");
}

fn compact_layer()
-> Box<dyn tracing_subscriber::Layer<tracing_subscriber::Registry> + Send + Sync + 'static> {
    Box::new(tracing_subscriber::fmt::layer().compact())
}

fn pretty_layer()
-> Box<dyn tracing_subscriber::Layer<tracing_subscriber::Registry> + Send + Sync + 'static> {
    Box::new(tracing_subscriber::fmt::layer().pretty())
}
