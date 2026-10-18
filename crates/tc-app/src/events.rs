//! The event bus (APP-DESIGN §3.6, §4.1, §5.4, AD10): every frame the server sends the UI is
//! numbered here, kept in a ring of [`RING_FRAMES`] for reconnects, and sent to the readers
//! that are connected.
//!
//! This module is the ring and the numbering. The server (unit A2) writes the frames as
//! server-sent events, adds the 15 s heartbeat comment, and bounds each connection's queue:
//! a reader that is too slow is disconnected, and it reconnects with the last number it read.
//!
//! A reader that reconnects with `after` gets, in order, the frames after `after` that the ring
//! holds and then the live ones, with none lost or repeated between them. When the ring no
//! longer holds every frame after `after`, or `after` is a number this run never reached
//! (another server run's), it gets one `Resync` frame first, and the UI reads every live query
//! again. Progress frames are coalesced in the ring: only the newest `Running` frame of each
//! job stays, and only the newest `Startup(Preparing)` frame, because the newest count
//! replaces the older ones (a `Startup(Ready)` or `Startup(Failed)` frame removes the last
//! one and is itself kept). Every other frame stays until the ring is full.

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex, MutexGuard};

use tc_api::event::{Event, EventFrame, StartupState};
use tc_api::ids::{JobId, ServerInstance};
use tc_api::jobs::JobState;
use tokio::sync::broadcast;
use tokio::sync::broadcast::error::{RecvError, TryRecvError};

/// The frames a reconnect can be served from (AD10).
pub const RING_FRAMES: usize = 4096;

#[derive(Clone)]
pub struct EventBus {
    inner: Arc<BusInner>,
}

struct BusInner {
    instance: ServerInstance,
    state: Mutex<Ring>,
    live: broadcast::Sender<EventFrame>,
}

struct Ring {
    /// The number the next frame gets. The first frame is 1.
    next_seq: u64,
    /// The number of `slots[0]`.
    first_seq: u64,
    /// `None`: a progress frame that a newer frame of its job replaced.
    slots: VecDeque<Option<EventFrame>>,
    /// The highest number dropped to make room; 0 until the ring is full.
    floor: u64,
    /// The number of the newest `Running` frame of each job that is still in the ring.
    latest_progress: HashMap<JobId, u64>,
    /// The number of the newest `Startup(Preparing)` frame, while it is in the ring.
    latest_startup: Option<u64>,
}

impl Ring {
    fn head(&self) -> u64 {
        self.next_seq - 1
    }

    fn slot_mut(&mut self, seq: u64) -> Option<&mut Option<EventFrame>> {
        let index = usize::try_from(seq.checked_sub(self.first_seq)?).ok()?;
        self.slots.get_mut(index)
    }

    fn push(&mut self, frame: EventFrame) {
        self.slots.push_back(Some(frame));
        while self.slots.len() > RING_FRAMES {
            let seq = self.first_seq;
            if let Some(Some(dropped)) = self.slots.pop_front()
                && let Event::Job(view) = &dropped.event
                && self.latest_progress.get(&view.id) == Some(&seq)
            {
                self.latest_progress.remove(&view.id);
            }
            if self.latest_startup == Some(seq) {
                self.latest_startup = None;
            }
            self.floor = seq;
            self.first_seq += 1;
        }
    }
}

impl EventBus {
    pub fn new(instance: ServerInstance) -> Self {
        let (live, _) = broadcast::channel(RING_FRAMES);
        Self {
            inner: Arc::new(BusInner {
                instance,
                state: Mutex::new(Ring {
                    next_seq: 1,
                    first_seq: 1,
                    slots: VecDeque::new(),
                    floor: 0,
                    latest_progress: HashMap::new(),
                    latest_startup: None,
                }),
                live,
            }),
        }
    }

    pub fn instance(&self) -> ServerInstance {
        self.inner.instance
    }

