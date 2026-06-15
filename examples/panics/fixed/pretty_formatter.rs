use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

fn main() {
    let (reload_layer, reload_handle) =
        tracing_reload::Layer::new(Box::new(tracing_subscriber::fmt::layer().compact())
            as Box<dyn tracing_subscriber::Layer<_> + Send + Sync>);

    tracing_subscriber::registry().with(reload_layer).init();

    let span = tracing::info_span!("Operation");
    tracing::info!(parent: &span, "Compact");
    reload_handle
        .reload(Box::new(tracing_subscriber::fmt::layer().pretty())
            as Box<dyn tracing_subscriber::Layer<_> + Send + Sync>)
        .unwrap();
    tracing::info!(parent: &span, "It's still compact but doesn't panic!");

    tracing::info_span!("Pretty operation").in_scope(|| {
        tracing::info!("Now it's pretty.");
        reload_handle
            .reload(Box::new(tracing_subscriber::fmt::layer().compact())
                as Box<dyn tracing_subscriber::Layer<_> + Send + Sync>)
            .unwrap();
        tracing::info!("And the whole span will be pretty.");
    });
}
