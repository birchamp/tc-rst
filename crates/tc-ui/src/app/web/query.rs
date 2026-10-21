//! The query cache's glue (APP-DESIGN 6.2): it holds the values and runs the fetches, and the
//! [`Registry`] decides every other thing. Its first users are Home's reads (B2), whose
//! [`CachedQuery`] rows are at the end; this file stays thin on purpose.

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
    AppTopic, CachedQuery, Held, Landed, Outcome, QueryId, ReadId, Registry, Start, Ticket,
};
use crate::app::client::{CallError, Transport};
use crate::app::load::Load;

/// The reads screens hold, shared by request. Cheap to clone.
#[derive(Clone)]
pub struct QueryCache(Rc<Inner>);

struct Inner {
    api: Api,
    registry: RefCell<Registry>,
    /// A slot for each read the registry holds, by the id it gave: two server runs' reads of
    /// one request are two slots.
    slots: RefCell<HashMap<ReadId, Rc<dyn Slot>>>,
}

/// A read's value and the way to fetch it, without its type.
trait Slot {
    fn into_any(self: Rc<Self>) -> Rc<dyn Any>;
    /// Starts the fetch `ticket`.
    fn begin(self: Rc<Self>, cache: QueryCache, id: ReadId, ticket: Ticket);
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

    fn begin(self: Rc<Self>, cache: QueryCache, id: ReadId, ticket: Ticket) {
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
        // The registry says which read this is: a request that a dead read of an earlier server
        // run still holds is a new read, with a slot of its own.
        let (read, held) = self.0.registry.borrow_mut().hold(&id, request.read_of());
        let existing = self.0.slots.borrow().get(&read).cloned();
        let slot = match existing {
            Some(slot) => match slot.into_any().downcast::<TypedSlot<Q>>() {
                Ok(slot) => slot,
                Err(_) => {
                    self.release(&read);
                    return failed(format!("{} is read as two types", id.path));
                }
            },
            None => {
                let slot = Rc::new(TypedSlot {
                    request: request.clone(),
                    value: ArcRwSignal::new(Load::<Arc<Q::Ok>>::Loading),
                });
                let erased: Rc<dyn Slot> = slot.clone();
                self.0.slots.borrow_mut().insert(read.clone(), erased);
                slot
            }
        };
        match held {
            Held::Fetch(ticket) => self.start(&read, ticket),
            Held::Shared => {}
            Held::Gone => slot.dropped(),
        }
        let (cache, held_read) = (SendWrapper::new(self.clone()), SendWrapper::new(read));
        on_cleanup(move || cache.release(&held_read));
        let value = slot.value.clone();
        Signal::derive(move || value.get())
    }

    /// A Retry after a failure: fetches the read again.
    pub fn refresh<Q: CachedQuery>(&self, request: &Q) {
        let Ok(id) = QueryId::of(request) else {
            return;
        };
        let start = self.0.registry.borrow_mut().refresh(&id);
        if let Some(Start { id, ticket }) = start {
            self.start(&id, ticket);
        }
    }

    fn release(&self, id: &ReadId) {
        if self.0.registry.borrow_mut().release(id) {
            self.0.slots.borrow_mut().remove(id);
        }
    }

