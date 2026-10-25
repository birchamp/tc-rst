//! The flow gate: one sync, gateway change, share, sign-in and import at a time, and one USB
//! exchange per project, never beside its sync.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use proptest::prelude::*;
use tc_api::error::{ApiError, Flow};
use tc_api::ids::ProjectRef;
use tc_app::flows::{FlowGate, FlowPass, FlowRequest};

fn p(name: &str) -> ProjectRef {
    ProjectRef::parse(name).unwrap()
}

fn busy(result: Result<FlowPass, ApiError>) -> Flow {
    match result {
        Err(ApiError::Busy(flow)) => flow,
        Err(other) => panic!("a different error: {other:?}"),
        Ok(_) => panic!("the flow was free"),
    }
}

#[test]
fn receive_and_sync_of_one_project_never_overlap() {
    let gate = FlowGate::default();
    // Receive first: a sync of the project is refused, and names the receive.
    let receive = gate
        .enter(FlowRequest::ExchangeReceive(p("titus")))
        .unwrap();
    assert_eq!(
        busy(gate.enter(FlowRequest::Sync(p("titus")))),
        Flow::ExchangeReceive
    );
    drop(receive);
    // The sync first: a receive is refused, and names the sync.
    let sync = gate.enter(FlowRequest::Sync(p("titus"))).unwrap();
    assert_eq!(
        busy(gate.enter(FlowRequest::ExchangeReceive(p("titus")))),
        Flow::Sync
    );
    drop(sync);
    // Once the pass is dropped, the other enters.
    let again = gate
        .enter(FlowRequest::ExchangeReceive(p("titus")))
        .unwrap();
    assert_eq!(gate.running(), [Flow::ExchangeReceive]);
    drop(again);
    assert!(gate.running().is_empty());
}

#[test]
fn receive_and_sync_of_one_project_never_overlap_under_threads() {
    let gate = FlowGate::default();
    let sync_inside = Arc::new(AtomicBool::new(false));
    let receive_inside = Arc::new(AtomicBool::new(false));
    let run = |request: FlowRequest, mine: Arc<AtomicBool>, other: Arc<AtomicBool>| {
        let gate = gate.clone();
        std::thread::spawn(move || {
            for _ in 0..10_000 {
                let pass = loop {
                    match gate.enter(request.clone()) {
                        Ok(pass) => break pass,
                        Err(ApiError::Busy(_)) => std::thread::yield_now(),
                        Err(other) => panic!("{other:?}"),
                    }
                };
                mine.store(true, Ordering::SeqCst);
                std::thread::yield_now();
                assert!(
                    !other.load(Ordering::SeqCst),
                    "both flows of one project ran together"
                );
                mine.store(false, Ordering::SeqCst);
                drop(pass);
            }
        })
    };
    let a = run(
        FlowRequest::Sync(p("titus")),
        Arc::clone(&sync_inside),
        Arc::clone(&receive_inside),
    );
    let b = run(
        FlowRequest::ExchangeReceive(p("titus")),
        receive_inside,
        sync_inside,
    );
    a.join().unwrap();
    b.join().unwrap();
    assert!(gate.running().is_empty());
}

#[test]
fn export_and_receive_of_one_project_never_overlap() {
    let gate = FlowGate::default();
    let export = gate.enter(FlowRequest::ExchangeExport(p("titus"))).unwrap();
    assert_eq!(
        busy(gate.enter(FlowRequest::ExchangeReceive(p("titus")))),
        Flow::ExchangeExport
    );
    assert_eq!(
        busy(gate.enter(FlowRequest::ExchangeExport(p("titus")))),
        Flow::ExchangeExport
    );
    drop(export);
    let receive = gate
        .enter(FlowRequest::ExchangeReceive(p("titus")))
        .unwrap();
    assert_eq!(
        busy(gate.enter(FlowRequest::ExchangeExport(p("titus")))),
        Flow::ExchangeReceive
    );
    drop(receive);
}

#[test]
fn exchanges_of_two_projects_run_together() {
    let gate = FlowGate::default();
    let _export = gate.enter(FlowRequest::ExchangeExport(p("titus"))).unwrap();
    let _receive = gate
        .enter(FlowRequest::ExchangeReceive(p("jonah")))
        .unwrap();
    let _more = gate.enter(FlowRequest::ExchangeExport(p("ruth"))).unwrap();
    assert_eq!(gate.running().len(), 3);
}

#[test]
fn sync_of_one_project_and_receive_of_another_run_together() {
    let gate = FlowGate::default();
    let _sync = gate.enter(FlowRequest::Sync(p("titus"))).unwrap();
    let _receive = gate
        .enter(FlowRequest::ExchangeReceive(p("jonah")))
        .unwrap();
    let _export = gate.enter(FlowRequest::ExchangeExport(p("ruth"))).unwrap();
}

#[test]
fn one_sync_at_a_time_across_projects() {
    let gate = FlowGate::default();
    let sync = gate.enter(FlowRequest::Sync(p("titus"))).unwrap();
    assert_eq!(busy(gate.enter(FlowRequest::Sync(p("jonah")))), Flow::Sync);
    drop(sync);
    let _other = gate.enter(FlowRequest::Sync(p("jonah"))).unwrap();
}

