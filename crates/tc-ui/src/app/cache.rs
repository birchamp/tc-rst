//! Which reads the UI holds, and which of them a change makes stale (APP-DESIGN 6.2, 6.3).
//!
//! Every decision of the query cache is here, in plain values, so a host test can run any
//! interleaving of holds, changes, replies and closes. The browser glue only executes what
//! the [`Registry`] says: it starts the fetches it returns, and gives each reply to
//! [`Registry::landed`] to learn whether to show it.
//!
//! **A change is never hidden.** If a change reaches a read while its fetch is running, the
//! reply may have been computed before the change. The read therefore fetches again when that
//! reply lands, once, however many changes came. A reply that is older than the value on
//! screen, or that belongs to an open that has ended, is not shown. The cost of any call is
//! the number of live reads, never the size of a project (PERFORMANCE rule 1).

use std::collections::{HashMap, HashSet};

use tc_api::endpoint::Endpoint;
use tc_api::event::ChangeScope;
use tc_api::ids::{Epoch, ProjectRef, Scope, ViewSeq};
use tc_api::query::QueryKey;

/// A read, by endpoint and request: equal requests share one fetch and one value (APP-DESIGN
/// 6.2).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct QueryId {
    pub path: &'static str,
    /// The request as JSON, which is the same text for equal requests.
    pub body: String,
}

impl QueryId {
    pub fn of<E: Endpoint>(request: &E) -> Result<QueryId, serde_json::Error> {
        Ok(QueryId {
            path: E::PATH,
            body: serde_json::to_string(request)?,
        })
    }
}

/// What makes a read stale.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReadOf {
    /// A read of the app, such as Home: fetched again by these topics and by `Resync`.
    App(Vec<AppTopic>),
    /// A read of one open project: dropped when that open ends, fetched again by a `Changed`
    /// at `at` whose scope one of `keys` depends on (`QueryKey::depends_on`), and by `Resync`.
    Project { at: Scope, keys: Vec<QueryKey> },
}

/// A part of the app that is not a project's data.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum AppTopic {
    /// The list of projects: a project was created, imported or joined.
    Projects,
    /// One Home card's summary.
    HomeCard(ProjectRef),
}

/// A read a screen keeps in the cache. Each query endpoint a screen unit adds implements it.
pub trait CachedQuery: Endpoint<Ok: Send + Sync + 'static> + Clone {
    fn read_of(&self) -> ReadOf;

    /// The snapshot a reply was built from; an older one never replaces a newer one.
    fn view(_ok: &Self::Ok) -> Option<ViewSeq> {
        None
    }
}

/// One fetch attempt of one read. Tickets are never reused, so a reply that belongs to an
/// earlier attempt, or to a read that was forgotten, is recognised.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Ticket(u64);

/// A fetch to send now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Start {
    pub id: QueryId,
    pub ticket: Ticket,
}

/// What [`Registry::hold`] found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Held {
    /// The first holder of a live read: fetch it with this ticket.
    Fetch(Ticket),
    /// The read already exists: share its value. Nothing is fetched.
    Shared,
    /// A new read of an open that has already ended: nothing is fetched, and the value is
    /// `Failed(Dropped)`.
    Gone,
}

/// A reply, as the registry needs to know it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Landed {
    /// The value, with the snapshot it was built from if its type says.
    Value(Option<ViewSeq>),
    /// An error, other than `StaleScope`.
    Error,
    /// The server answered `ApiError::StaleScope`: the read's open has ended.
    StaleScope,
}

/// What to do with a reply.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// Show it: the value, or the error as `Failed`.
    Apply,
    /// Keep the value and end the refresh: this was the read's current attempt, but its reply
    /// is older than what is shown.
    Keep,
    /// Do nothing: the reply is not the read's current attempt (a duplicate, an earlier
    /// attempt, a forgotten read's).
    Ignore,
    /// The read's open has ended: its value becomes `Failed(Dropped)`.
    Drop,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Landing {
    pub outcome: Outcome,
    /// Another fetch of the same read, because a change reached it while this one ran.
    pub then: Option<Ticket>,
    /// Other reads of the same open that a `StaleScope` ended.
    pub died: Vec<QueryId>,
}

/// What a server restart leaves of the reads.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Restarted {
    /// Reads of projects: their opens are gone.
    pub dead: Vec<QueryId>,
    /// Reads of the app: still meaningful, fetched again.
    pub starts: Vec<Start>,
}

#[derive(Debug, Clone, Copy)]
enum Fetch {
    Idle,
    /// `again`: a change reached the read after this fetch started.
    Running {
        ticket: Ticket,
        again: bool,
    },
}

#[derive(Debug)]
struct Read {
    of: ReadOf,
    holders: usize,
    fetch: Fetch,
    /// The snapshot of the value on screen.
    applied: Option<ViewSeq>,
    /// The server run the read was made in; a project read of an earlier run is gone.
    run: u64,
}

