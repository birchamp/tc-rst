//! The event stream's parser and reader (APP-DESIGN 5.4), on the bytes tc-server writes.

use proptest::prelude::*;
use proptest::test_runner::{Config, TestCaseError, TestRunner};
use tc_api::event::{ChangeCause, ChangeScope, Event, EventFrame, SessionEvent};
use tc_api::ids::{Epoch, ProjectRef, Scope, ServerInstance, ViewSeq};
use tc_ui::app::events::{EventReader, Incoming, LineTooLong, SseEvent, SseParser};

fn instance(n: u8) -> ServerInstance {
    ServerInstance::from_bytes([n; 16])
}

fn frame(seq: u64, event: Event) -> EventFrame {
    EventFrame {
        seq,
        instance: instance(1),
        event,
    }
}

fn scope() -> Scope {
    Scope {
        project: ProjectRef::parse("TIT").unwrap(),
        epoch: Epoch::new(3),
    }
}

/// A frame as `crates/tc-server/src/events.rs` writes it.
fn wire(frame: &EventFrame) -> String {
    format!(
        "id: {}\ndata: {}\n\n",
        frame.seq,
        serde_json::to_string(frame).unwrap()
    )
}

/// Everything the reader returns for these chunks, as frames.
fn frames_of(reader: &mut EventReader, chunks: &[&[u8]]) -> Vec<EventFrame> {
    chunks
        .iter()
        .flat_map(|chunk| reader.push(chunk).unwrap())
        .map(|incoming| match incoming {
            Incoming::Frame(frame) => frame,
            other => panic!("not a frame: {other:?}"),
        })
        .collect()
}

fn events(parser: &mut SseParser, text: &str) -> Vec<SseEvent> {
    parser.push(text.as_bytes()).unwrap()
}

fn event(id: Option<&str>, data: &str) -> SseEvent {
    SseEvent {
        id: id.map(str::to_owned),
        data: data.to_owned(),
    }
}

#[test]
fn reads_the_frames_tc_server_writes() {
    let first = frame(1, Event::Internet { on: true });
    let second = frame(
        2,
        Event::Session {
            at: scope(),
            event: SessionEvent::Closed,
        },
    );
    // The first chunk of a stream, two frames, and a heartbeat between and after them.
    let bytes = format!(": tc4\n\n{}:\n\n{}:\n\n", wire(&first), wire(&second));
    let mut reader = EventReader::new(instance(1));
    assert_eq!(
        frames_of(&mut reader, &[bytes.as_bytes()]),
        vec![first, second]
    );
    assert_eq!(reader.after(), 2);
}

#[test]
fn comments_and_heartbeats_dispatch_nothing() {
    let mut parser = SseParser::new();
    assert!(events(&mut parser, ": tc4\n\n:\n\n: any text\n\n:no space\n\n").is_empty());
    // A comment inside an event leaves the event alone.
    assert_eq!(
        events(&mut parser, "data: a\n: note\ndata: b\n\n"),
        vec![event(None, "a\nb")]
    );
}

#[test]
fn a_cr_lf_split_across_chunks_ends_one_line() {
    let mut parser = SseParser::new();
    // "data: x" CR | LF "id: 4" CR | LF CR | LF: the LFs belong to the CRs before them, so
    // there is one event, not extra empty lines that would end events early.
    assert!(parser.push(b"data: x\r").unwrap().is_empty());
    assert!(parser.push(b"\nid: 4\r").unwrap().is_empty());
    assert!(parser.push(b"\n\r").unwrap().len() == 1);
    assert!(parser.push(b"\n").unwrap().is_empty());
    // And an LF that does not follow a CR is a line end of its own.
    assert_eq!(
        events(&mut parser, "data: y\n\n"),
        vec![event(Some("4"), "y")]
    );
}

#[test]
fn data_lines_join_with_a_newline_and_one_space_is_dropped() {
    let mut parser = SseParser::new();
    assert_eq!(
        events(&mut parser, "data: one\ndata:  two\ndata:three\ndata\n\n"),
        vec![event(None, "one\n two\nthree\n")]
    );
    // An empty data line is still data: an event whose text is empty.
    assert_eq!(events(&mut parser, "data:\n\n"), vec![event(None, "")]);
}

