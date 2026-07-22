//! Wrapper for a [`Layer`](tracing_subscriber::layer::Layer) to allow it to be dynamically modified
//! at runtime. This is based on [`tracing_subscriber::reload`].
//!
//! This crate provides a [`Layer`] type implementing the [`Layer`
//! trait](tracing_subscriber::layer::Layer) which wraps another type implementing the same trait.
//! Reloads publish a new value and retire the old one instead of mutating in place, which together
//! with not moving the old layers keeps pointers returned from [`downcast_raw`] valid. The
//! [`on_layer`] setup hook is forwarded through a mutable reference during construction, before any
//! such pointer is handed out.
//!
//! The inner value is reloaded by *replacing* it through [`Handle::reload_with`] (or
//! [`Handle::reload`]); it is never mutated in place. Each reload publishes a new value and retires
//! the previous one, which is kept alive so that pointers previously handed out by [`downcast_raw`]
//! stay valid and all events in a given span can be routed to the same layer. Retired versions are
//! reclaimed when the subscriber is dropped.
//!
//! ## Why to use this instead of `tracing_subscriber::reload::Layer`
//!
//! This layer is built so that [`downcast_raw`] is possible through this layer.  The price is
//! leaked memory (and possibly other resources) on every reload.
//!
//! That happens because [`downcast_raw`] returns a raw pointer and if we return a pointer to the
//! inner layer, we need to guarantee that the pointer is valid for as long as someone has a
//! reference to the layer. The method has no safety doc comment in [`tracing-subscriber`] and from
//! other usage it is only clear that it should be valid for as long as the `&self` argument. But we
//! can never know when that reference goes out of scope, we can only be sure it is safe to clean up
//! once this [`Layer`] is dropped. Until then we have to keep around all layers we have downcasted
//! to.
//!
//! Furthermore, this layer tracks which version of the inner layer created a given span and routes
//! `on_event`, `on_enter`, `on_exit`, and `on_close` to that layer even if a newer one is
//! available. This is because some layers depend on information stored in span extensions which is
//! added on span creation and may panic when reload happens at a wrong time.
//!
//! # Examples
//!
//! ```rust
//! # use tracing::info;
//! use tracing_reload::{Layer, Handle};
//! use tracing_subscriber::{filter, fmt, prelude::*};
//! let filter = filter::LevelFilter::WARN;
//! let (filter, reload_handle) = Layer::new(filter);
//! tracing_subscriber::registry()
//!   .with(filter)
//!   .with(fmt::Layer::default())
//!   .init();
//! #
//! # // specifying the Registry type is required
//! # let _: &Handle<filter::LevelFilter, tracing_subscriber::Registry> = &reload_handle;
//! #
//! info!("This will be ignored");
//! reload_handle.reload(filter::LevelFilter::INFO);
//! info!("This will be logged");
//! ```
//!
//! Reloading a [`Filtered`](tracing_subscriber::filter::Filtered) layer:
//!
//! ```rust
//! # use tracing::info;
//! use tracing_subscriber::{filter, fmt, reload, prelude::*};
//! let filtered_layer = fmt::Layer::default().with_filter(filter::LevelFilter::WARN);
//! let (filtered_layer, reload_handle) = reload::Layer::new(filtered_layer);
//! #
//! # // specifying the Registry type is required
//! # let _: &reload::Handle<filter::Filtered<fmt::Layer<tracing_subscriber::Registry>,
//! # filter::LevelFilter, tracing_subscriber::Registry>,tracing_subscriber::Registry>
//! # = &reload_handle;
//! #
//! tracing_subscriber::registry()
//!   .with(filtered_layer)
//!   .init();
//! info!("This will be ignored");
//! reload_handle.modify(|layer| *layer.filter_mut() = filter::LevelFilter::INFO);
//! info!("This will be logged");
//! ```
//!
//! [`on_layer`]: tracing_subscriber::layer::Layer::on_layer
//! [`downcast_raw`]: tracing_subscriber::layer::Layer::downcast_raw
//! [`Filter` trait]: tracing_subscriber::layer::Filter
//! [`Layer` trait]: tracing_subscriber::layer::Layer
//! [`tracing-subscriber`]: tracing_subscriber

