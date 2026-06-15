// https://github.com/tokio-rs/tracing/issues/2499#issuecomment-1462542365

fn main() {
    use tracing_subscriber::{filter, fmt, prelude::*, registry, reload};

    let stdout_layer = fmt::Layer::default()
        .with_filter(filter::LevelFilter::INFO)
        .boxed();

    let tracing_layers = vec![stdout_layer];
    let (tracing_layers, reload_handle) = reload::Layer::new(tracing_layers);

    registry().with(tracing_layers).init();

    tracing::info!("before reload");

    let reload_result = reload_handle.modify(|layers| {
        let json_layer = fmt::Layer::default()
            .json()
            .with_filter(filter::LevelFilter::INFO)
            .boxed();
        (*layers).push(json_layer);
    });
    match reload_result {
        Ok(_) => {}, // Great!
        Err(err) => tracing::warn!("Unable to add new layer: {}", err),
    }

    tracing::info!("after reload");
}
