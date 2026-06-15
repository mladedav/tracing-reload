//! This is probably the most complex test. There is interaction between the two layers in that
//! `tracing-opentelemetry` sets the `opentelemetry::Context` and ties it to a span. It is queriable
//! by a function to which one needs to pass a `Dispatch` that downcasts to the OpenTelemetry layer
//! (or rather to something private it holds) and also to the `S` that the layer thinks is the
//! subscriber. The other layer saves the `Dispatch` it got in its `on_register_dispatch` and uses
//! the mentioned method.

use std::{
    io::Write,
    sync::{Arc, Mutex},
};

use opentelemetry::trace::TracerProvider;
use opentelemetry_sdk::{
    error::OTelSdkResult,
    trace::{SpanData, SpanExporter, Tracer as SdkTracer},
};
use tracing_opentelemetry::OpenTelemetryLayer;
use tracing_reload::{Handle, Layer, ReloadSubscriber};
use tracing_subscriber::{
    Registry,
    fmt::MakeWriter,
    layer::{Layered, SubscriberExt},
};

#[derive(Debug, Default)]
struct TestExporter(Arc<Mutex<Vec<SpanData>>>);

impl SpanExporter for TestExporter {
    async fn export(&self, mut batch: Vec<SpanData>) -> OTelSdkResult {
        let spans = self.0.clone();
        if let Ok(mut inner) = spans.lock() {
            inner.append(&mut batch);
        }
        Ok(())
    }
}

#[derive(Default, Clone)]
struct BufWriter(Arc<Mutex<Vec<u8>>>);

impl<'a> MakeWriter<'a> for BufWriter {
    type Writer = BufGuard;

    fn make_writer(&'a self) -> Self::Writer {
        BufGuard(self.0.clone())
    }
}

struct BufGuard(Arc<Mutex<Vec<u8>>>);

impl Write for BufGuard {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().write(buf)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.0.lock().unwrap().flush()
    }
}

#[test]
fn opentelemetry_with_json() {
    run(|_, _, _, _| {})
}

/// This is exactly the same thing, we just reload the layer before stuff happens. This exercises
/// `on_register_dispatch` called on reloaded layers.
#[test]
fn opentelemetry_with_json_reloaded() {
    run(|tracer, output, opentelemetry, json_subscriber| {
        // These are the same.
        opentelemetry
            .reload(tracing_opentelemetry::layer().with_tracer(tracer))
            .unwrap();
        json_subscriber
            .reload(
                json_subscriber::layer()
                    .with_writer(output.clone())
                    .with_current_span(false)
                    .with_span_list(false)
                    .with_opentelemetry_ids(true),
            )
            .unwrap();
    })
}

type ReloadableOpenTelemetryLayer<S> = OpenTelemetryLayer<ReloadSubscriber<S>, SdkTracer>;
type OpenTelemetryHandle = Handle<ReloadableOpenTelemetryLayer<Registry>, Registry>;
type LayeredWithOpenTelemetryLayer<S> = Layered<Layer<ReloadableOpenTelemetryLayer<S>, S>, S>;
type JsonLayerHandle = Handle<
    json_subscriber::fmt::Layer<
        ReloadSubscriber<LayeredWithOpenTelemetryLayer<Registry>>,
        BufWriter,
    >,
    LayeredWithOpenTelemetryLayer<Registry>,
>;

fn run<F>(f: F)
where
    F: FnOnce(SdkTracer, BufWriter, OpenTelemetryHandle, JsonLayerHandle),
{
    let exporter = TestExporter::default();
    let spans_arc = exporter.0.clone();
    let builder =
        opentelemetry_sdk::trace::SdkTracerProvider::builder().with_simple_exporter(exporter);
    let provider = builder.build();
    let tracer = provider.tracer("test-exporter");
    opentelemetry::global::set_tracer_provider(provider);

    let output = BufWriter::default();

    let opentelemetry_layer = tracing_opentelemetry::layer().with_tracer(tracer.clone());
    let json_layer = json_subscriber::layer()
        .with_writer(output.clone())
        .with_current_span(false)
        .with_span_list(false)
        .with_opentelemetry_ids(true);

    let (opentelemetry_layer, opentelemetry_handle) =
        tracing_reload::Layer::new(opentelemetry_layer);
    let (json_layer, json_handle) = tracing_reload::Layer::new(json_layer);

    let subscriber = tracing_subscriber::registry()
        .with(opentelemetry_layer)
        .with(json_layer);

    tracing::subscriber::with_default(subscriber, || {
        f(tracer, output.clone(), opentelemetry_handle, json_handle);

        // this creates a new event, outside of any spans.
        tracing::info!(number_of_yaks = 3, "preparing to shave yaks");

        for i in 0..3 {
            let _span = tracing::info_span!("yak_shaving").entered();
            tracing::info!(yak_number = i, "shaving...");
        }
        tracing::info!(all_yaks_shaved = true, "yak shaving completed.");
    });

    // Gather OTel span IDs from the exporter
    let spans = spans_arc.lock().unwrap();
    let otel_ids: Vec<(String, String)> = spans
        .iter()
        .map(|s| {
            let trace_id = s.span_context.trace_id().to_string();
            let span_id = s.span_context.span_id().to_string();
            (trace_id, span_id)
        })
        .collect();
    assert_eq!(otel_ids.len(), 3, "expected 3 yak_shaving spans");

    // Parse JSON output lines
    let json_output = output.0.lock().unwrap();
    let output_str = String::from_utf8(json_output.clone()).unwrap();
    let lines: Vec<serde_json::Value> = output_str
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert_eq!(lines.len(), 5, "expected 5 JSON lines");

    // Event 1: outside any span — no OTel IDs
    assert!(lines[0].get("openTelemetry").is_none());

    // Events 2-4: inside yak_shaving spans — must have matching OTel IDs
    for (i, line) in lines[1..4].iter().enumerate() {
        let json_ids = line["openTelemetry"].as_object().unwrap();
        assert_eq!(json_ids["traceId"].as_str().unwrap(), otel_ids[i].0);
        assert_eq!(json_ids["spanId"].as_str().unwrap(), otel_ids[i].1);
    }

    // Event 5: outside any span — no OTel IDs
    assert!(lines[4].get("openTelemetry").is_none());
}