#[test]
fn an_event_is_dispatched_only_at_its_blank_line() {
    let mut parser = SseParser::new();
    assert!(events(&mut parser, "id: 1\ndata: a\n").is_empty());
    assert!(events(&mut parser, "data: b").is_empty());
    assert_eq!(events(&mut parser, "\n\n"), vec![event(Some("1"), "a\nb")]);
    // Blank lines with nothing before them dispatch nothing.
    assert!(events(&mut parser, "\n\n\n").is_empty());
    // An event with an id but no data dispatches nothing, and the id stays for the next.
    assert!(events(&mut parser, "id: 9\n\n").is_empty());
    assert_eq!(
        events(&mut parser, "data: c\n\n"),
        vec![event(Some("9"), "c")]
    );
}

#[test]
fn a_bom_at_the_start_is_dropped_once() {
    let mut parser = SseParser::new();
    // The three bytes of U+FEFF, split across chunks.
    assert!(parser.push(&[0xEF]).unwrap().is_empty());
    assert!(parser.push(&[0xBB, 0xBF]).unwrap().is_empty());
    assert_eq!(
        events(&mut parser, "data: a\n\n\u{feff}data: b\n\n"),
        vec![event(None, "a")],
        "a second BOM is part of the line 'BOM data: b', which is an unknown field"
    );
}

#[test]
fn fields_other_than_data_and_id_are_ignored() {
    let mut parser = SseParser::new();
    assert_eq!(
        events(
            &mut parser,
            "event: custom\nretry: 5000\nunknown: x\nnocolon\ndata: kept\n\n"
        ),
        vec![event(None, "kept")]
    );
}

#[test]
fn an_id_holding_nul_is_ignored() {
    let mut parser = SseParser::new();
    assert_eq!(
        events(&mut parser, "id: 5\ndata: a\n\nid: 6\0\ndata: b\n\n"),
        vec![event(Some("5"), "a"), event(Some("5"), "b")]
    );
}

#[test]
fn bytes_that_are_not_utf8_become_replacement_characters() {
    let mut parser = SseParser::new();
    let got = parser.push(b"data: a\xFFb\n\n").unwrap();
    assert_eq!(got, vec![event(None, "a\u{fffd}b")]);
}

#[test]
fn a_line_over_the_limit_is_an_error() {
    let mut parser = SseParser::new();
    let block = vec![b'a'; 1024 * 1024];
    let mut result = Ok(Vec::new());
    for _ in 0..=SseParser::MAX_LINE / block.len() {
        result = parser.push(&block);
        if result.is_err() {
            break;
        }
    }
    assert_eq!(result, Err(LineTooLong));
    // A line of exactly the limit is fine, and a reset starts over.
    parser.reset();
    assert!(parser.push(&block).unwrap().is_empty());
    assert_eq!(
        events(&mut parser, "\ndata: ok\n\n"),
        vec![event(None, "ok")]
    );
}

#[test]
fn after_follows_the_last_frame_read() {
    let mut reader = EventReader::new(instance(1));
    assert_eq!(reader.after(), 0);
    let a = wire(&frame(4, Event::Internet { on: true }));
    let b = wire(&frame(9, Event::ProjectsChanged));
    assert_eq!(frames_of(&mut reader, &[a.as_bytes()]).len(), 1);
    assert_eq!(reader.after(), 4);
    assert_eq!(frames_of(&mut reader, &[b.as_bytes()]).len(), 1);
    assert_eq!(reader.after(), 9);
}

#[test]
fn a_repeated_frame_is_skipped() {
    let mut reader = EventReader::new(instance(1));
    let a = wire(&frame(1, Event::Internet { on: true }));
    let b = wire(&frame(2, Event::Internet { on: false }));
    let all = format!("{a}{b}");
    assert_eq!(frames_of(&mut reader, &[all.as_bytes()]).len(), 2);
    reader.reconnected();
    // The server sends them again after a reconnect that asked from too early.
    assert!(reader.push(all.as_bytes()).unwrap().is_empty());
    assert_eq!(reader.after(), 2);
}

