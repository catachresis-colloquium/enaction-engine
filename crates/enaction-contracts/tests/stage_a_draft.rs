// SPDX-License-Identifier: MPL-2.0
//! The move from stage A to stage B inside the 2.0 draft (ADR-0030, "Limits",
//! second bullet).
//!
//! The wire does not mark a draft, so a package written under stage A is read
//! by a stage B host as it stands. Stage A wrote a supersession link as a bare
//! belief id; stage B writes a record that names its cause. This file pins the
//! two halves of what ADR-0030 says follows:
//!
//! * a stage A package WITH a link fails to decode, as one `BadShape` fault at
//!   the link, and is never validated;
//! * a stage A package WITHOUT a link validates and reads exactly as before,
//!   because stage B adds rules about links only.
//!
//! The bytes below are frozen copies of two stage A files, taken from commit
//! `ea791a2` (`conformance/tom/v2/accepted/caught/runner-output.json` and
//! `.../true-concealment/runner-output.json`). They are deliberately not read
//! from the live corpus, which stage B regenerates.

use enaction_contracts::tom::{ModelVerdict, model_timelines, stale_divergence};
use enaction_contracts::{BeliefId, parse_policies, validate_runner_output};

/// `policies.json` of the corpus at `ea791a2`.
const POLICIES: &str = r#"{
  "contract": "enaction-policies/1",
  "status": {
    "id": "conformance/status/v1",
    "rejected_ceiling": 1000,
    "unknown_band": 6000,
    "believed_floor": 7000,
    "suspected_floor": 5000
  },
  "depth": {
    "max_order": 1
  }
}
"#;

/// Stage A `accepted/caught`: beliefs 3 and 4 carry `"supersedes":1` and
/// `"supersedes":2`, the bare-id link.
const STAGE_A_WITH_LINK: &str = r#"{"contract":"enaction-runner-output/2","package":{"beliefs":[{"bearer":"contact-21","belief_id":1,"confidence":{"belief":0,"plausibility":500},"proposition":"guard-0 at cell-14","source_events":[1],"status":"rejected","valid_from":1},{"bearer":"contact-21","belief_id":2,"confidence":{"belief":0,"plausibility":500},"holders":["guard-0"],"proposition":"guard-0 at cell-14","source_events":[2],"status":"rejected","valid_from":2},{"bearer":"contact-21","belief_id":3,"confidence":{"belief":8000,"plausibility":9500},"proposition":"guard-0 at cell-14","source_events":[3],"status":"believed","supersedes":1,"valid_from":5},{"bearer":"contact-21","belief_id":4,"confidence":{"belief":8000,"plausibility":9500},"holders":["guard-0"],"proposition":"guard-0 at cell-14","source_events":[4],"status":"believed","supersedes":2,"valid_from":5}],"events":[{"confidence":500,"event_id":1,"kind":"observation","observer":"contact-21","proposition":"guard-0 at cell-14","provenance":{"origin":"sight"},"sequence":1,"subject":{"agent":"guard-0"},"tick":1},{"confidence":500,"event_id":2,"kind":"observation","observer":"guard-0","proposition":"contact-21 sees guard-0","provenance":{"origin":"sight"},"sequence":2,"subject":{"agent":"contact-21"},"tick":2},{"confidence":9000,"event_id":3,"kind":"observation","observer":"contact-21","proposition":"guard-0 at cell-14","provenance":{"origin":"sight"},"sequence":3,"subject":{"agent":"guard-0"},"tick":5},{"confidence":9000,"event_id":4,"kind":"observation","observer":"guard-0","proposition":"contact-21 sees guard-0","provenance":{"origin":"sight"},"sequence":4,"subject":{"agent":"contact-21"},"tick":5}],"manifest":{"digest":"ada480a97924146c88aa8d1b3bf9d4ecba0287b0df43bf5d74731fa08394c4b4","guarantees":["esm/replay-deterministic"],"kernel_version":{"major":2,"minor":0},"migration_from":{"major":1,"minor":0},"profile":{"id":"conformance/tom/v2","targets_kernel":{"major":2,"minor":0},"version":{"major":1,"minor":0}},"schema_version":{"major":1,"minor":0},"seed":{"fixed":7},"source":"conformance:tom/v2/caught"}},"world_truth":["guard-0 at cell-14","contact-21 sees guard-0"]}
"#;

/// Stage A `accepted/true-concealment`: one belief, no link.
const STAGE_A_WITHOUT_LINK: &str = r#"{"contract":"enaction-runner-output/2","package":{"beliefs":[{"bearer":"contact-21","belief_id":1,"confidence":{"belief":0,"plausibility":500},"holders":["guard-0"],"proposition":"guard-0 at cell-14","source_events":[1],"status":"rejected","valid_from":2}],"events":[{"confidence":500,"event_id":1,"kind":"observation","observer":"guard-0","proposition":"contact-21 sees guard-0","provenance":{"origin":"sight"},"sequence":1,"subject":{"agent":"contact-21"},"tick":2}],"manifest":{"digest":"77784e87623f66a81cc136fd8544141a830a6e172da32b0dac47c2f85979d9c6","guarantees":["esm/replay-deterministic"],"kernel_version":{"major":2,"minor":0},"migration_from":{"major":1,"minor":0},"profile":{"id":"conformance/tom/v2","targets_kernel":{"major":2,"minor":0},"version":{"major":1,"minor":0}},"schema_version":{"major":1,"minor":0},"seed":{"fixed":7},"source":"conformance:tom/v2/true-concealment"}},"world_truth":["guard-0 at cell-14"]}
"#;

#[test]
fn a_stage_a_package_with_a_link_fails_to_decode_under_stage_b() {
    let policies = parse_policies(POLICIES.as_bytes()).unwrap();
    assert!(
        STAGE_A_WITH_LINK.contains(r#""supersedes":1,"#),
        "a bare-id link"
    );
    let faults = validate_runner_output(STAGE_A_WITH_LINK.as_bytes(), &policies).unwrap_err();
    let refused: Vec<_> = faults.iter().map(|f| (f.rule, f.path.as_str())).collect();
    assert_eq!(
        refused,
        [("BadShape", "package.beliefs[2].supersedes")],
        "one fault, at the first link, and no rule reached"
    );
}

#[test]
fn a_stage_a_package_without_a_link_validates_and_reads_identically() {
    assert!(!STAGE_A_WITHOUT_LINK.contains("supersedes"));
    let policies = parse_policies(POLICIES.as_bytes()).unwrap();
    let (output, _) = validate_runner_output(STAGE_A_WITHOUT_LINK.as_bytes(), &policies)
        .expect("stage B adds rules about links only");
    // What stage A's `expected.json` says: belief 1 is a model of order 1
    // that agrees from tick 2, and is not stale.
    let timelines = model_timelines(&output.package).unwrap();
    assert_eq!(timelines.len(), 1);
    assert_eq!(timelines[0].model, BeliefId(1));
    let line: Vec<_> = timelines[0]
        .segments
        .iter()
        .map(|s| (s.from_tick, s.verdict))
        .collect();
    assert_eq!(line, [(2, ModelVerdict::Agrees)]);
    assert_eq!(timelines[0].until, None);
    assert!(!stale_divergence(&timelines[0]));
}
