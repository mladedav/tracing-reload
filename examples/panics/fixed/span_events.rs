use tracing_subscriber::{filter::LevelFilter, fmt::format::FmtSpan, prelude::*};

fn main() {
    let layer = tracing_subscriber::fmt::layer()
        .with_span_events(FmtSpan::FULL)
        .with_filter(LevelFilter::TRACE);
    let (layer, reload_handle) = tracing_reload::Layer::new(layer);
    tracing_subscriber::registry().with(layer).init();

    // open and enter a span, incrementing
    let span = tracing::info_span!("my_span");
    let _guard = span.enter();

    // turn off span events
    reload_handle
        .reload(
            tracing_subscriber::fmt::layer()
                .with_span_events(FmtSpan::NONE)
                .with_filter(LevelFilter::TRACE),
        )
        .unwrap();

    // exit the span, not decrementing
    drop(_guard);

    // turn back on span events before closing the span
    reload_handle
        .reload(
            tracing_subscriber::fmt::layer()
                .with_span_events(FmtSpan::FULL)
                .with_filter(LevelFilter::TRACE),
        )
        .unwrap();

    // close the span
    drop(span); // no bang
}
