// Goes at crates/tc-usfm/tests/repro_420.rs (its own test target).
//! tC4 #420 repro: an aligned word keeps the character style around it after the weave.
//! Run: tc-heavy cargo test -p tc-usfm --test repro_420 -- --nocapture
use tc_usfm::align::{AlignmentGroup, SourceWord, TargetWord, parse_aligned, target_words, weave};

fn src(content: &str) -> SourceWord {
    SourceWord {
        content: content.into(),
        strong: None,
        lemma: None,
        morph: None,
        occurrence: 1,
        occurrences: 1,
        extra: Vec::new(),
    }
}

/// Align the words named in `pairs` (target word -> source word); the rest stay unaligned.
fn woven(plain: &str, pairs: &[(&str, &str)]) -> String {
    let words = target_words(plain);
    let groups: Vec<AlignmentGroup> = pairs
        .iter()
        .map(|(t, s)| AlignmentGroup {
            sources: vec![src(s)],
            targets: vec![words.iter().find(|w| w.text == *t).unwrap().clone()],
        })
        .collect();
    let out = weave(plain, &groups).unwrap();
    println!("plain:  {plain}\nwoven:  {out}\n");
    // The woven verse takes apart into the same plain text: no word or marker lost.
    assert_eq!(parse_aligned(&out).unwrap().plain.trim(), plain.trim());
    out
}

#[test]
fn an_aligned_word_keeps_its_character_style() {
    for (marker, word) in [("add", "Jesucristo"), ("bd", "siervo"), ("it", "Dios"), ("nd", "Señor")] {
        let plain = format!("Pablo, \\{marker} {word}\\{marker}* de la fe,");
        let out = woven(&plain, &[(word, "X"), ("fe", "πίστιν")]);
        let open = format!("\\{marker} ");
        let close = format!("\\{marker}*");
        let (a, b) = (out.find(&open).unwrap(), out.find(&close).unwrap());
        let inside = &out[a..b];
        assert!(inside.contains(&format!("\\w {word}|")), "{marker} wraps {word}: {out}");
        assert!(inside.contains("\\zaln-s"), "the milestone sits inside the span: {out}");
    }
}

#[test]
fn two_words_and_an_unaligned_word_in_one_span_stay_inside() {
    let out = woven("Pablo, \\add siervo de\\add* Dios,", &[("siervo", "δοῦλος")]);
    let (a, b) = (out.find("\\add ").unwrap(), out.find("\\add*").unwrap());
    let inside = &out[a..b];
    assert!(inside.contains("\\w siervo|") && inside.contains("\\w de|"), "{out}");
}

#[test]
fn a_nested_style_keeps_its_word() {
    let out = woven("siervo de \\add el \\+nd Dios\\+nd*\\add* y", &[("Dios", "Θεοῦ")]);
    let (a, b) = (out.find("\\+nd ").unwrap(), out.find("\\+nd*").unwrap());
    assert!(out[a..b].contains("\\w Dios|"), "\\+nd wraps Dios: {out}");
}
