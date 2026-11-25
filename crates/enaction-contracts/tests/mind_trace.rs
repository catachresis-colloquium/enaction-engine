// SPDX-License-Identifier: MPL-2.0
//! `enaction-mind-trace/1` (WEEVIL-HOST-ROUTE, "The mind trace contract")
//! against the published fixtures in `conformance/mind/v2/`.
//!
//! Set `ENACTION_BLESS=1` to rewrite the two generated files
//! (`runner-output.json` and `SHA256SUMS`) after a deliberate change. Without
//! it, a difference fails the test.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use enaction_contracts::canonical;
use enaction_contracts::mind::{
    MIND_RULES, MindTick, commitment_override, commitment_overrides, validate_mind_trace,
};
use enaction_contracts::{
    Belief, BeliefId, BeliefInterval, BeliefStatus, ContractVersion, EpistemicEvent,
    EpistemicPackage, EventId, EventKind, KERNEL_CONTRACT_VERSION, Mass, PackageManifest,
    ProfileRef, Proposition, Provenance, RUNNER_OUTPUT_CONTRACT, RunnerOutput, SeedPolicy, Subject,
    package_digest, parse_policies, validate_runner_output,
};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../conformance/mind/v2")
}

fn bless() -> bool {
    std::env::var_os("ENACTION_BLESS").is_some_and(|v| v == "1")
}

fn read(path: &Path) -> Vec<u8> {
    std::fs::read(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn mass(v: u16) -> Mass {
    Mass::new(v).unwrap()
}

/// The paired runner output every mind fixture is checked against. Its
/// event ids are the causes a mind trace may cite: 11 (damage from
/// contact-3), 17 (contact-3 seen at close range), 23 (an unexplained noise).
fn runner_output() -> RunnerOutput {
    let event =
        |id: u64, tick: u64, subject: &str, kind, proposition: &str, origin: &str| EpistemicEvent {
            event_id: EventId(id),
            sequence: id,
            tick,
            observer: "guard-0".into(),
            subject: Subject::Agent(subject.into()),
            kind,
            proposition: Proposition(proposition.into()),
            confidence: mass(9_000),
            provenance: Provenance {
                origin: origin.into(),
                channel: None,
            },
            affect: None,
            conation: None,
        };
    let mut package = EpistemicPackage {
        manifest: PackageManifest {
            kernel_version: KERNEL_CONTRACT_VERSION,
            profile: ProfileRef {
                id: "idaptik/esm/v1".into(),
                version: ContractVersion { major: 1, minor: 0 },
                targets_kernel: KERNEL_CONTRACT_VERSION,
            },
            schema_version: ContractVersion { major: 1, minor: 0 },
            source: "conformance:mind/v2".into(),
            digest: String::new(),
            migration_from: ContractVersion { major: 1, minor: 0 },
            seed: SeedPolicy::Fixed(7),
            guarantees: vec!["esm/replay-deterministic".into()],
        },
        events: vec![
            event(
                11,
                10,
                "guard-0",
                EventKind::Observation,
                "noise behind guard-0",
                "hearing",
            ),
            event(
                17,
                40,
                "contact-3",
                EventKind::Observation,
                "contact-3 at close range",
                "sight",
            ),
            event(
                23,
                41,
                "contact-3",
                EventKind::ActionOutcome,
                "contact-3 damaged guard-0",
                "damage",
            ),
        ],
        beliefs: vec![Belief {
            belief_id: BeliefId(1),
            holders: vec![],
            bearer: "guard-0".into(),
            proposition: Proposition("contact-3 is hostile".into()),
            status: BeliefStatus::Believed,
            confidence: BeliefInterval::new(mass(8_000), mass(9_500)).unwrap(),
            source_events: vec![EventId(17), EventId(23)],
            valid_from: 41,
            supersedes: None,
        }],
    };
    package.manifest.digest = package_digest(&package).unwrap();
    RunnerOutput {
        contract: RUNNER_OUTPUT_CONTRACT.into(),
        package,
        world_truth: vec![Proposition("contact-3 is hostile".into())],
    }
}

fn check_generated(path: &Path, want: &[u8]) {
    if bless() {
        std::fs::write(path, want).unwrap();
    }
    let have = read(path);
    assert!(
        have == want,
        "{} differs from what the contract types generate; rerun with \
         ENACTION_BLESS=1 after a deliberate change.\nwant:\n{}",
        path.display(),
        String::from_utf8_lossy(want)
    );
}

fn runner_output_bytes() -> Vec<u8> {
    let mut bytes = canonical::canonical_bytes(&runner_output()).unwrap();
    bytes.push(b'\n');
    bytes
}

#[test]
fn the_paired_runner_output_is_generated_from_the_contract_types() {
    let bytes = runner_output_bytes();
    check_generated(&root().join("runner-output.json"), &bytes);
    let policies = parse_policies(&read(&root().join("policies.json"))).unwrap();
    validate_runner_output(&bytes, &policies).expect("the paired runner output is valid");
}

fn files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            files(&path, out);
        } else {
            out.push(path);
        }
    }
}

