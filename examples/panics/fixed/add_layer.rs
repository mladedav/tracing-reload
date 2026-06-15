fn main() {
    use tracing_subscriber::{filter, fmt, prelude::*, registry};

    let stdout_layer = fmt::Layer::default()
        .with_filter(filter::LevelFilter::INFO)
        .boxed();

    let tracing_layers = vec![stdout_layer];
    let (tracing_layers, reload_handle) = tracing_reload::Layer::new(tracing_layers);

    registry().with(tracing_layers).init();

    tracing::info!("before reload");

    let reload_result = reload_handle.reload({
        let stdout_layer = fmt::Layer::default()
            .with_filter(filter::LevelFilter::INFO)
            .boxed();
        let json_layer = fmt::Layer::default()
            .json()
            .with_filter(filter::LevelFilter::INFO)
            .boxed();
        vec![stdout_layer, json_layer]
    });
    match reload_result {
        Ok(_) => {}, // Great!
        Err(err) => tracing::warn!("Unable to add new layer: {}", err),
    }

    tracing::info!("after reload");
}
