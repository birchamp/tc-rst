#[test]
fn quick() {}

/// Waits on a channel nobody sends to: idle, as the D1 tc-sync test sat for 61 minutes.
#[test]
fn waits_forever() {
    let (_tx, rx) = std::sync::mpsc::channel::<()>();
    rx.recv().unwrap();
}