#![cfg_attr(docsrs, feature(doc_cfg))]
#![warn(clippy::pedantic)]
#![warn(
    clippy::allow_attributes,
    clippy::allow_attributes_without_reason,
    clippy::as_conversions,
    clippy::expect_used,
    clippy::future_not_send,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::panic_in_result_fn,
    clippy::string_slice,
    clippy::todo,
    clippy::unreachable,
    clippy::unwrap_used
)]
#![expect(
    clippy::missing_errors_doc,
    reason = "These need some more cleanup and documenting."
)]

use std::{
    cell::Cell,
    collections::HashMap,
    marker::PhantomData,
    num::NonZeroUsize,
    ops::{Deref, DerefMut},
    sync::{Arc, Mutex, OnceLock, RwLock, Weak, atomic::AtomicBool},
};

use subscriber_cache::SubscriberCache;
use tracing::{Subscriber, dispatcher::WeakDispatch};
use tracing_core::{callsite, span};

macro_rules! try_lock {
    ($lock:expr) => {
        try_lock!($lock, else return)
    };
    ($lock:expr, else $els:expr) => {
        if let ::core::result::Result::Ok(l) = $lock {
            l
        } else if std::thread::panicking() {
            $els
        } else {
            panic!("lock poisoned")
        }
    };
}

mod error;
mod layer;
mod subscriber;
mod subscriber_cache;

pub use error::Error;
pub use subscriber::ReloadSubscriber;
use tracing_subscriber::{
    filter::FilterId,
    layer::{Context, SubscriberExt},
    registry::LookupSpan,
};

const MAX_FILTER_COUNT: NonZeroUsize = NonZeroUsize::new(16).unwrap();

/// Determines how new spans are routed when layers are reloaded.
#[derive(Debug, Clone, Copy)]
pub enum ReloadRouting {
    /// Route new spans to the most recently reloaded layer.
    NewSpanToLatestLayer,

    /// Route new spans to the same layer that handled their parent span.
    NewSpanToParentLayer,
}

/// Wraps another type implementing [`tracing_subscriber::layer::Layer`] or
/// [`tracing_subscriber::layer::Filter`], allowing it to be modified dynamically at runtime.
///
/// The inner value is held even after the layer was reloaded if someone tried to downcast to it.
/// This makes it safe to return raw pointers from
/// [`Layer::downcast_raw`](tracing_subscriber::layer::Layer::downcast_raw).
#[derive(Debug)]
pub struct Layer<L, S> {
    inner: Arc<Shared<L>>,
    span_routing: ReloadRouting,
    subscriber_cache: Mutex<SubscriberCache>,
    _s: PhantomData<fn(S)>,
}

/// Allows modifying the state of an associated reloadable layer.
#[derive(Debug)]
pub struct Handle<L, S> {
    inner: Weak<Shared<L>>,
    _s: PhantomData<fn(S)>,
}

/// Shared storage behind a [`Layer`] and its [`Handle`].
#[derive(Debug)]
struct Shared<L> {
    dispatch: OnceLock<WeakDispatch>,
    dispatch_leaked: AtomicBool,

    layers: RwLock<Layers<L>>,

    span_map: RwLock<HashMap<span::Id, Arc<L>>>,
    filters: Mutex<ReloadableFilters>,
}

#[derive(Debug)]
struct Layers<L> {
    latest: InnerLayer<Arc<L>>,
    historical: Vec<InnerLayer<Option<Arc<L>>>>,
}

impl<L> Layers<L> {
    fn new(inner: L) -> Self {
        Self {
            latest: InnerLayer::new(Arc::new(inner)),
            historical: Vec::new(),
        }
    }