#[derive(Debug, Default)]
pub struct Registry {
    reads: HashMap<QueryId, Read>,
    /// The newest open of each folder that was announced.
    newest: HashMap<ProjectRef, Epoch>,
    /// Opens that were closed or refused as stale. Tracking the dead and not the live means a
    /// unit that forgets to announce an open still gets its replies.
    dead: HashSet<Scope>,
    run: u64,
    next_ticket: u64,
}

fn current(newest: &HashMap<ProjectRef, Epoch>, dead: &HashSet<Scope>, at: &Scope) -> bool {
    !dead.contains(at) && newest.get(&at.project).is_none_or(|n| *n <= at.epoch)
}

/// Whether a read can still be answered: a read of the app always can; a read of a project
/// only while its open is current and the server run is the one it was made in.
fn is_live(
    read: &Read,
    run: u64,
    newest: &HashMap<ProjectRef, Epoch>,
    dead: &HashSet<Scope>,
) -> bool {
    match &read.of {
        ReadOf::App(_) => true,
        ReadOf::Project { at, .. } => read.run == run && current(newest, dead, at),
    }
}

fn fresh(next: &mut u64) -> Ticket {
    *next += 1;
    Ticket(*next)
}

impl Registry {
    pub fn new() -> Self {
        Self::default()
    }

    /// The number of reads held.
    pub fn len(&self) -> usize {
        self.reads.len()
    }

    pub fn is_empty(&self) -> bool {
        self.reads.is_empty()
    }

    /// Whether `at` is an open that can still answer: not closed, not refused as stale, and not
    /// replaced by a newer open of its folder.
    pub fn is_current(&self, at: &Scope) -> bool {
        current(&self.newest, &self.dead, at)
    }

    /// One more holder of the read `id`. The first holder of a live read fetches it; the
    /// others share its value.
    pub fn hold(&mut self, id: &QueryId, of: ReadOf) -> Held {
        if let Some(read) = self.reads.get_mut(id) {
            read.holders += 1;
            return Held::Shared;
        }
        if let ReadOf::Project { at, .. } = &of {
            // A read of an open the registry has not heard of teaches it that open.
            let known = self.newest.entry(at.project.clone()).or_insert(at.epoch);
            *known = (*known).max(at.epoch);
        }
        let mut read = Read {
            of,
            holders: 1,
            fetch: Fetch::Idle,
            applied: None,
            run: self.run,
        };
        let held = if is_live(&read, self.run, &self.newest, &self.dead) {
            let ticket = fresh(&mut self.next_ticket);
            read.fetch = Fetch::Running {
                ticket,
                again: false,
            };
            Held::Fetch(ticket)
        } else {
            Held::Gone
        };
        self.reads.insert(id.clone(), read);
        held
    }

    /// One holder fewer. True when it was the last: the read is forgotten, and a reply that is
    /// still on its way is ignored.
    pub fn release(&mut self, id: &QueryId) -> bool {
        let Some(read) = self.reads.get_mut(id) else {
            return false;
        };
        read.holders = read.holders.saturating_sub(1);
        if read.holders > 0 {
            return false;
        }
        self.reads.remove(id);
        true
    }

    /// Fetches `id` again, for a Retry after a failure. `None` when the read is not live.
    pub fn refresh(&mut self, id: &QueryId) -> Option<Ticket> {
        let Registry {
            reads,
            newest,
            dead,
            run,
            next_ticket,
        } = self;
        let read = reads.get_mut(id)?;
        if !is_live(read, *run, newest, dead) {
            return None;
        }
        make_stale(read, next_ticket)
    }

    /// Makes stale every live read that `wants`, and returns the fetches to start.
    fn stale_where(&mut self, wants: impl Fn(&ReadOf) -> bool) -> Vec<Start> {
        let Registry {
            reads,
            newest,
            dead,
            run,
            next_ticket,
        } = self;
        let mut starts = Vec::new();
        for (id, read) in reads.iter_mut() {
            if is_live(read, *run, newest, dead)
                && wants(&read.of)
                && let Some(ticket) = make_stale(read, next_ticket)
            {
                starts.push(Start {
                    id: id.clone(),
                    ticket,
                });
            }
        }
        starts
    }

    /// A `Session::Changed` event at `at`: every live read of that open with a key that depends
    /// on `scope` fetches again.
    pub fn changed(&mut self, at: &Scope, scope: &ChangeScope) -> Vec<Start> {
        self.stale_where(|of| match of {
            ReadOf::Project { at: read_at, keys } => {
                read_at == at && keys.iter().any(|key| key.depends_on(scope))
            }
            ReadOf::App(_) => false,
        })
    }

    /// `ProjectsChanged` or `HomeCard`: every live read of the app that names the topic.
    pub fn topic(&mut self, topic: &AppTopic) -> Vec<Start> {
        self.stale_where(|of| match of {
            ReadOf::App(topics) => topics.contains(topic),
            ReadOf::Project { .. } => false,
        })
    }

    /// `Resync`, or a frame that could not be read: every live read fetches again.
    pub fn resync(&mut self) -> Vec<Start> {
        self.stale_where(|_| true)
    }