    fn start(&self, id: &ReadId, ticket: Ticket) {
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

    fn drop_reads(&self, ids: &[ReadId]) {
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

impl CachedQuery for tc_api::home::List {
    /// Home: read again when the projects change.
    fn read_of(&self) -> crate::app::cache::ReadOf {
        crate::app::cache::ReadOf::App(vec![AppTopic::Projects])
    }
}

impl CachedQuery for tc_api::home::Card {
    /// One card: read again when its summary is computed or changes.
    fn read_of(&self) -> crate::app::cache::ReadOf {
        crate::app::cache::ReadOf::App(vec![AppTopic::HomeCard(self.project.clone())])
    }
}

/// The project reads of Understand (B3), each read again by the changes its value is made of.
fn project_read(at: &Scope, keys: Vec<tc_api::query::QueryKey>) -> crate::app::cache::ReadOf {
    crate::app::cache::ReadOf::Project {
        at: at.clone(),
        keys,
    }
}

impl CachedQuery for tc_api::editor::Book {
    /// The book's chapters, drafted share and comments: any change in the book.
    fn read_of(&self) -> crate::app::cache::ReadOf {
        project_read(&self.at, vec![tc_api::query::QueryKey::Book(self.book)])
    }
}

/// The resources a book's panes and helps are read from follow the pins and the helps
/// setting.
fn resource_keys() -> Vec<tc_api::query::QueryKey> {
    use tc_api::event::Aspect;
    vec![
        tc_api::query::QueryKey::Aspect(Aspect::Pins),
        tc_api::query::QueryKey::Aspect(Aspect::Settings),
    ]
}

impl CachedQuery for tc_api::editor::Chapters {
    /// Translate's text: any change in the book (B4a), and the project's conflicts, which
    /// `ChaptersView::conflicts` counts across every book.
    fn read_of(&self) -> crate::app::cache::ReadOf {
        use tc_api::{event::Aspect, query::QueryKey};
        project_read(
            &self.at,
            vec![
                QueryKey::Book(self.book),
                QueryKey::Aspect(Aspect::Conflicts),
            ],
        )
    }

    fn view(ok: &Self::Ok) -> Option<tc_api::ids::ViewSeq> {
        Some(ok.view)
    }
}

impl CachedQuery for tc_api::editor::Sources {
    fn read_of(&self) -> crate::app::cache::ReadOf {
        project_read(&self.at, resource_keys())
    }
}

impl CachedQuery for tc_api::helps::Passage {
    fn read_of(&self) -> crate::app::cache::ReadOf {
        project_read(&self.at, resource_keys())
    }
}

impl CachedQuery for tc_api::helps::Article {
    fn read_of(&self) -> crate::app::cache::ReadOf {
        project_read(&self.at, resource_keys())
    }
}

/// The Check picker (B5a): both tools' lists, the book's verses for the Align card (a story has
/// none: its own text), and the pins the preflight reads.
impl CachedQuery for tc_api::check::Picker {
    fn read_of(&self) -> crate::app::cache::ReadOf {
        use tc_api::check::{CheckListRef, CheckUnit, Tool};
        use tc_api::event::Aspect;
        use tc_api::query::QueryKey;
        let list = |tool| {
            QueryKey::CheckList(CheckListRef {
                tool,
                unit: self.unit,
            })
        };
        let text = match self.unit {
            CheckUnit::Book(book) => QueryKey::Book(book),
            CheckUnit::Story(story) => QueryKey::Story(story),
        };
        project_read(
            &self.at,
            vec![
                list(Tool::TranslationWords),
                list(Tool::TranslationNotes),
                text,
                QueryKey::Aspect(Aspect::Pins),
            ],
        )
    }
}

/// The stories' rail (B7): every story's row follows any change of a story.
impl CachedQuery for tc_api::story::List {
    fn read_of(&self) -> crate::app::cache::ReadOf {
        project_read(
            &self.at,
            (1..=tc_api::ids::StoryNo::COUNT)
                .filter_map(|n| tc_api::ids::StoryNo::new(n).ok())
                .map(tc_api::query::QueryKey::Story)
                .collect(),
        )
    }
}

/// One story (B7): its text and its comments, the pictures and the gateway story its pins give.
impl CachedQuery for tc_api::story::Open {
    fn read_of(&self) -> crate::app::cache::ReadOf {
        let mut keys = resource_keys();
        keys.push(tc_api::query::QueryKey::Story(self.story));
        project_read(&self.at, keys)
    }
}

impl CachedQuery for tc_api::helps::Story {
    fn read_of(&self) -> crate::app::cache::ReadOf {
        project_read(&self.at, resource_keys())
    }
}

impl CachedQuery for tc_api::helps::StoryArticle {
    fn read_of(&self) -> crate::app::cache::ReadOf {
        project_read(&self.at, resource_keys())
    }
}

/// A tool's rail (B5a) is read once per session and then patched by row (AD17): these reads are
/// made by the Check screen's actions, never kept in the cache.
impl CachedQuery for tc_api::check::Open {
    fn read_of(&self) -> crate::app::cache::ReadOf {
        project_read(
            &self.at,
            vec![tc_api::query::QueryKey::CheckList(self.list)],
        )
    }
}

impl CachedQuery for tc_api::check::Rows {
    fn read_of(&self) -> crate::app::cache::ReadOf {
        project_read(
            &self.at,
            vec![tc_api::query::QueryKey::CheckList(self.list)],
        )
    }
}

impl CachedQuery for tc_api::resources::FixState {
    fn read_of(&self) -> crate::app::cache::ReadOf {
        project_read(&self.at, resource_keys())
    }
}

/// A check's detail and its article (B5b): read by the Check screen's actions when a check is
/// selected or changed, never kept in the cache.
impl CachedQuery for tc_api::check::Item {
    fn read_of(&self) -> crate::app::cache::ReadOf {
        project_read(
            &self.at,
            vec![tc_api::query::QueryKey::CheckList(self.list)],
        )
    }
}

impl CachedQuery for tc_api::check::CheckArticleRead {
    fn read_of(&self) -> crate::app::cache::ReadOf {
        project_read(&self.at, resource_keys())
    }
}
