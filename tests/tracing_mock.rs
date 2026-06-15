use tracing_mock::{expect, layer};
use tracing_reload::Layer;
use tracing_subscriber::{filter::filter_fn, prelude::*, registry::Registry};

#[derive(Debug)]
struct NoopLayer;
impl<S: tracing_core::Subscriber> tracing_subscriber::Layer<S> for NoopLayer {}

#[test]
fn events_forwarded_before_and_after_reload() {
    let (mock, handle) = layer::mock()
        .event(expect::event().with_fields(expect::msg("before reload")))
        .event(expect::event().with_fields(expect::msg("after reload")))
        .only()
        .run_with_handle();

    let (reload, reload_handle) = Layer::new(NoopLayer);
    let subscriber = Registry::default()
        .with(reload)
        .with(mock.with_filter(filter_fn(|_| true)));

    tracing::subscriber::with_default(subscriber, || {
        tracing::info!("before reload");
        reload_handle.reload_with(|_| NoopLayer).unwrap();
        tracing::info!("after reload");
    });

    handle.assert_finished();
}

#[test]
fn span_lifecycle_after_reload() {
    let span1 = expect::span()
        .named("my_span")
        .at_level(tracing::Level::INFO);
    let span2 = expect::span()
        .named("my_span")
        .at_level(tracing::Level::INFO);
    let (mock, handle) = layer::mock()
        .new_span(span1.clone())
        .enter(&span1)
        .exit(&span1)
        .new_span(span2.clone())
        .enter(&span2)
        .exit(&span2)
        .only()
        .run_with_handle();

    let (reload, reload_handle) = Layer::new(NoopLayer);
    let subscriber = Registry::default()
        .with(reload)
        .with(mock.with_filter(filter_fn(|_| true)));

    tracing::subscriber::with_default(subscriber, || {
        {
            let span = tracing::info_span!("my_span");
            let _guard = span.enter();
        }

        reload_handle.reload_with(|_| NoopLayer).unwrap();

        {
            let span = tracing::info_span!("my_span");
            let _guard = span.enter();
        }
    });

    handle.assert_finished();
}

#[test]
fn event_fields_preserved_across_reload() {
    let (mock, handle) = layer::mock()
        .event(
            expect::event().with_fields(
                expect::field("number")
                    .with_value(&42)
                    .and(expect::field("name").with_value(&"hello")),
            ),
        )
        .event(
            expect::event().with_fields(
                expect::field("number")
                    .with_value(&99)
                    .and(expect::field("name").with_value(&"world")),
            ),
        )
        .only()
        .run_with_handle();

    let (reload, reload_handle) = Layer::new(NoopLayer);
    let subscriber = Registry::default()
        .with(reload)
        .with(mock.with_filter(filter_fn(|_| true)));

    tracing::subscriber::with_default(subscriber, || {
        tracing::info!(number = 42, name = "hello", "before reload");
        reload_handle.reload_with(|_| NoopLayer).unwrap();
        tracing::info!(number = 99, name = "world", "after reload");
    });

    handle.assert_finished();
}

#[test]
fn events_through_multiple_reloads() {
    let span = expect::span().named("root").at_level(tracing::Level::INFO);
    let (mock, handle) = layer::mock()
        .event(expect::event().with_fields(expect::msg("first")))
        .new_span(span)
        .event(expect::event().with_fields(expect::msg("second")))
        .event(expect::event().with_fields(expect::msg("third")))
        .only()
        .run_with_handle();

    let (reload, reload_handle) = Layer::new(NoopLayer);
    let subscriber = Registry::default()
        .with(reload)
        .with(mock.with_filter(filter_fn(|_| true)));

    tracing::subscriber::with_default(subscriber, || {
        tracing::info!("first");
        reload_handle.reload_with(|_| NoopLayer).unwrap();
        let _span = tracing::info_span!("root");
        tracing::info!("second");
        reload_handle.reload_with(|_| NoopLayer).unwrap();
        tracing::info!("third");
    });

    handle.assert_finished();
}
