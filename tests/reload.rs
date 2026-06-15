//! Integration tests adapted from `tracing-subscriber/tests/reload.rs`

use std::sync::atomic::{AtomicUsize, Ordering};

use tracing_core::{
    Metadata, LevelFilter, Subscriber,
    subscriber::Interest,
};
use tracing_subscriber::{layer, prelude::*, registry::LookupSpan, registry::Registry};
use tracing_reload::Layer;

fn event() {
    tracing::info!("my event");
}

/// Running these two tests in parallel will cause flaky failures, since they are both modifying
/// the MAX_LEVEL value. `cargo test -- --test-threads=1` fixes it, but it runs all tests in serial.
/// The only way to run tests in serial in a single file is this way.
#[test]
fn run_all_reload_test() {
    reload_handle();
    reload_filter();
}

fn reload_handle() {
    static FILTER1_CALLS: AtomicUsize = AtomicUsize::new(0);
    static FILTER2_CALLS: AtomicUsize = AtomicUsize::new(0);

    enum Filter {
        One,
        Two,
    }

    impl<S: Subscriber + for<'a> LookupSpan<'a>> tracing_subscriber::Layer<S> for Filter {
        fn register_callsite(&self, _m: &'static Metadata<'static>) -> Interest {
            println!("REGISTER: {:?}", _m);
            Interest::sometimes()
        }

        fn enabled(&self, _m: &Metadata<'_>, _: layer::Context<'_, S>) -> bool {
            println!("ENABLED: {:?}", _m);
            match self {
                Filter::One => FILTER1_CALLS.fetch_add(1, Ordering::SeqCst),
                Filter::Two => FILTER2_CALLS.fetch_add(1, Ordering::SeqCst),
            };
            true
        }

        fn max_level_hint(&self) -> Option<LevelFilter> {
            match self {
                Filter::One => Some(LevelFilter::INFO),
                Filter::Two => Some(LevelFilter::DEBUG),
            }
        }
    }

    let (reload, handle) = Layer::new(Filter::One);
    let subscriber = Registry::default().with(reload);

    tracing::subscriber::with_default(subscriber, || {
        assert_eq!(FILTER1_CALLS.load(Ordering::SeqCst), 0);
        assert_eq!(FILTER2_CALLS.load(Ordering::SeqCst), 0);

        event();

        assert_eq!(FILTER1_CALLS.load(Ordering::SeqCst), 1);
        assert_eq!(FILTER2_CALLS.load(Ordering::SeqCst), 0);

        assert_eq!(LevelFilter::current(), LevelFilter::INFO);
        handle.reload(Filter::Two).expect("should reload");
        assert_eq!(LevelFilter::current(), LevelFilter::DEBUG);

        event();

        assert_eq!(FILTER1_CALLS.load(Ordering::SeqCst), 1);
        assert_eq!(FILTER2_CALLS.load(Ordering::SeqCst), 1);
    });
}

// This is changed a lot from the original because we currently don't implement `Filter`, just
// `Layer`.
fn reload_filter() {
    static FILTER1_CALLS: AtomicUsize = AtomicUsize::new(0);
    static FILTER2_CALLS: AtomicUsize = AtomicUsize::new(0);

    enum Filter {
        One,
        Two,
    }

    impl<S: Subscriber + for<'a> LookupSpan<'a>> tracing_subscriber::layer::Filter<S> for Filter {
        fn enabled(&self, _m: &Metadata<'_>, _: &layer::Context<'_, S>) -> bool {
            println!("ENABLED: {:?}", _m);
            match self {
                Filter::One => FILTER1_CALLS.fetch_add(1, Ordering::SeqCst),
                Filter::Two => FILTER2_CALLS.fetch_add(1, Ordering::SeqCst),
            };
            true
        }

        fn max_level_hint(&self) -> Option<LevelFilter> {
            match self {
                Filter::One => Some(LevelFilter::INFO),
                Filter::Two => Some(LevelFilter::DEBUG),
            }
        }
    }

    struct NopLayer;

    impl<S: Subscriber + for<'a> LookupSpan<'a>> tracing_subscriber::Layer<S> for NopLayer {}

    let filtered = NopLayer.with_filter(Filter::One);
    let (reload, handle) = Layer::new(filtered);

    let subscriber = Registry::default().with(reload);

    tracing::subscriber::with_default(subscriber, || {
        assert_eq!(FILTER1_CALLS.load(Ordering::SeqCst), 0);
        assert_eq!(FILTER2_CALLS.load(Ordering::SeqCst), 0);

        event();

        assert_eq!(FILTER1_CALLS.load(Ordering::SeqCst), 1);
        assert_eq!(FILTER2_CALLS.load(Ordering::SeqCst), 0);

        assert_eq!(LevelFilter::current(), LevelFilter::INFO);
        let new_filtered = NopLayer.with_filter(Filter::Two);
        handle.reload(new_filtered).expect("should reload");
        assert_eq!(LevelFilter::current(), LevelFilter::DEBUG);

        event();

        assert_eq!(FILTER1_CALLS.load(Ordering::SeqCst), 1);
        assert_eq!(FILTER2_CALLS.load(Ordering::SeqCst), 1);
    });
}