    fn push(&mut self, next: L) {
        let previous = std::mem::replace(&mut self.latest, InnerLayer::new(Arc::new(next)));
        self.historical.push(previous.map(Some));
    }
}

#[derive(Debug)]
struct InnerLayer<L> {
    inner: L,
}

impl<L> Deref for InnerLayer<L> {
    type Target = L;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<L> DerefMut for InnerLayer<L> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

impl<L> InnerLayer<L> {
    fn new(inner: L) -> Self {
        Self { inner }
    }

    fn map<L2, F: FnOnce(L) -> L2>(self, f: F) -> InnerLayer<L2> {
        InnerLayer::<L2> {
            inner: f(self.inner),
        }
    }
}

#[derive(Debug, Clone)]
struct ReloadableFilters {
    used: Vec<FilterId>,
    free: Vec<FilterId>,
    backup: FilterId,
}
impl ReloadableFilters {
    fn new() -> Self {
        // We'll get another later. For now, we just get any filter ID we can.
        let backup = tracing_subscriber::registry().register_filter();
        Self {
            used: Vec::new(),
            free: Vec::new(),
            backup,
        }
    }
}

// ===== impl Layer =====

impl<L, S> Layer<L, S> {
    /// Wraps the given value, returning a [`Layer`] and a [`Handle`] that allows the inner value to
    /// be replaced at runtime.
    pub fn new(inner: L) -> (Self, Handle<L, S>) {
        Self::new_with_routing(inner, ReloadRouting::NewSpanToLatestLayer)
    }

    /// Wraps the given value with a specific span routing strategy, returning a [`Layer`] and a
    /// [`Handle`] that allows the inner value to be replaced at runtime.
    pub fn new_with_routing(inner: L, span_routing: ReloadRouting) -> (Self, Handle<L, S>) {
        let this = Self {
            inner: Arc::new(Shared {
                dispatch: OnceLock::new(),
                dispatch_leaked: AtomicBool::new(false),
                layers: RwLock::new(Layers::new(inner)),
                span_map: RwLock::new(HashMap::new()),
                filters: Mutex::new(ReloadableFilters::new()),
            }),
            span_routing,
            subscriber_cache: Mutex::new(SubscriberCache::default()),
            _s: PhantomData,
        };
        let handle = this.handle();
        (this, handle)
    }

    /// Returns a [`Handle`] that can be used to replace the wrapped [`Layer`].
    pub fn handle(&self) -> Handle<L, S> {
        Handle {
            inner: Arc::downgrade(&self.inner),
            _s: PhantomData,
        }
    }

    fn with_span<T>(&self, span: &span::Id, mapper: impl FnOnce(&L) -> T) -> Option<T> {
        let layers = try_lock!(self.inner.span_map.read(), else return None);
        let layer = layers.get(span)?;
        Some(mapper(layer))
    }

    #[must_use]
    fn with_ctx<F, T>(&self, f: F) -> Option<T>
    where
        S: Subscriber,
        F: FnOnce(Context<'_, subscriber::ReloadSubscriber<S>>) -> T,
        T: 'static,
    {
        type DynFnOnce<'a, S, T> =
            dyn FnOnce(Context<'_, subscriber::ReloadSubscriber<S>>) -> T + 'a;
        struct ContextStealer<S: 'static, T>(
            Cell<Option<Box<DynFnOnce<'static, S, T>>>>,
            Cell<Option<T>>,
        );

        impl<S: Subscriber, T> tracing_subscriber::Layer<subscriber::ReloadSubscriber<S>>
            for ContextStealer<S, T>
        where
            T: 'static,
        {
            // There's no reason we use this method specifically, we just need one that uses
            // `Context` and preferably one which takes arguments that are easy to construct.
            fn on_follows_from(
                &self,
                _span: &span::Id,
                _follows: &span::Id,
                ctx: Context<'_, subscriber::ReloadSubscriber<S>>,
            ) {
                #[expect(
                    clippy::unwrap_used,
                    reason = "We set `self.0` before calling this in this function."
                )]
                self.1.set(Some(self.0.replace(None).unwrap()(ctx)));
            }
        }

