use std::sync::atomic::{AtomicU32, Ordering};

use tracing_core::LevelFilter;
use tracing_reload::Layer;
use tracing_subscriber::{Registry, layer::SubscriberExt};

#[test]
fn modify_interest_cache_rebuilt() {
    static COUNT: AtomicU32 = AtomicU32::new(0);

    fn test_log() {
        // A log event with a side-effect. Don't do this at home. Seriously.
        // The idea is that the first time this should be filtered out and the field values are
        // not built, thus the counter not incremented. The second time, this is not filtered
        // out and the side-effect can be observed.
        tracing::info!(count = COUNT.fetch_add(1, Ordering::Relaxed));
    }
    let (reload, handle) = Layer::new(LevelFilter::WARN);
    let subscriber = Registry::default().with(reload);

    tracing::subscriber::with_default(subscriber, || {
        assert_eq!(LevelFilter::current(), LevelFilter::WARN);
        test_log();
        assert_eq!(COUNT.load(Ordering::Relaxed), 0);

        handle.reload(LevelFilter::INFO).unwrap();

        assert_eq!(LevelFilter::current(), LevelFilter::INFO);
        test_log();
        assert_eq!(COUNT.load(Ordering::Relaxed), 1);
    });
}
