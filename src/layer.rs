#![expect(clippy::unwrap_used, reason = "This file needs a bit more cleanup.")]

use std::{ptr, sync::atomic::Ordering};

use tracing_core::{
    Event,
    LevelFilter,
    Metadata,
    span,
    subscriber::{Interest, Subscriber},
};
use tracing_subscriber::{layer, registry::LookupSpan};

use crate::{Layer, ReloadRouting, subscriber::ReloadSubscriber};

impl<L, S> tracing_subscriber::Layer<S> for Layer<L, S>
where
    S: Subscriber + for<'lookup> LookupSpan<'lookup>,
    L: tracing_subscriber::Layer<ReloadSubscriber<S>>,
{
    fn on_layer(&mut self, subscriber: &mut S) {
        let mut filters = try_lock!(self.inner.filters.lock());
        filters.backup = subscriber.register_filter();
        for _ in 1..crate::MAX_FILTER_COUNT.get() {
            filters.free.push(subscriber.register_filter());
        }

        // SAFETY
        // We hold the subscriber just until the end of this function. The `'static` extension is
        // needed just so `ReloadSubscriber: 'static` but we do not give anyone else direct access
        // to the reference inside and pass the fake subscriber with a set lifetime so it itself
        // cannot outlive that method call. And it is dropped at the end of this method along with
        // the extended reference so it's ok.
        let inner_subscriber = unsafe { std::mem::transmute::<&'_ S, &'static S>(&*subscriber) };

        let mut fake_subscriber = ReloadSubscriber::new(inner_subscriber, filters.clone());

        #[expect(clippy::unwrap_used, reason = "There is always at least one layer.")]
        try_lock!(self.inner.layers.write())
            .last_mut()
            .unwrap()
            .as_mut()
            .on_layer(&mut fake_subscriber);
    }

    fn on_register_dispatch(&self, subscriber: &tracing_core::Dispatch) {
        // This should be called just once. If it somehow isn't, we panic because in `downcast_raw`
        // we rely on having the correct `Dispatch`. This isn't foolproof, but eliminates some
        // potential issues.
        #[expect(
            clippy::expect_used,
            reason = "Panicking here might prevent worse bugs. This should only be called during \
                      startup."
        )]
        self.inner
            .dispatch
            .set(subscriber.downgrade())
            .expect("on_register_dispatch should be called only once");

        <L as layer::Layer<ReloadSubscriber<S>>>::on_register_dispatch(
            #[expect(clippy::unwrap_used, reason = "There is always at least one layer.")]
            try_lock!(self.inner.layers.read()).last().unwrap(),
            subscriber,
        );
    }

    #[inline]
    fn register_callsite(&self, metadata: &'static Metadata<'static>) -> Interest {
        <L as layer::Layer<ReloadSubscriber<S>>>::register_callsite(
            #[expect(clippy::unwrap_used, reason = "There is always at least one layer.")]
            try_lock!(self.inner.layers.read(), else return Interest::sometimes())
                .last()
                .unwrap(),
            metadata,
        )
    }

    #[inline]
    fn enabled(&self, metadata: &Metadata<'_>, _ctx: layer::Context<'_, S>) -> bool {
        #[expect(
            clippy::expect_used,
            reason = "TODO when can this happen and is sentinel return value better than \
                      panicking."
        )]
        self.with_ctx(move |ctx| {
            #[expect(clippy::unwrap_used, reason = "There is always at least one layer.")]
            try_lock!(self.inner.layers.read(), else return false)
                .last()
                .unwrap()
                .enabled(metadata, ctx)
        })
        .expect("could not get value for enabled")
    }

    #[inline]
    fn event_enabled(&self, event: &Event<'_>, _ctx: layer::Context<'_, S>) -> bool {
        #[expect(
            clippy::expect_used,
            reason = "TODO when can this happen and is sentinel return value better than \
                      panicking."
        )]
        self.with_ctx(move |ctx| {
            #[expect(clippy::unwrap_used, reason = "There is always at least one layer.")]
            try_lock!(self.inner.layers.read(), else return false)
                .last()
                .unwrap()
                .event_enabled(event, ctx)
        })
        .expect("could not get value for event_enabled")
    }

    #[inline]
    fn max_level_hint(&self) -> Option<LevelFilter> {
        <L as layer::Layer<ReloadSubscriber<S>>>::max_level_hint(
            #[expect(clippy::unwrap_used, reason = "There is always at least one layer.")]
            try_lock!(self.inner.layers.read(), else return None)
                .last()
                .unwrap(),
        )
    }

    #[inline]
    fn on_new_span(&self, attrs: &span::Attributes<'_>, id: &span::Id, ctx: layer::Context<'_, S>) {
        let index = {
            let layers = try_lock!(self.inner.layers.read());
            let index = match self.span_routing {
                ReloadRouting::NewSpanToLatestLayer => layers.len() - 1,
                ReloadRouting::NewSpanToParentLayer => {
                    let parent_id = attrs.parent().cloned().or_else(|| {
                        attrs
                            .is_contextual()
                            .then_some(ctx.current_span().id().cloned())
                            .flatten()
                    });
                    parent_id
                        .and_then(|parent_id| {
                            let span_map = try_lock!(self.inner.span_map.read(), else return None);
                            span_map.get(&parent_id).copied()
                        })
                        .unwrap_or(layers.len() - 1)
                },
            };
            self.with_ctx(move |ctx| {
                layers.get(index).unwrap().on_new_span(attrs, id, ctx);
            })
            .unwrap();
            index
        };
        let mut span_map = try_lock!(self.inner.span_map.write());
        span_map.insert(id.clone(), index);
    }

    #[inline]
    fn on_record(&self, span: &span::Id, values: &span::Record<'_>, _ctx: layer::Context<'_, S>) {
        self.with_span(span, |layer| {
            self.with_ctx(move |ctx| layer.on_record(span, values, ctx))
        });
    }

    #[inline]
    fn on_follows_from(&self, span: &span::Id, follows: &span::Id, _ctx: layer::Context<'_, S>) {
        let idx = {
            let span_map = try_lock!(self.inner.span_map.read());
            span_map.get(span).copied()
        };
        if let Some(idx) = idx {
            let current = try_lock!(self.inner.layers.read());
            if let Some(layer) = current.get(idx) {
                self.with_ctx(move |ctx| {
                    layer.on_follows_from(span, follows, ctx);
                });
            }
        }
    }

    #[inline]
    fn on_event(&self, event: &Event<'_>, ctx: layer::Context<'_, S>) {
        let span_id = event
            .parent()
            .cloned()
            .or_else(|| ctx.current_span().id().cloned());

        if let Some(span_id) = span_id {
            self.with_span(&span_id, |layer| {
                self.with_ctx(move |ctx| layer.on_event(event, ctx))
            });
        } else {
            self.with_ctx(move |ctx| {
                #[expect(clippy::unwrap_used, reason = "There is always at least one layer.")]
                try_lock!(self.inner.layers.read())
                    .last()
                    .unwrap()
                    .on_event(event, ctx);
            });
        }
    }

    #[inline]
    fn on_enter(&self, id: &span::Id, _ctx: layer::Context<'_, S>) {
        self.with_span(id, |layer| {
            self.with_ctx(move |ctx| layer.on_enter(id, ctx))
        });
    }

    #[inline]
    fn on_exit(&self, id: &span::Id, _ctx: layer::Context<'_, S>) {
        self.with_span(id, |layer| self.with_ctx(move |ctx| layer.on_exit(id, ctx)));
    }

    #[inline]
    fn on_close(&self, id: span::Id, _ctx: layer::Context<'_, S>) {
        let index = {
            let mut span_map = try_lock!(self.inner.span_map.write());
            span_map.remove(&id).unwrap()
        };
        self.with_ctx(move |ctx| {
            try_lock!(self.inner.layers.read())
                .get(index)
                .unwrap()
                .on_close(id, ctx);
        });
    }

    #[inline]
    fn on_id_change(&self, old: &span::Id, new: &span::Id, _ctx: layer::Context<'_, S>) {
        let index = {
            let mut span_map = try_lock!(self.inner.span_map.write());
            let index = span_map.remove(old).unwrap();
            span_map.insert(new.clone(), index);
            index
        };
        self.with_ctx(move |ctx| {
            try_lock!(self.inner.layers.read())
                .get(index)
                .unwrap()
                .on_id_change(old, new, ctx);
        });
    }

    #[doc(hidden)]
    unsafe fn downcast_raw(&self, id: core::any::TypeId) -> Option<*const ()> {
        if id == core::any::TypeId::of::<Self>() {
            return Some(ptr::from_ref(self).cast::<()>());
        }

        if id == core::any::TypeId::of::<ReloadSubscriber<S>>() {
            let dispatch = self.inner.dispatch.get()?.upgrade()?;

            // See the SAFETY block after this.
            if !self.inner.dispatch_leaked.swap(true, Ordering::Relaxed) {
                std::mem::forget(dispatch.clone());
            }

            let subscriber = dispatch.downcast_ref::<S>()?;

            // SAFETY The scenario that is most likely happening is that a layer wrapped in
            // `tracing_reload::Layer` has a `Dispatch` and wants to downcast to
            // `ReloadSubscriber<S>`. That `Dispatch` downcasts first to `dyn Subscriber` which is
            // actually `S`, most likely some kind of `Layered`. It doesn't recognize the `TypeId`
            // so it start calling downcast methods on the wrapped layers. And then it got here.
            // Now, if we have the same `Dispatch` (and each `Subscriber` is owned by a single
            // `Dispatch` so that should hold), we should be safe since we can downcast and be sure
            // that the pointer (and thus the reference) is valid for at least as long as the
            // original caller holds the `Dispatch`.
            //
            // Where this could be unsound is if someone holds a reference to this `Layer`, provides
            // it with _another_ `Dispatch` through `on_register_dispatch`, then downcasts to
            // `ReloadSubscriber<S>` and drops their `Dispatch`, dropping also `S` and invalidating
            // the pointer inside the struct pointed to by the pointer we have given out. The only
            // way out of this I see is not holding just `WeakDispatch`, but the full one so that it
            // can't ever be dropped. That's another leak though. So that's what we've done just a
            // few lines higher.
            let static_subscriber = unsafe { std::mem::transmute::<&'_ S, &'static S>(subscriber) };

            return Some(
                try_lock!(self.subscriber_cache.lock(), else return None)
                    .get(id, static_subscriber),
            );
        }

        let current = try_lock!(self.inner.layers.read(), else return None);

        current.last().and_then(|layer| unsafe {
            <L as layer::Layer<ReloadSubscriber<S>>>::downcast_raw(layer, id)
        })
    }
}
