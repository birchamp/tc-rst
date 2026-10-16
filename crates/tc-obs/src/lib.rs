//! Open Bible Stories for tC: the story file and its frames, pictures, a new project's
//! template, progress and the print model. Pure logic: the caller reads and writes files.
//!
//! The rules are tC4's, at commit `d310bf3` (BURRITO-SPEC §10, D74, D75):
//!
//! - [`story`] reads a story file `content/NN.md` without losing a byte ([`StoryFile`]),
//!   and holds tC4's three writers, which splice lines and never re-serialize. It also
//!   splits a file into the records STORAGE §2.2 keeps for it ([`decode_story`]) and
//!   renders them back ([`StoryRecords::render`]).
//! - [`ids`] holds [`StoryNo`] and [`FrameNo`] (frame 0 is the title) and the paths of an
//!   OBS project.
//! - [`images`] finds the picture of each frame; [`presentation`] puts a story, its
//!   gateway story and its pictures together for the story screen.
//! - [`frame_set`] checks that a gateway source has the project's stories and frames.
//! - [`template`] holds the platform's OBS template and makes a new project from it.
//! - [`progress`] computes draft percentages and the story a project opens on;
//!   [`recency`] keeps the stories edited last; [`print`] is the print model.
//! - [`questions`] keeps the OBS Translation Questions of a story.

mod error;
pub mod frame_set;
pub mod ids;
pub mod images;
mod js;
pub mod presentation;
pub mod print;
pub mod progress;
pub mod questions;
pub mod recency;
pub mod story;
pub mod template;

pub use error::{Error, NewProjectError, StoryError, TemplateError, TextProblem, WriteError};
pub use ids::{FrameNo, STORY_COUNT, StoryNo};
pub use story::{DecodedStory, Reformatted, StoryFile, StoryRecords, StoryState, decode_story};