#[test]
fn sha256sums_covers_the_sorted_complete_file_set() {
    let root = root();
    if bless() {
        // Tests run in parallel: write the generated file this hash covers
        // before hashing, so a bless run is deterministic.
        std::fs::write(root.join("runner-output.json"), runner_output_bytes()).unwrap();
    }
    let mut all = Vec::new();
    files(&root, &mut all);
    let mut names: Vec<String> = all
        .iter()
        .map(|p| {
            p.strip_prefix(&root)
                .unwrap()
                .to_str()
                .unwrap()
                .replace('\\', "/")
        })
        .filter(|n| n != "SHA256SUMS")
        .collect();
    names.sort();
    let mut sums = String::new();
    for name in &names {
        let hex = canonical::sha256_hex(&read(&root.join(name)));
        sums.push_str(&format!("{hex}  {name}\n"));
    }
    // A plant: the set must include every case directory's files.
    assert!(names.iter().any(|n| n.starts_with("accepted/the-grudge/")));
    assert!(
        names
            .iter()
            .any(|n| n.starts_with("rejected/DanglingCause/"))
    );
    check_generated(&root.join("SHA256SUMS"), sums.as_bytes());
}

fn mind(case: &Path) -> Vec<u8> {
    read(&case.join("mind-trace.json"))
}

#[test]
fn every_rejected_case_is_refused_with_exactly_its_rule() {
    let runner = runner_output();
    let mut seen = BTreeSet::new();
    for entry in std::fs::read_dir(root().join("rejected")).unwrap() {
        let case = entry.unwrap().path();
        let rule = case.file_name().unwrap().to_str().unwrap().to_string();
        let faults = validate_mind_trace(&mind(&case), &runner)
            .expect_err(&format!("rejected/{rule} must be refused"));
        let rules: Vec<_> = faults.iter().map(|f| f.rule).collect();
        assert_eq!(rules, [rule.as_str()], "rejected/{rule}: {faults:?}");
        seen.insert(rule);
    }
    let all: BTreeSet<String> = MIND_RULES.iter().map(|r| (*r).to_string()).collect();
    assert_eq!(seen, all, "one rejected case for each rule, and no more");
    assert_eq!(MIND_RULES.len(), 10);
}