#[test]
fn a_resync_is_never_skipped_and_resets_after() {
    let mut reader = EventReader::new(instance(1));
    let ahead = wire(&frame(10, Event::Internet { on: true }));
    assert_eq!(frames_of(&mut reader, &[ahead.as_bytes()]).len(), 1);
    assert_eq!(reader.after(), 10);
    // A Resync is numbered like the server's newest frame, which is below the reader's.
    let resync = wire(&frame(4, Event::Resync));
    let got = frames_of(&mut reader, &[resync.as_bytes()]);
    assert_eq!(got, vec![frame(4, Event::Resync)]);
    assert_eq!(reader.after(), 4);
    // The same Resync again, as a reconnect may resend it, is read again too.
    assert_eq!(frames_of(&mut reader, &[resync.as_bytes()]).len(), 1);
    // And the frames after it are read.
    let next = wire(&frame(5, Event::ProjectsChanged));
    assert_eq!(frames_of(&mut reader, &[next.as_bytes()]).len(), 1);
    assert_eq!(reader.after(), 5);
}

#[test]
fn an_unreadable_frame_is_reported_and_after_moves_past_it() {
    let mut reader = EventReader::new(instance(1));
    let unreadable = "id: 7\ndata: {\"seq\":7,\"instance\":\"x\"}\n\nid: 8\ndata: not json\n\n";
    let got = reader.push(unreadable.as_bytes()).unwrap();
    assert_eq!(got.len(), 2);
    assert!(matches!(
        got.first(),
        Some(Incoming::Unreadable { seq: Some(7), .. })
    ));
    assert!(matches!(
        got.get(1),
        Some(Incoming::Unreadable { seq: Some(8), .. })
    ));
    assert_eq!(reader.after(), 8, "the same events are not asked for again");
    // A variant this build does not know is unreadable too, never dropped.
    let future = format!(
        "id: 9\ndata: {{\"seq\":9,\"instance\":\"{}\",\"event\":{{\"type\":\"fromTheFuture\"}}}}\n\n",
        instance(1)
    );
    let got = reader.push(future.as_bytes()).unwrap();
    assert!(matches!(
        got.as_slice(),
        [Incoming::Unreadable { seq: Some(9), .. }]
    ));
    assert_eq!(reader.after(), 9);
    // An unreadable event with no number leaves `after` where it was.
    let got = reader.push(b"data: ???\n\n").unwrap();
    assert!(matches!(got.as_slice(), [Incoming::Unreadable { .. }]));
    assert_eq!(reader.after(), 9);
}

#[test]
fn a_frame_of_another_server_run_is_a_restart() {
    let mut reader = EventReader::new(instance(1));
    let old = wire(&frame(30, Event::Internet { on: true }));
    assert_eq!(frames_of(&mut reader, &[old.as_bytes()]).len(), 1);
    let mut restarted = frame(2, Event::Internet { on: false });
    restarted.instance = instance(2);
    let got = reader.push(wire(&restarted).as_bytes()).unwrap();
    assert_eq!(got, vec![Incoming::Restart(instance(2))]);
    assert_eq!(reader.instance(), instance(2));
    assert_eq!(reader.after(), 2, "the new run counts from its own start");
    // Frames of the new run are read.
    let mut next = frame(3, Event::ProjectsChanged);
    next.instance = instance(2);
    assert_eq!(
        reader.push(wire(&next).as_bytes()).unwrap(),
        vec![Incoming::Frame(next)]
    );
}

#[test]
fn a_reconnect_drops_the_half_read_event() {
    let mut reader = EventReader::new(instance(1));
    let a = wire(&frame(1, Event::Internet { on: true }));
    let (head, tail) = a.as_bytes().split_at(a.len() / 2);
    assert!(reader.push(head).unwrap().is_empty());
    reader.reconnected();
    // The tail alone is not an event, and the next whole frame reads cleanly.
    assert!(reader.push(tail).unwrap().is_empty());
    let b = wire(&frame(2, Event::ProjectsChanged));
    assert_eq!(frames_of(&mut reader, &[b.as_bytes()]).len(), 1);
    assert_eq!(reader.after(), 2);
}

/// A frame the stream can carry in any position: no `Resync`, which is read again when
/// repeated.
fn plain_event() -> impl Strategy<Value = Event> {
    any::<Event>().prop_map(|event| match event {
        Event::Resync => Event::ProjectsChanged,
        other => other,
    })
}

