use std::{any::TypeId, collections::HashMap, ptr};

use crate::ReloadSubscriber;

#[derive(Debug, Default)]
pub(crate) struct SubscriberCache(HashMap<(TypeId, *const ()), *const ()>);

// SAFETY we use this only with subscribers which are `Send` and therefore it's fine to pass the
// pointers to other threads.
unsafe impl Send for SubscriberCache {}

// SAFETY we use this only with subscribers which are `Sync` and therefore it's fine to pass the
// pointers to other threads.
unsafe impl Sync for SubscriberCache {}

impl SubscriberCache {
    pub(crate) fn get<S>(&mut self, type_id: TypeId, inner: &'static S) -> *const () {
        let ptr = self
            .0
            .entry((type_id, ptr::from_ref(inner).cast::<()>()))
            .or_insert_with(|| {
                let subscriber = ReloadSubscriber::new(inner, crate::ReloadableFilters::new());
                let ptr = Box::leak(Box::new(subscriber));

                ptr::from_ref(ptr).cast::<()>()
            });

        *ptr
    }
}