    fn ring(&self) -> MutexGuard<'_, Ring> {
        // No code that holds this lock can panic with the ring half changed, so a poisoned
        // lock still guards a whole ring.
        self.inner
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Numbers, keeps and sends one frame; returns its number. The ring's lock is held
    /// while the frame is stored and sent, so a subscription made at the same time sees it
    /// in its replay or on its live receiver, never in both and never in neither.
    pub fn publish(&self, event: Event) -> u64 {
        let mut ring = self.ring();
        let seq = ring.next_seq;
        ring.next_seq += 1;
        if let Event::Job(view) = &event {
            if let Some(older) = ring.latest_progress.remove(&view.id)
                && let Some(slot) = ring.slot_mut(older)
            {
                *slot = None;
            }
            if matches!(view.state, JobState::Running(_)) {
                ring.latest_progress.insert(view.id, seq);
            }
        }
        if let Event::Startup(state) = &event {
            if let Some(older) = ring.latest_startup.take()
                && let Some(slot) = ring.slot_mut(older)
            {
                *slot = None;
            }
            if matches!(state, StartupState::Preparing(_)) {
                ring.latest_startup = Some(seq);
            }
        }
        let frame = EventFrame {
            seq,
            instance: self.inner.instance,
            event,
        };
        ring.push(frame.clone());
        // A send with no connected reader is not an error here.
        let _ = self.inner.live.send(frame);
        seq
    }

    /// The number of the newest frame, 0 before the first.
    pub fn head(&self) -> u64 {
        self.ring().head()
    }

    /// The frames after `after` (the last number the reader holds, 0 for none), then the live
    /// ones. If the ring no longer holds every frame after `after`, or `after` is beyond the
    /// newest frame, the first frame is a `Resync` numbered like the newest frame.
    pub fn subscribe(&self, after: u64) -> Subscription {
        let ring = self.ring();
        let live = self.inner.live.subscribe();
        let head = ring.head();
        let replay = if after < ring.floor || after > head {
            VecDeque::from([EventFrame {
                seq: head,
                instance: self.inner.instance,
                event: Event::Resync,
            }])
        } else {
            ring.slots
                .iter()
                .flatten()
                .filter(|frame| frame.seq > after)
                .cloned()
                .collect()
        };
        Subscription {
            replay,
            live,
            // Everything up to the newest frame is in the replay (or was coalesced out of
            // it), and the receiver was made under the lock, so it sees only later frames.
            last: head,
        }
    }
}

/// One reader's view of the bus.
pub struct Subscription {
    replay: VecDeque<EventFrame>,
    live: broadcast::Receiver<EventFrame>,
    last: u64,
}

// A frame is what a reader gets almost every time; boxing it would allocate once per frame
// to save bytes on the two rare variants.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Next {
    Frame(EventFrame),
    /// The reader fell more than [`RING_FRAMES`] behind. The server ends its stream, and the
    /// UI reconnects with the last number it read.
    Lagged,
    Closed,
}

impl Subscription {
    /// The next frame, waiting for one.
    pub async fn next(&mut self) -> Next {
        loop {
            if let Some(frame) = self.replay.pop_front() {
                return Next::Frame(frame);
            }
            match self.live.recv().await {
                Ok(frame) => {
                    if let Some(next) = self.accept(frame) {
                        return next;
                    }
                }
                Err(RecvError::Lagged(_)) => return Next::Lagged,
                Err(RecvError::Closed) => return Next::Closed,
            }
        }
    }

    /// The next frame if one is ready.
    pub fn try_next(&mut self) -> Option<Next> {
        loop {
            if let Some(frame) = self.replay.pop_front() {
                return Some(Next::Frame(frame));
            }
            match self.live.try_recv() {
                Ok(frame) => {
                    if let Some(next) = self.accept(frame) {
                        return Some(next);
                    }
                }
                Err(TryRecvError::Empty) => return None,
                Err(TryRecvError::Lagged(_)) => return Some(Next::Lagged),
                Err(TryRecvError::Closed) => return Some(Next::Closed),
            }
        }
    }

    /// A live frame the reader has not been given yet. The receiver is made under the
    /// ring's lock, so a frame at or before `last` cannot arrive; the check makes a repeat
    /// impossible if that ever changes.
    fn accept(&mut self, frame: EventFrame) -> Option<Next> {
        if frame.seq <= self.last {
            return None;
        }
        self.last = frame.seq;
        Some(Next::Frame(frame))
    }
}
