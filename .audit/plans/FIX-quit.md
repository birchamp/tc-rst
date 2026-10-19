# FIX-quit plan

Four tests in `crates/tc-server/tests/lifecycle.rs` fail under load (503 where 200 was
expected): `quit_waits_for_a_call_whose_client_went_away`,
`quit_work_begins_after_the_requests_being_answered`,
`a_request_being_answered_when_quit_begins_still_gets_its_reply`,
`a_call_whose_client_went_away_runs_to_its_end`.

1. Reproduce: run the four tests many times in parallel copies of the test binary.
2. Server: a request the gate admitted (counted in flight before the quit) is refused 503
   after its body is read if the quit flag flipped meanwhile (`routes::api` checks
   `is_closing()`), although the drain is still waiting for it and the backend is open.
   Fix: one phase enum (serving, draining, closing) instead of the `closing` flag; the
   gate refuses new requests once draining begins, the post-body check refuses only once
   the drain is over and the backend's quit work begins. Deterministic test with
   `Expect: 100-continue` (hyper sends 100 only when the handler, past the gate, reads
   the body).
3. Tests: replace sleeps with signals. The stub's `call` reports when it is entered and
   waits for a release; the tests wait for "entered" before quitting or hanging up, wait
   for the server to answer 503 (closing began) before releasing, and give the drain a
   limit far above any load so it cannot time out under the test.
4. Prove: the four (and the new test) pass 200 runs in a row under load; clippy and the
   crate's tests pass. Update §5.1's sentence on a body still arriving.
