//! This module defines a fake subscriber which is not to be constructible from user code and thus
//! cannot be registered as an actual subscriber. We do this just so we have something implementing
//! [`LookupSpan`] that the reloaded layers can be provided in their [`on_layer`] implementation.
//!
//! [`on_layer`]: tracing_subscriber::layer::Layer::on_layer

use std::{
    any::TypeId, ptr, sync::atomic::{AtomicBool, Ordering}
};

use tracing::{Subscriber, span};
use tracing_subscriber::registry::LookupSpan;

use crate::ReloadableFilters;

pub struct ReloadSubscriber<S>
where
    S: 'static,
{
    inner: &'static S,
    filters: ReloadableFilters,
    follows_from_called: AtomicBool,
}

impl<S: 'static> ReloadSubscriber<S> {
    pub(crate) fn new(inner: &'static S, filters: ReloadableFilters) -> Self {
        Self {
            inner,
            filters,
            follows_from_called: AtomicBool::new(false),
        }
    }

    pub(crate) fn into_filters(self) -> ReloadableFilters {
        self.filters
    }
}

impl<S: Subscriber + 'static> Subscriber for ReloadSubscriber<S> {
    fn record_follows_from(&self, span: &span::Id, follows: &span::Id) {
        // The first call is our fake call from `with_ctx`. We do not want this to propagate to the
        // inner subscriber and especially to any other layers it wraps.
        if self.follows_from_called.swap(true, Ordering::Relaxed) {
            self.inner.record_follows_from(span, follows);
        }
    }

    unsafe fn downcast_raw(&self, id: TypeId) -> Option<*const ()> {
        if id == TypeId::of::<Self>() {
            Some(ptr::from_ref(self).cast::<()>())
        } else {
            unsafe { self.inner.downcast_raw(id) }
        }
    }

    // Everything else is `fn x(..) -> y { self.inner.x(..) }`

    fn enabled(&self, metadata: &tracing::Metadata<'_>) -> bool {
        self.inner.enabled(metadata)
    }

    fn new_span(&self, span: &span::Attributes<'_>) -> tracing_core::span::Id {
        self.inner.new_span(span)
    }

    fn record(&self, span: &span::Id, values: &tracing_core::span::Record<'_>) {
        self.inner.record(span, values);
    }

    fn event(&self, event: &tracing::Event<'_>) {
        self.inner.event(event);
    }

    fn enter(&self, span: &span::Id) {
        self.inner.enter(span);
    }

    fn exit(&self, span: &span::Id) {
        self.inner.exit(span);
    }

    fn on_register_dispatch(&self, subscriber: &tracing::Dispatch) {
        self.inner.on_register_dispatch(subscriber);
    }

    fn register_callsite(
        &self,
        metadata: &'static tracing::Metadata<'static>,
    ) -> tracing_core::Interest {
        self.inner.register_callsite(metadata)
    }

    fn max_level_hint(&self) -> Option<tracing_core::LevelFilter> {
        self.inner.max_level_hint()
    }

    fn event_enabled(&self, event: &tracing::Event<'_>) -> bool {
        self.inner.event_enabled(event)
    }

    fn clone_span(&self, id: &tracing_core::span::Id) -> tracing_core::span::Id {
        self.inner.clone_span(id)
    }

    fn drop_span(&self, id: tracing_core::span::Id) {
        #[expect(deprecated, reason = "It's deprecated...")]
        self.inner.drop_span(id);
    }

    fn try_close(&self, id: tracing_core::span::Id) -> bool {
        self.inner.try_close(id)
    }

    fn current_span(&self) -> tracing_core::span::Current {
        self.inner.current_span()
    }
}

impl<'lookup, S: LookupSpan<'lookup>> LookupSpan<'lookup> for ReloadSubscriber<S> {
    type Data = S::Data;

    fn span_data(&'lookup self, id: &tracing::span::Id) -> Option<Self::Data> {
        self.inner.span_data(id)
    }

    fn register_filter(&mut self) -> tracing_subscriber::filter::FilterId {
        let filters = &mut self.filters;

        let Some(filter_id) = filters.free.pop() else {
            // We don't have enough filter IDs. If we don't give out anything the filter would panic
            // so we just give it _anything_ which may break some filters but at least it doesn't
            // panic. Hopefully.

            return filters.backup;
        };

        filters.used.push(filter_id);
        filter_id
    }
}