        self.inner.with_subscriber(|subscriber| {
            // SAFETY
            // We do this just so that `ReloadSubscriber: 'static`. We need to make sure that no one
            // stashes the static reference to use it after this method finishes.
            let inner_subscriber = unsafe { std::mem::transmute::<&'_ S, &'static S>(subscriber) };
            let subscriber = subscriber::ReloadSubscriber::new(
                inner_subscriber,
                try_lock!(self.inner.filters.lock(), else return None).clone(),
            );

            // TODO is there a better way to make `F` into `F + 'static` without boxing?
            let boxed: Box<DynFnOnce<'_, S, T>> = Box::new(f);

            // SAFETY
            // The whole `'static` bound exists just so we can use the function inside the `Layer`
            // which has to be `'static` same as the `Subscriber` itself. We call the
            // closure will outlive this function and is called inside this function and
            // then it's gone afterwards so this is safe to do and call.
            let transmuted = unsafe {
                std::mem::transmute::<Box<DynFnOnce<'_, S, T>>, Box<DynFnOnce<'static, S, T>>>(
                    boxed,
                )
            };

            let layered = subscriber.with(ContextStealer::<S, T>(
                Cell::new(Some(transmuted)),
                Cell::new(None),
            ));
            // The span IDs are ignored, just pass anything inside. `Registry` doesn't do anything
            // with this and the only layer is the one that we use to steal the context. We make
            // sure not to propagate this first call to the inner subscriber.
            layered
                .record_follows_from(&span::Id::from_u64(u64::MAX), &span::Id::from_u64(u64::MAX));

            #[expect(
                clippy::unwrap_used,
                reason = "We registered tis layer and its second field is set unconditionally in \
                          the `on_follows_from` implementation."
            )]
            let result = layered
                .downcast_ref::<ContextStealer<S, T>>()
                .unwrap()
                .1
                .replace(None)
                .unwrap();

            // Check invariants after the unsafe usage. The reverse drop order at the end of the
            // function would handle this the same way, but let's be explicit.
            drop(layered);

            Some(result)
        })?
    }
}

// ===== impl Handle =====

impl<L, S> Handle<L, S> {
    /// Replaces the active layer with a new one.
    ///
    /// The closure provides a reference to the active layer so if the layer implements `Clone`, it
    /// can be constructed that way and then modified in place.
    pub fn reload_with(&self, f: impl FnOnce(&L) -> L) -> Result<(), Error>
    where
        L: tracing_subscriber::Layer<ReloadSubscriber<S>>,
        S: Subscriber,
    {
        let inner = self.inner.upgrade().ok_or_else(Error::subscriber_gone)?;

        inner
            .with_subscriber(|subscriber| {
                // SAFETY
                // We do this just so that `ReloadSubscriber: 'static`. We need to make sure that no
                // one stashes the static reference to use it after this method finishes.
                let inner_subscriber =
                    unsafe { std::mem::transmute::<&'_ S, &'static S>(subscriber) };

                let mut layers =
                    try_lock!(inner.layers.write(), else return Err(Error::poisoned()));
                let mut next = f(&layers.latest);

                let mut filters =
                    try_lock!(inner.filters.lock(), else return Err(Error::poisoned()));

                let mut subscriber = ReloadSubscriber::new(inner_subscriber, filters.clone());

                next.on_layer(&mut subscriber);
                next.on_register_dispatch(
                    &inner
                        .dispatch
                        .get()
                        .ok_or(Error::subscriber_not_initialized())?
                        .upgrade()
                        .ok_or(Error::subscriber_gone())?,
                );

                *filters = subscriber.into_filters();

                layers.push(next);
                drop(layers);

                callsite::rebuild_interest_cache();

                #[cfg(feature = "tracing-log")]
                tracing_log::log::set_max_level(tracing_log::AsLog::as_log(
                    &tracing_subscriber::filter::LevelFilter::current(),
                ));

                Ok(())
            })
            .ok_or(Error::subscriber_not_initialized())?
    }

