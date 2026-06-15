use std::{
    io::Write,
    sync::{Arc, Mutex},
};

use opentelemetry::trace::TracerProvider as _;
use opentelemetry_sdk::trace::{InMemorySpanExporter, SdkTracerProvider};
use tracing_subscriber::{fmt::writer::MakeWriter, prelude::*, reload};

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

fn main() {
    let trace_buf = Arc::new(Mutex::new(Vec::new()));
    let otel_exporter = InMemorySpanExporter::default();

    let provider = SdkTracerProvider::builder()
        .with_simple_exporter(otel_exporter.clone())
        .build();

    let fmt_layer = tracing_subscriber::fmt::layer().with_writer(BufWriter(trace_buf.clone()));
    let otel_layer = tracing_opentelemetry::layer().with_tracer(provider.tracer("v1"));

    let (otel_layer, reload_handle) = reload::Layer::new(otel_layer);

    tracing_subscriber::registry()
        .with(otel_layer)
        .with(fmt_layer)
        .init();

    tracing::info_span!("request", version = "v1").in_scope(|| {
        tracing::info!("handling request");
    });

    reload_handle
        .reload(
            tracing_opentelemetry::layer()
                .with_tracer(provider.tracer("v2"))
                .with_location(false),
        )
        .unwrap();

    tracing::info_span!("request", version = "v2").in_scope(|| {
        tracing::info!("handling another request");
    });

    provider.shutdown().unwrap();

    let trace_output = String::from_utf8(trace_buf.lock().unwrap().clone()).unwrap();
    assert!(!trace_output.is_empty(), "tracing wrote output");

    let otel_spans = otel_exporter.get_finished_spans().unwrap();
    let otel_output = format!("{otel_spans:?}");
    assert!(!otel_output.is_empty(), "opentelemetry exported spans");
}
