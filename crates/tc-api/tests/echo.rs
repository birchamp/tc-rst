//! `test/echo` answers a reply of the size the loopback measurement asks for (unit A2).
#![cfg(not(target_arch = "wasm32"))]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "test code: a failed unwrap is a failed test"
)]

use tc_api::test::{EchoView, MAX_ECHO};
use tc_api::upload::UploadFlow;

fn reply_length(view: &EchoView) -> usize {
    serde_json::to_vec(&tc_api::Reply::Ok(view)).unwrap().len()
}

#[test]
fn echo_reply_reaches_the_size_by_at_most_one_word() {
    let one_word = reply_length(&EchoView::of_size(MAX_ECHO))
        - reply_length(&EchoView::of_size(MAX_ECHO - 128));
    assert!(
        one_word < 256,
        "a step of {one_word} bytes is more than a word"
    );
    for size in [0_u32, 1, 100, 20_480, 1 << 20, MAX_ECHO] {
        let view = EchoView::of_size(size);
        let length = reply_length(&view);
        assert!(length >= size as usize, "{length} bytes for {size}");
        if let Some((_, shorter)) = view.words.split_last() {
            let without_last = EchoView {
                words: shorter.to_vec(),
            };
            assert!(
                reply_length(&without_last) < size as usize,
                "size {size}: the view has a word to spare"
            );
        }
    }
}

#[test]
fn echo_is_the_same_view_for_the_same_size_and_cycles_pointed_hebrew() {
    let a = EchoView::of_size(20_480);
    assert_eq!(a, EchoView::of_size(20_480));
    assert!(a.words.len() > 100);
    assert!(
        a.words
            .iter()
            .enumerate()
            .all(|(i, w)| w.index as usize == i)
    );
    assert!(
        a.words
            .iter()
            .all(|w| (1..=4).contains(&w.occurrence) && w.occurrences == 4)
    );
    assert!(
        a.words[0]
            .text
            .chars()
            .any(|c| ('\u{0591}'..='\u{05C7}').contains(&c))
    );
}

#[test]
fn an_upload_flow_header_names_the_flow_serde_names() {
    for flow in UploadFlow::ALL {
        let on_the_wire = serde_json::to_value(flow).unwrap();
        assert_eq!(on_the_wire, flow.header_value());
        assert_eq!(UploadFlow::from_header(flow.header_value()), Some(flow));
    }
    assert_eq!(UploadFlow::from_header("Exchange"), None);
    assert_eq!(UploadFlow::from_header(""), None);
}

#[test]
fn caps_are_4_gib_for_an_exchange_file_and_512_mib_for_every_other_flow() {
    assert_eq!(UploadFlow::Exchange.cap(), 4_294_967_296);
    for flow in [UploadFlow::Import, UploadFlow::Usfm, UploadFlow::GuidedFix] {
        assert_eq!(flow.cap(), 536_870_912);
    }
}
