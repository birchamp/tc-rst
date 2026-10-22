#[test]
fn oracle_independence_false_adoption() {
    Run::run(&[
        Step::Edit { verse: 1, told: false },
        Step::Open,
        act(Act::Save(2)),
        act(Act::Leave),
    ])
    .unwrap();
}

#[test]
fn oracle_independence_false_adoption_control() {
    let mut run = Run::new();
    run.step(&Step::Edit { verse: 1, told: false }).unwrap();
    let outside = run.puts.first().expect("FA probe must create a put").bytes.clone();
    assert!(contains(&outside, "zq1x"), "FA_PROBE_SETUP_WRONG_OUTSIDE_TEXT");
    for step in [Step::Open, act(Act::Save(2)), act(Act::Leave)] {
        run.step(&step).unwrap();
    }
    let expected = Save {
        text: "zq1x".into(),
        target: Target::Verse(1),
        session: 0,
        fate: Fate::Kept,
        shown: false,
    };
    let fs = run.fs();
    let current = fs.peek(&run.path(USFM));
    let projection = run.handle().snapshot().fold().render(&DerivedFile::Usfm(TIT));
    let kept_whole_copy = fs.files_under(&run.path(".tc/external"))
        .values().any(|copy| *copy == outside);
    assert!(
        run.in_history(&expected)
            || current.as_deref() == Some(outside.as_slice())
            || projection.as_deref() == Some(outside.as_slice())
            || kept_whole_copy,
        "ORACLE_FALSE_ADOPTION_LOST_OUTSIDE_TEXT"
    );
    run.finish().unwrap();
}