    /// A project was opened: the open `at` is the newest of its folder. Reads of the older
    /// opens of that folder have ended; their ids are returned once, so their values can say
    /// so.
    pub fn opened(&mut self, at: Scope) -> Vec<QueryId> {
        let known = self.newest.entry(at.project.clone()).or_insert(at.epoch);
        *known = (*known).max(at.epoch);
        let newest = *known;
        let Registry {
            reads, dead, run, ..
        } = self;
        // The older opens of this folder that no event or reply has ended yet.
        let newly_ended: HashSet<Scope> = reads
            .values()
            .filter(|read| read.run == *run)
            .filter_map(|read| match &read.of {
                ReadOf::Project { at: read_at, .. }
                    if read_at.project == at.project && read_at.epoch < newest =>
                {
                    Some(read_at.clone())
                }
                _ => None,
            })
            .filter(|ended| !dead.contains(ended))
            .collect();
        dead.extend(newly_ended.iter().cloned());
        reads
            .iter()
            .filter(|(_, read)| {
                read.run == *run
                    && matches!(&read.of, ReadOf::Project { at: read_at, .. } if newly_ended.contains(read_at))
            })
            .map(|(id, _)| id.clone())
            .collect()
    }

    /// A project was closed. Reads of that open have ended; their ids are returned.
    pub fn closed(&mut self, at: &Scope) -> Vec<QueryId> {
        if !self.dead.insert(at.clone()) {
            return Vec::new();
        }
        self.reads
            .iter()
            .filter(|(_, read)| {
                read.run == self.run
                    && matches!(&read.of, ReadOf::Project { at: read_at, .. } if read_at == at)
            })
            .map(|(id, _)| id.clone())
            .collect()
    }

    /// The server started again (a frame of another instance): every open is gone, and the
    /// numbers of its opens begin again. Project reads end; reads of the app fetch again.
    pub fn restarted(&mut self) -> Restarted {
        let dead = self
            .reads
            .iter()
            .filter(|(_, read)| read.run == self.run && matches!(read.of, ReadOf::Project { .. }))
            .map(|(id, _)| id.clone())
            .collect();
        self.run += 1;
        self.newest.clear();
        self.dead.clear();
        let starts = self.stale_where(|of| matches!(of, ReadOf::App(_)));
        Restarted { dead, starts }
    }

    /// A reply to the fetch `ticket` of `id`: whether to show it, and whether to fetch again.
    pub fn landed(&mut self, id: &QueryId, ticket: Ticket, landed: Landed) -> Landing {
        let ignored = Landing {
            outcome: Outcome::Ignore,
            then: None,
            died: Vec::new(),
        };
        let Registry {
            reads,
            newest,
            dead,
            run,
            next_ticket,
        } = self;
        let Some(read) = reads.get_mut(id) else {
            return ignored;
        };
        let again = match read.fetch {
            Fetch::Running {
                ticket: running,
                again,
            } if running == ticket => again,
            _ => return ignored,
        };
        read.fetch = Fetch::Idle;

        let live = is_live(read, *run, newest, dead);
        let project_at = match &read.of {
            ReadOf::Project { at, .. } => Some(at.clone()),
            ReadOf::App(_) => None,
        };
        if let (Some(at), Landed::StaleScope, true) = (&project_at, landed, live) {
            // The server says the open is gone before any event told us.
            dead.insert(at.clone());
            let died = reads
                .iter()
                .filter(|(other, other_read)| {
                    *other != id
                        && other_read.run == *run
                        && matches!(&other_read.of, ReadOf::Project { at: read_at, .. } if read_at == at)
                })
                .map(|(other, _)| other.clone())
                .collect();
            return Landing {
                outcome: Outcome::Drop,
                then: None,
                died,
            };
        }
        if !live {
            return Landing {
                outcome: Outcome::Drop,
                then: None,
                died: Vec::new(),
            };
        }

        let outcome = match landed {
            Landed::Value(Some(view)) if read.applied.is_some_and(|shown| view < shown) => {
                Outcome::Keep
            }
            Landed::Value(view) => {
                read.applied = view.or(read.applied);
                Outcome::Apply
            }
            // A `StaleScope` for a read of the app cannot be the open's end: it is an error.
            Landed::Error | Landed::StaleScope => Outcome::Apply,
        };
        let then = again.then(|| {
            let next = fresh(next_ticket);
            read.fetch = Fetch::Running {
                ticket: next,
                again: false,
            };
            next
        });
        Landing {
            outcome,
            then,
            died: Vec::new(),
        }
    }
}

/// A change reached the read: start a fetch if none runs, else make the running one be
/// followed by another.
fn make_stale(read: &mut Read, next: &mut u64) -> Option<Ticket> {
    match read.fetch {
        Fetch::Idle => {
            let ticket = fresh(next);
            read.fetch = Fetch::Running {
                ticket,
                again: false,
            };
            Some(ticket)
        }
        Fetch::Running { ticket, .. } => {
            read.fetch = Fetch::Running {
                ticket,
                again: true,
            };
            None
        }
    }
}