    /// Replaces the active layer with a new one.
    ///
    /// Depending on configuration, new spans and events may still be routed to the older layer.
    pub fn reload(&self, layer: impl Into<L>) -> Result<(), Error>
    where
        L: tracing_subscriber::Layer<ReloadSubscriber<S>>,
        S: Subscriber,
    {
        self.reload_with(|_| layer.into())
    }

    /// Invokes a closure with a borrowed reference to the current layer slice, returning the
    /// result.
    pub fn with_current<T>(&self, f: impl FnOnce(&L) -> T) -> Result<T, Error> {
        let inner = self.inner.upgrade().ok_or_else(Error::subscriber_gone)?;
        let layers = try_lock!(inner.layers.read(), else return Err(Error::poisoned()));
        Ok(f(&layers.latest))
    }
}

impl<L, S> Clone for Handle<L, S> {
    fn clone(&self) -> Self {
        Handle {
            inner: self.inner.clone(),
            _s: PhantomData,
        }
    }
}

impl<L> Shared<L> {
    fn with_subscriber<S, T, F>(&self, fun: F) -> Option<T>
    where
        F: FnOnce(&S) -> T,
        S: 'static,
    {
        let dispatch = self.dispatch.get()?.upgrade()?;

        let subscriber = dispatch.downcast_ref::<S>()?;
        let result = fun(subscriber);

        Some(result)
    }
}

#[cfg(test)]
mod tests {
    use tracing::Level;
    use tracing_mock::{expect, layer as mock_layer};
    use tracing_subscriber::{filter::LevelFilter, registry::Registry};

    use super::*;

    #[test]
    fn modify_while_subscriber_active() {
        fn emit_event(level: tracing::Level, id: u8) {
            match level {
                tracing::Level::ERROR => tracing::error!("event {}", id),
                tracing::Level::WARN => tracing::warn!("event {}", id),
                tracing::Level::INFO => tracing::info!("event {}", id),
                tracing::Level::DEBUG => tracing::debug!("event {}", id),
                tracing::Level::TRACE => tracing::trace!("event {}", id),
            }
        }

        let (mock, handle) = mock_layer::mock()
            .event(
                expect::event()
                    .at_level(Level::ERROR)
                    .with_fields(expect::msg("event 0")),
            )
            .event(
                expect::event()
                    .at_level(Level::WARN)
                    .with_fields(expect::msg("event 1")),
            )
            .event(
                expect::event()
                    .at_level(Level::ERROR)
                    .with_fields(expect::msg("event 3")),
            )
            .only()
            .run_with_handle();

        let (reload, reload_handle) = Layer::new(LevelFilter::TRACE);
        let subscriber = Registry::default().with(reload).with(mock);

        tracing::subscriber::with_default(subscriber, || {
            emit_event(tracing::Level::ERROR, 0);
            emit_event(tracing::Level::WARN, 1);

            reload_handle.reload_with(|_| LevelFilter::ERROR).unwrap();

            emit_event(tracing::Level::INFO, 2);
            emit_event(tracing::Level::ERROR, 3);
        });

        handle.assert_finished();
    }

    #[test]
    fn with_current_borrows() {
        let (reload, handle) = Layer::new(LevelFilter::INFO);
        let subscriber = Registry::default().with(reload);
        tracing::subscriber::with_default(subscriber, || {
            let val = handle
                .with_current(|layer| layer.into_level().unwrap())
                .unwrap();
            assert_eq!(val, Level::INFO);

            handle.reload(LevelFilter::ERROR).unwrap();

            let val = handle
                .with_current(|layer| layer.into_level().unwrap())
                .unwrap();
            assert_eq!(val, Level::ERROR);
        });
    }