#[test]
fn every_accepted_case_validates_and_matches_its_expected_output() {
    let runner = runner_output();
    let mut cases = BTreeSet::new();
    for entry in std::fs::read_dir(root().join("accepted")).unwrap() {
        let case = entry.unwrap().path();
        let name = case.file_name().unwrap().to_str().unwrap().to_string();
        let (trace, _) = validate_mind_trace(&mind(&case), &runner)
            .unwrap_or_else(|f| panic!("accepted/{name}: {f:?}"));
        let derived = serde_json::json!({
            "commitment_override": commitment_overrides(&trace)
                .iter()
                .map(|(agent, tick)| serde_json::json!({"agent": agent, "tick": tick}))
                .collect::<Vec<_>>()
        });
        let expected: serde_json::Value =
            serde_json::from_slice(&read(&case.join("expected.json"))).unwrap();
        assert_eq!(derived, expected, "accepted/{name}");
        if name == "the-grudge" {
            assert!(
                !commitment_overrides(&trace).is_empty(),
                "the grudge shows ill will that persists"
            );
        }
        if name == "the-flinch" {
            assert!(
                trace
                    .ticks
                    .iter()
                    .any(|t| t.tells.iter().any(|tell| tell == "startle")),
                "the flinch shows a startle tell"
            );
        }
        cases.insert(name);
    }
    assert!(cases.contains("the-grudge") && cases.contains("the-flinch"));
}

fn tick(ranking: &[&str], incumbent: &str, selected: &str) -> MindTick {
    let value = serde_json::json!({
        "tick": 1,
        "agent": "a",
        "affect": {"token": "calm", "attention": 0, "caused_by": []},
        "conation": {
            "ranking": ranking,
            "incumbent": incumbent,
            "selected": selected,
        },
        "tells": [],
    });
    serde_json::from_value(value).unwrap()
}

#[test]
fn commitment_override_is_derived_not_asserted() {
    // Selected is the incumbent and is not first: override.
    assert!(commitment_override(&tick(
        &["evade", "engage"],
        "engage",
        "engage"
    )));
    // Selected is first: the ranking explains it, incumbent or not.
    assert!(!commitment_override(&tick(
        &["engage", "evade"],
        "engage",
        "engage"
    )));
    assert!(!commitment_override(&tick(&["engage"], "engage", "engage")));
    // Selected is neither first nor incumbent: no override (and the
    // validator refuses it as UnexplainedSelection).
    assert!(!commitment_override(&tick(
        &["evade", "engage"],
        "advance",
        "engage"
    )));
    // A producer cannot assert it: there is no member for it.
    let mut value = serde_json::to_value(tick(&["a"], "a", "a")).unwrap();
    value["commitment_override"] = true.into();
    let bytes = value.to_string();
    let fault = canonical::decode::<MindTick>(bytes.as_bytes()).unwrap_err();
    assert_eq!(fault.rule(), "UnknownField");
}

#[test]
fn a_mind_trace_carries_no_utility_and_no_float() {
    let case = root().join("accepted/the-grudge");
    let mut value: serde_json::Value = serde_json::from_slice(&mind(&case)).unwrap();
    value["ticks"][0]["conation"]["utility"] = 7.into();
    let faults = validate_mind_trace(value.to_string().as_bytes(), &runner_output()).unwrap_err();
    assert_eq!(faults[0].rule, "UnknownField");
    assert_eq!(faults[0].path, "ticks[0].conation.utility");

    let mut value: serde_json::Value = serde_json::from_slice(&mind(&case)).unwrap();
    value["ticks"][0]["affect"]["attention"] = serde_json::json!(0.75);
    let faults = validate_mind_trace(value.to_string().as_bytes(), &runner_output()).unwrap_err();
    assert_eq!(faults[0].rule, "NonIntegerNumber");
}

#[test]
fn attention_is_optional_and_incumbent_is_required_per_adr_0027() {
    let case = root().join("accepted/the-grudge");
    let mut value: serde_json::Value = serde_json::from_slice(&mind(&case)).unwrap();
    value["ticks"][0]["affect"]
        .as_object_mut()
        .unwrap()
        .remove("attention");
    validate_mind_trace(value.to_string().as_bytes(), &runner_output())
        .expect("attention may be absent");

    let mut value: serde_json::Value = serde_json::from_slice(&mind(&case)).unwrap();
    value["ticks"][0]["conation"]
        .as_object_mut()
        .unwrap()
        .remove("incumbent");
    let faults = validate_mind_trace(value.to_string().as_bytes(), &runner_output()).unwrap_err();
    assert_eq!(faults[0].rule, "BadShape", "{faults:?}");
}
