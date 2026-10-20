//! tC4's views (`src/views` in translationCore 4), one module per tC4 file, read top to bottom
//! (APP-DESIGN 6.1, 6.12). They draw what `use_app()` holds and call its actions. wasm32 with
//! the `app` feature only.
//!
//! The modules index with `get` and never with `[]` (APP-DESIGN 6.5, A-15).
#![deny(clippy::indexing_slicing)]

pub mod account_menu;
pub mod book_rail;
pub mod check;
pub mod conflict_markers;
pub mod draft_chrome;
pub mod help_panel;
pub mod helps_panel;
pub mod home;
pub mod modals;
pub mod open_progress;
pub mod panel_icons;
pub mod rail_chrome;
pub mod section_editor;
pub mod source_verse;
pub mod sync_result;
pub mod translate;
pub mod understand;

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use leptos::prelude::{
    AnimationFrameRequestHandle, on_cleanup, request_animation_frame_with_handle,
};

/// A chain of animation frames that the owner's cleanup ends.
struct Frames {
    pending: Cell<Option<AnimationFrameRequestHandle>>,
    live: Cell<bool>,
    step: RefCell<Box<dyn FnMut() -> bool>>,
}

/// Runs `step` on each of the next `frames` animation frames while it returns true. The current
/// owner's cleanup ends the chain and cancels the frame still pending, as tC4's effects return
/// `() => cancelAnimationFrame(frame)`: an effect that runs again, or a component that
/// unmounts, stops its frames, so no frame reads a `NodeRef` its component took with it (that
/// read panics, and a panic reloads the page).
pub(crate) fn each_frame(frames: u32, step: impl FnMut() -> bool + 'static) {
    let chain = Rc::new(Frames {
        pending: Cell::new(None),
        live: Cell::new(true),
        step: RefCell::new(Box::new(step)),
    });
    next_frame(chain.clone(), frames);
    let kept = send_wrapper::SendWrapper::new(chain);
    on_cleanup(move || {
        let chain = kept.take();
        chain.live.set(false);
        if let Some(handle) = chain.pending.take() {
            handle.cancel();
        }
    });
}

fn next_frame(chain: Rc<Frames>, left: u32) {
    if left == 0 {
        return;
    }
    let again = chain.clone();
    let handle = request_animation_frame_with_handle(move || {
        again.pending.set(None);
        if !again.live.get() {
            return;
        }
        let more = (again.step.borrow_mut())();
        if more && again.live.get() {
            next_frame(again, left - 1);
        }
    });
    chain.pending.set(handle.ok());
}