#[test]
fn each_app_wide_flow_runs_one_at_a_time() {
    for (request, flow) in [
        (FlowRequest::GatewayChange, Flow::GatewayChange),
        (FlowRequest::Share, Flow::Share),
        (FlowRequest::Import, Flow::Import),
    ] {
        let gate = FlowGate::default();
        let pass = gate.enter(request.clone()).unwrap();
        assert_eq!(busy(gate.enter(request.clone())), flow);
        drop(pass);
        let _again = gate.enter(request).unwrap();
    }
}

#[test]
fn different_app_wide_flows_run_together() {
    let gate = FlowGate::default();
    let _passes = [
        gate.enter(FlowRequest::GatewayChange).unwrap(),
        gate.enter(FlowRequest::Share).unwrap(),
        gate.enter(FlowRequest::Import).unwrap(),
        gate.enter(FlowRequest::Sync(p("titus"))).unwrap(),
        gate.enter(FlowRequest::ExchangeExport(p("jonah"))).unwrap(),
    ];
    assert_eq!(gate.running().len(), 5);
}

#[test]
fn a_pass_frees_its_flow_when_dropped() {
    let gate = FlowGate::default();
    let pass = gate.enter(FlowRequest::Import).unwrap();
    assert_eq!(pass.request(), &FlowRequest::Import);
    assert_eq!(gate.running(), [Flow::Import]);
    drop(pass);
    assert!(gate.running().is_empty());
}

#[test]
fn a_pass_frees_its_flow_when_the_thread_that_holds_it_panics() {
    let gate = FlowGate::default();
    let pass = gate.enter(FlowRequest::Sync(p("titus"))).unwrap();
    let crashed = std::thread::spawn(move || {
        let _held = pass;
        panic!("the job failed");
    })
    .join();
    assert!(crashed.is_err());
    assert!(gate.running().is_empty());
    let _free = gate.enter(FlowRequest::Sync(p("jonah"))).unwrap();
}

#[test]
fn the_rule_is_symmetric_and_matches_the_design() {
    let all = requests();
    // Indexes into `all`: 0 and 1 are syncs of a and b; 2 to 4 the app-wide flows; 5 and 6
    // exports of a and b; 7 and 8 receives of a and b.
    let excluding: [(usize, usize); 16] = [
        // A request excludes itself: each flow runs one at a time.
        (0, 0),
        (1, 1),
        (2, 2),
        (3, 3),
        (4, 4),
        (5, 5),
        (6, 6),
        (7, 7),
        (8, 8),
        // Two syncs of different projects exclude each other, as in tC4.
        (0, 1),
        // One exchange per project, whichever kind.
        (5, 7),
        (6, 8),
        // A sync and an exchange of the same project.
        (0, 5),
        (0, 7),
        (1, 6),
        (1, 8),
    ];
    for (i, x) in all.iter().enumerate() {
        for (j, y) in all.iter().enumerate() {
            let listed = excluding.contains(&(i, j)) || excluding.contains(&(j, i));
            assert_eq!(x.excludes(y), listed, "{x:?} / {y:?}");
        }
    }
}

#[derive(Debug, Clone)]
enum Step {
    Enter(usize),
    Drop(usize),
}

fn requests() -> Vec<FlowRequest> {
    let (a, b) = (p("a"), p("b"));
    vec![
        FlowRequest::Sync(a.clone()),
        FlowRequest::Sync(b.clone()),
        FlowRequest::GatewayChange,
        FlowRequest::Share,
        FlowRequest::Import,
        FlowRequest::ExchangeExport(a.clone()),
        FlowRequest::ExchangeExport(b.clone()),
        FlowRequest::ExchangeReceive(a),
        FlowRequest::ExchangeReceive(b),
    ]
}

proptest! {
    /// `enter` succeeds exactly when no held request excludes the new one, and the refusal
    /// names a holder that excludes it.
    #[test]
    fn flow_gate_matches_its_model(
        steps in proptest::collection::vec(
            prop_oneof![(0usize..9).prop_map(Step::Enter), (0usize..9).prop_map(Step::Drop)],
            1..80,
        )
    ) {
        let all = requests();
        let gate = FlowGate::default();
        let mut model: Vec<FlowRequest> = Vec::new();
        let mut passes: Vec<FlowPass> = Vec::new();
        for step in steps {
            match step {
                Step::Enter(index) => {
                    let request = all[index].clone();
                    let blocker = model.iter().find(|held| held.excludes(&request)).cloned();
                    match (gate.enter(request.clone()), blocker) {
                        (Ok(pass), None) => {
                            model.push(request);
                            passes.push(pass);
                        }
                        (Err(ApiError::Busy(flow)), Some(_)) => {
                            prop_assert!(model.iter().any(|held| held.flow() == flow && held.excludes(&request)));
                        }
                        (Ok(_), Some(holder)) => prop_assert!(false, "entered beside {holder:?}"),
                        (Err(error), None) => prop_assert!(false, "refused with {error:?} though free"),
                        (Err(error), Some(_)) => prop_assert!(false, "not Busy: {error:?}"),
                    }
                }
                Step::Drop(index) => {
                    if !passes.is_empty() {
                        let at = index % passes.len();
                        drop(passes.remove(at));
                        model.remove(at);
                    }
                }
            }
            prop_assert_eq!(gate.running().len(), model.len());
        }
    }
}