/// Frames numbered 1, 2, … of one run, with text in Hebrew and Arabic among them so that a
/// split can fall inside a character.
fn numbered_frames() -> impl Strategy<Value = Vec<EventFrame>> {
    proptest::collection::vec(plain_event(), 0..8).prop_map(|events| {
        let mut frames: Vec<EventFrame> = events.into_iter().map(|event| frame(0, event)).collect();
        frames.push(frame(
            0,
            Event::Session {
                at: scope(),
                event: SessionEvent::Changed {
                    view: ViewSeq::new(2),
                    scope: ChangeScope::Everything,
                    cause: ChangeCause::OutsideEdit {
                        path: "תהילים/שלום.usfm — مرحبا بالعالم".into(),
                    },
                },
            },
        ));
        for (index, frame) in frames.iter_mut().enumerate() {
            frame.seq = index as u64 + 1;
        }
        frames
    })
}

/// The stream's bytes for `frames`: a first comment, a heartbeat after each, line ends `eol`.
fn stream(frames: &[EventFrame], eol: &str) -> Vec<u8> {
    let mut text = format!(": tc4{eol}{eol}");
    for frame in frames {
        text.push_str(&format!(
            "id: {}{eol}data: {}{eol}{eol}:{eol}{eol}",
            frame.seq,
            serde_json::to_string(frame).unwrap()
        ));
    }
    text.into_bytes()
}

fn cut(bytes: &[u8], mut points: Vec<usize>) -> Vec<&[u8]> {
    points.iter_mut().for_each(|p| *p %= bytes.len() + 1);
    points.sort_unstable();
    points.dedup();
    let mut chunks = Vec::new();
    let mut start = 0;
    for point in points {
        chunks.push(bytes.get(start..point).unwrap());
        start = point;
    }
    chunks.push(bytes.get(start..).unwrap());
    chunks
}

/// Runs a property on a thread with room for proptest's nested strategies, whose debug-build
/// frames overflow the default test stack for the deeper types (as in tc-api's tests).
fn property<S: Strategy>(
    strategy: impl FnOnce() -> S + Send + 'static,
    check: impl Fn(S::Value) -> Result<(), TestCaseError> + Send + 'static,
) {
    let outcome = std::thread::Builder::new()
        .stack_size(64 << 20)
        .spawn(move || {
            let mut runner = TestRunner::new(Config {
                cases: 128,
                failure_persistence: None,
                ..Config::default()
            });
            runner.run(&strategy(), check).map_err(|e| e.to_string())
        })
        .unwrap()
        .join()
        .unwrap();
    if let Err(failure) = outcome {
        panic!("{failure}");
    }
}

#[test]
fn any_split_of_the_stream_gives_the_same_frames() {
    property(
        || {
            (
                numbered_frames(),
                prop_oneof![Just("\n"), Just("\r\n"), Just("\r")],
                proptest::collection::vec(any::<usize>(), 0..24),
            )
        },
        |(frames, eol, points)| {
            let bytes = stream(&frames, eol);
            let mut whole = EventReader::new(instance(1));
            let mut split = EventReader::new(instance(1));
            let expected = frames_of(&mut whole, &[&bytes]);
            prop_assert_eq!(&expected, &frames);
            prop_assert_eq!(frames_of(&mut split, &cut(&bytes, points)), expected);
            prop_assert_eq!(split.after(), frames.len() as u64);
            Ok(())
        },
    );
}

#[test]
fn replays_with_repeats_are_read_once() {
    property(
        || {
            (
                numbered_frames(),
                // For each connection: how far back it starts (0 = none) and how far ahead it
                // ends.
                proptest::collection::vec((0usize..8, 1usize..8), 1..8),
                1usize..40,
            )
        },
        |(frames, plan, chunk)| {
            let mut reader = EventReader::new(instance(1));
            let mut got = Vec::new();
            let mut read = 0;
            let mut plan = plan.into_iter();
            while read < frames.len() {
                let (back, ahead) = plan.next().unwrap_or((0, frames.len()));
                let start = read.saturating_sub(back);
                let end = (read + ahead).min(frames.len());
                let bytes = stream(frames.get(start..end).unwrap(), "\n");
                for part in bytes.chunks(chunk) {
                    got.extend(frames_of(&mut reader, &[part]));
                }
                reader.reconnected();
                read = end;
            }
            prop_assert_eq!(got, frames);
            Ok(())
        },
    );
}
