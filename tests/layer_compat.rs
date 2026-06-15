//! Layer compatibility. If it compiles, it works.

use tracing_core::LevelFilter;
use tracing_reload::Layer;
use tracing_subscriber::{prelude::*, registry::Registry};

#[test]
fn fmt() {
    let (layer, _handle) = Layer::new(tracing_subscriber::fmt::Layer::default());
    let s = Registry::default().with(layer);
    tracing::subscriber::with_default(s, || {});
}

#[test]
fn fmt_json() {
    let (layer, _handle) = Layer::new(tracing_subscriber::fmt::Layer::default().json());
    let s = Registry::default().with(layer);
    tracing::subscriber::with_default(s, || {});
}

#[test]
fn appender_non_blocking() {
    let (writer, _guard) = tracing_appender::non_blocking(std::io::stdout());
    let (layer, _handle) =
        Layer::new(tracing_subscriber::fmt::Layer::default().with_writer(writer));
    let s = Registry::default().with(layer);
    tracing::subscriber::with_default(s, || {});
}

#[test]
fn appender_rolling() {
    let writer = tracing_appender::rolling::never("", "");
    let (layer, _handle) =
        Layer::new(tracing_subscriber::fmt::Layer::default().with_writer(writer));
    let s = Registry::default().with(layer);
    tracing::subscriber::with_default(s, || {});
}

#[test]
fn filtered() {
    let filtered = tracing_subscriber::fmt::Layer::default().with_filter(LevelFilter::INFO);
    let (layer, _handle) = Layer::new(filtered);
    let s = Registry::default().with(layer);
    tracing::subscriber::with_default(s, || {});
}

#[test]
fn layered() {
    let layered = tracing_subscriber::fmt::Layer::default()
        .and_then(tracing_subscriber::fmt::Layer::default());
    let (layer, _handle) = Layer::new(layered);
    let s = Registry::default().with(layer);
    tracing::subscriber::with_default(s, || {});
}

#[test]
fn tracing_error() {
    let (layer, _handle) = Layer::new(tracing_error::ErrorLayer::default());
    let s = Registry::default().with(layer);
    tracing::subscriber::with_default(s, || {});
}

#[test]
fn tracing_opentelemetry() {
    let (layer, _handle) = Layer::new(tracing_opentelemetry::layer());
    let s = Registry::default().with(layer);
    tracing::subscriber::with_default(s, || {});
}

#[test]
fn tracing_tree() {
    let (layer, _handle) = Layer::new(tracing_tree::HierarchicalLayer::new(2));
    let s = Registry::default().with(layer);
    tracing::subscriber::with_default(s, || {});
}

#[test]
fn tracing_forest() {
    let (layer, _handle) = Layer::new(tracing_forest::ForestLayer::sink());
    let s = Registry::default().with(layer);
    tracing::subscriber::with_default(s, || {});
}

#[test]
fn nested_tracing_reload() {
    let (layer, _handle) = Layer::new(tracing_subscriber::fmt::Layer::default());
    let (layer, _handle) = Layer::new(layer);
    let s = Registry::default().with(layer);
    tracing::subscriber::with_default(s, || {});
}

#[test]
fn multiple_tracing_reload() {
    let (layer, _handle) = Layer::new(tracing_subscriber::fmt::Layer::default());
    let (layer2, _handle) = Layer::new(tracing_subscriber::fmt::Layer::default());
    let s = Registry::default().with(layer).with(layer2);
    tracing::subscriber::with_default(s, || {});
}

#[test]
fn json_subscriber() {
    let (layer, _handle) = Layer::new(json_subscriber::layer());
    let s = Registry::default().with(layer);
    tracing::subscriber::with_default(s, || {});
}
