//! The query cache's glue (APP-DESIGN 6.2): it holds the values and runs the fetches, and the
//! [`Registry`] decides every other thing. No phase-0 endpoint is a cached read, so the first
//! user is the Home screen; this file stays thin on purpose.

use std::any::Any;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

use leptos::prelude::*;
use send_wrapper::SendWrapper;
use tc_api::ApiError;
use tc_api::event::ChangeScope;
use tc_api::ids::Scope;
use wasm_bindgen_futures::spawn_local;

use super::api::Api;
use crate::app::cache::{
    AppTopic, CachedQuery, Held, Landed, Outcome, QueryId, Registry, Start, Ticket,
};
use crate::app::client::{CallError, Transport};
use crate::app::load::Load;

/// The reads screens hold, shared by request. Cheap to clone.
#[derive(Clone)]
pub struct QueryCache(Rc<Inner>);

struct Inner {
    api: Api,
    registry: RefCell<Registry>,
    slots: RefCell<HashMap<QueryId, Rc<dyn Slot>>>,
}

/// A read's value and the way to fetch it, without its type.
trait Slot {
    fn into_any(self: Rc<Self>) -> Rc<dyn Any>;
    /// Starts the fetch `ticket`.
    fn begin(self: Rc<Self>, cache: QueryCache, id: QueryId, ticket: Ticket);
    /// The read's open ended.
    fn dropped(&self);
}

struct TypedSlot<Q: CachedQuery> {
    request: Q,
    value: ArcRwSignal<Load<Arc<Q::Ok>>>,
}

impl<Q: CachedQuery> Slot for TypedSlot<Q> {
    fn into_any(self: Rc<Self>) -> Rc<dyn Any> {
        self
    }

    fn begin(self: Rc<Self>, cache: QueryCache, id: QueryId, ticket: Ticket) {
        // What is on screen stays while the next value is fetched.
        self.value
            .update(|value| *value = std::mem::replace(value, Load::Loading).refreshing());
        spawn_local(async move {
            let result = cache.0.api.call(&self.request).await;
            let landed = match &result {
                Ok(ok) => Landed::Value(Q::view(ok)),
                Err(CallError::Api(ApiError::StaleScope { .. })) => Landed::StaleScope,
                Err(_) => Landed::Error,
            };
            let landing = cache.0.registry.borrow_mut().landed(&id, ticket, landed);
            match landing.outcome {
                Outcome::Apply => self.value.set(match result {
                    Ok(ok) => Load::Ready(Arc::new(ok)),
                    Err(error) => Load::Failed(error),
                }),
                Outcome::Drop => self.value.set(Load::Failed(CallError::Dropped)),
                Outcome::Keep if landing.then.is_none() => self
                    .value
                    .update(|value| *value = std::mem::replace(value, Load::Loading).settled()),
                Outcome::Keep | Outcome::Ignore => {}
            }
            cache.drop_reads(&landing.died);
            if let Some(next) = landing.then {
                cache.start(&id, next);
            }
        });
    }

    fn dropped(&self) {
        self.value.set(Load::Failed(CallError::Dropped));
    }
}

impl QueryCache {
    pub(super) fn new(api: Api) -> Self {
        Self(Rc::new(Inner {
            api,
            registry: RefCell::new(Registry::new()),
            slots: RefCell::new(HashMap::new()),
        }))
    }

    /// The read `request`, held while the calling reactive owner lives; the last holder to go
    /// forgets it. Equal requests share one fetch and one value.
    pub fn read<Q: CachedQuery>(&self, request: Q) -> Signal<Load<Arc<Q::Ok>>> {
        let failed = |reason: String| {
            let value = Load::Failed(CallError::Transport(Transport::Unreadable(reason)));
            Signal::derive(move || value.clone())
        };
        let id = match QueryId::of(&request) {
            Ok(id) => id,
            Err(error) => return failed(format!("the request does not serialize: {error}")),
        };
        let existing = self.0.slots.borrow().get(&id).cloned();
        let slot = match existing {
            Some(slot) => match slot.into_any().downcast::<TypedSlot<Q>>() {
                Ok(slot) => slot,
                Err(_) => return failed(format!("{} is read as two types", id.path)),
            },
            None => {
                let slot = Rc::new(TypedSlot {
                    request: request.clone(),
                    value: ArcRwSignal::new(Load::<Arc<Q::Ok>>::Loading),
                });
                let erased: Rc<dyn Slot> = slot.clone();
                self.0.slots.borrow_mut().insert(id.clone(), erased);
                slot
            }
        };
        let held = self.0.registry.borrow_mut().hold(&id, request.read_of());
        match held {
            Held::Fetch(ticket) => self.start(&id, ticket),
            Held::Shared => {}
            Held::Gone => slot.dropped(),
        }
        let (cache, held_id) = (SendWrapper::new(self.clone()), SendWrapper::new(id));
        on_cleanup(move || cache.release(&held_id));
        let value = slot.value.clone();
        Signal::derive(move || value.get())
    }

    /// A Retry after a failure: fetches the read again.
    pub fn refresh<Q: CachedQuery>(&self, request: &Q) {
        let Ok(id) = QueryId::of(request) else {
            return;
        };
        let ticket = self.0.registry.borrow_mut().refresh(&id);
        if let Some(ticket) = ticket {
            self.start(&id, ticket);
        }
    }

    fn release(&self, id: &QueryId) {
        if self.0.registry.borrow_mut().release(id) {
            self.0.slots.borrow_mut().remove(id);
        }
    }

    fn start(&self, id: &QueryId, ticket: Ticket) {
        let slot = self.0.slots.borrow().get(id).cloned();
        if let Some(slot) = slot {
            slot.begin(self.clone(), id.clone(), ticket);
        }
    }

    fn start_all(&self, starts: Vec<Start>) {
        for start in starts {
            self.start(&start.id, start.ticket);
        }
    }

    fn drop_reads(&self, ids: &[QueryId]) {
        for id in ids {
            let slot = self.0.slots.borrow().get(id).cloned();
            if let Some(slot) = slot {
                slot.dropped();
            }
        }
    }

    pub(super) fn changed(&self, at: &Scope, scope: &ChangeScope) {
        let starts = self.0.registry.borrow_mut().changed(at, scope);
        self.start_all(starts);
    }

    pub(super) fn topic(&self, topic: &AppTopic) {
        let starts = self.0.registry.borrow_mut().topic(topic);
        self.start_all(starts);
    }

    pub(super) fn resync(&self) {
        let starts = self.0.registry.borrow_mut().resync();
        self.start_all(starts);
    }

    pub(super) fn opened(&self, at: Scope) {
        let ended = self.0.registry.borrow_mut().opened(at);
        self.drop_reads(&ended);
    }

    pub(super) fn closed(&self, at: &Scope) {
        let ended = self.0.registry.borrow_mut().closed(at);
        self.drop_reads(&ended);
    }

    pub(super) fn restarted(&self) {
        let restarted = self.0.registry.borrow_mut().restarted();
        self.drop_reads(&restarted.dead);
        self.start_all(restarted.starts);
    }

    pub(super) fn is_current(&self, at: &Scope) -> bool {
        self.0.registry.borrow().is_current(at)
    }
}