    #[test]
    fn handle_errors_after_subscriber_dropped() {
        let (reload, handle) = Layer::new(LevelFilter::OFF);
        let subscriber = Registry::default().with(reload);
        tracing::subscriber::with_default(subscriber, || {});

        assert!(handle.with_current(|_| ()).is_err());
        assert!(handle.reload_with(|_current| LevelFilter::OFF).is_err());
    }

    #[test]
    fn downcast_survives_reload_with_snapshot_semantics() {
        let (layer, handle) = Layer::new(LevelFilter::TRACE);

        // We need to do initialization of the layer
        let dispatch = tracing::Dispatch::new(tracing_subscriber::registry());
        <Layer<LevelFilter, _> as tracing_subscriber::Layer<Registry>>::on_register_dispatch(
            &layer, &dispatch,
        );

        let before = unsafe {
            <Layer<LevelFilter, _> as tracing_subscriber::Layer<Registry>>::downcast_raw(
                &layer,
                core::any::TypeId::of::<LevelFilter>(),
            )
        }
        .unwrap()
        .cast::<LevelFilter>();
        assert_eq!(unsafe { *before }, LevelFilter::TRACE);

        handle.reload(LevelFilter::DEBUG).unwrap();

        assert_eq!(unsafe { *before }, LevelFilter::TRACE);

        let after = unsafe {
            <Layer<LevelFilter, _> as tracing_subscriber::Layer<Registry>>::downcast_raw(
                &layer,
                core::any::TypeId::of::<LevelFilter>(),
            )
        }
        .unwrap()
        .cast::<LevelFilter>();
        assert_eq!(unsafe { *after }, LevelFilter::DEBUG);

        // This should still be valid even after getting the new pointer.
        assert_eq!(unsafe { *before }, LevelFilter::TRACE);
    }

    #[test]
    fn parent_layer_routing() {
        let parent = expect::span()
            .named("parent")
            .at_level(tracing::Level::INFO);
        let child = expect::span().named("child").at_level(tracing::Level::INFO);

        let (mock1, handle1) = mock_layer::mock()
            .new_span(parent.clone())
            .enter(&parent)
            .new_span(child.clone())
            .enter(&child)
            .exit(&child)
            .exit(&parent)
            .only()
            .run_with_handle();

        let (mock2, handle2) = mock_layer::mock().only().run_with_handle();

        let (reload, reload_handle) =
            Layer::new_with_routing(mock1, ReloadRouting::NewSpanToParentLayer);
        let subscriber = Registry::default().with(reload);

        tracing::subscriber::with_default(subscriber, || {
            let _parent = tracing::info_span!("parent").entered();

            reload_handle.reload(mock2).unwrap();

            _ = tracing::info_span!("child").entered();
        });

        handle2.assert_finished();
        handle1.assert_finished();
    }

    #[test]
    fn current_layer_routing() {
        let parent = expect::span()
            .named("parent")
            .at_level(tracing::Level::INFO);
        let child = expect::span().named("child").at_level(tracing::Level::INFO);

        let (mock1, handle1) = mock_layer::mock()
            .new_span(parent.clone())
            .enter(&parent)
            .exit(&parent)
            .only()
            .run_with_handle();

        let (mock2, handle2) = mock_layer::mock()
            .new_span(child.clone())
            .enter(&child)
            .exit(&child)
            .only()
            .run_with_handle();

        let (reload, reload_handle) =
            Layer::new_with_routing(mock1, ReloadRouting::NewSpanToLatestLayer);
        let subscriber = Registry::default().with(reload);

        tracing::subscriber::with_default(subscriber, || {
            let _parent = tracing::info_span!("parent").entered();

            reload_handle.reload(mock2).unwrap();

            _ = tracing::info_span!("child").entered();
        });

        handle2.assert_finished();
        handle1.assert_finished();
    }
}
