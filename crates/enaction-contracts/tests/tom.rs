// SPDX-License-Identifier: AGPL-3.0-or-later
//! Models are compared with minds (CAC-KERNEL §9, "Models are compared with
//! minds"; ADR-0028), against the published fixtures in `conformance/tom/v1/`.
//!
//! Each `runner-output.json` is generated from the contract types below.
//! Each `expected.json` is written by hand and is never generated: it is the
//! independent statement that the derivation is checked against. Set
//! `ENACTION_BLESS=1` to rewrite the generated files (`runner-output.json`
//! and `SHA256SUMS`) after a deliberate change. Without it, a difference
//! fails the test.

use std::collections::BTreeSet;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use enaction_contracts::canonical;
use enaction_contracts::tom::{MODEL_RULES, ModelVerdict, model_agreements};
use enaction_contracts::{
    Belief, BeliefId, BeliefInterval, BeliefStatus, ContractVersion, DepthPolicy, EpistemicEvent,
    EpistemicPackage, EventId, EventKind, KERNEL_CONTRACT_VERSION, Mass, PackageManifest,
    ProfileRef, Proposition, Provenance, RUNNER_OUTPUT_CONTRACT, RunnerOutput, SeedPolicy,
    StatusPolicy, Subject, package_digest, parse_policies, validate_package,
    validate_runner_output,
};

const BIN: &str = env!("CARGO_BIN_EXE_enaction-validate");

/// Where guard-0 is. The proposition every model in the corpus is about.
const WHERE: &str = "guard-0 at cell-14";
/// The cue: contact-21 looks at guard-0.
const CUE: &str = "contact-21 sees guard-0";

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../conformance/tom/v1")
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

/// The interval that the corpus status policy maps to each status.
fn interval(status: BeliefStatus) -> BeliefInterval {
    let (belief, plausibility) = match status {
        BeliefStatus::Certain => (10_000, 10_000),
        BeliefStatus::Believed => (8_000, 9_500),
        BeliefStatus::Suspected => (2_000, 6_000),
        BeliefStatus::Rejected => (0, 500),
        BeliefStatus::Unknown => (0, 10_000),
    };
    BeliefInterval::new(mass(belief), mass(plausibility)).unwrap()
}

/// An event: `observer` receives evidence about `subject`.
fn event(
    id: u64,
    tick: u64,
    observer: &str,
    subject: &str,
    kind: EventKind,
    proposition: &str,
    confidence: u16,
) -> EpistemicEvent {
    EpistemicEvent {
        event_id: EventId(id),
        sequence: id,
        tick,
        observer: observer.into(),
        subject: Subject::Agent(subject.into()),
        kind,
        proposition: Proposition(proposition.into()),
        confidence: mass(confidence),
        provenance: Provenance {
            origin: "sight".into(),
            channel: None,
        },
        affect: None,
        conation: None,
    }
}

struct B<'a> {
    id: u64,
    holders: &'a [&'a str],
    bearer: &'a str,
    proposition: &'a str,
    status: BeliefStatus,
    valid_from: u64,
    sources: &'a [u64],
    supersedes: Option<u64>,
}

fn belief(b: &B<'_>) -> Belief {
    Belief {
        belief_id: BeliefId(b.id),
        holders: b.holders.iter().map(|h| (*h).to_string()).collect(),
        bearer: b.bearer.into(),
        proposition: Proposition(b.proposition.into()),
        status: b.status,
        confidence: interval(b.status),
        source_events: b.sources.iter().copied().map(EventId).collect(),
        valid_from: b.valid_from,
        supersedes: b.supersedes.map(BeliefId),
    }
}

fn package(source: &str, events: Vec<EpistemicEvent>, beliefs: &[B<'_>]) -> EpistemicPackage {
    let mut package = EpistemicPackage {
        manifest: PackageManifest {
            kernel_version: KERNEL_CONTRACT_VERSION,
            profile: ProfileRef {
                id: "conformance/tom/v1".into(),
                version: ContractVersion { major: 1, minor: 0 },
                targets_kernel: KERNEL_CONTRACT_VERSION,
            },
            schema_version: ContractVersion { major: 1, minor: 0 },
            source: format!("conformance:tom/v1/{source}"),
            digest: String::new(),
            migration_from: ContractVersion { major: 1, minor: 0 },
            seed: SeedPolicy::Fixed(7),
            guarantees: vec!["esm/replay-deterministic".into()],
        },
        events,
        beliefs: beliefs.iter().map(belief).collect(),
    };
    package.manifest.digest = package_digest(&package).unwrap();
    package
}

fn output(package: EpistemicPackage, truth: &[&str]) -> RunnerOutput {
    RunnerOutput {
        contract: RUNNER_OUTPUT_CONTRACT.into(),
        package,
        world_truth: truth.iter().map(|t| Proposition((*t).into())).collect(),
    }
}

use BeliefStatus::{Believed, Rejected, Unknown};
use EventKind::{Inference, Observation};

/// Every case of the corpus, by directory under `conformance/tom/v1/`.
///
/// In every accepted case guard-0 really is at cell-14, so `WHERE` is in
/// world truth. A reader that compared a model with world truth would
/// therefore get three of the four cases wrong.
fn corpus() -> Vec<(&'static str, RunnerOutput)> {
    vec![
        // guard-0 thinks contact-21 does not know where it is. contact-21
        // has no belief about it at all, which under the closed-world rule
        // means that contact-21 does not hold it.
        (
            "accepted/true-concealment",
            output(
                package(
                    "true-concealment",
                    vec![event(1, 2, "guard-0", "contact-21", Observation, CUE, 500)],
                    &[B {
                        id: 1,
                        holders: &["guard-0"],
                        bearer: "contact-21",
                        proposition: WHERE,
                        status: Rejected,
                        valid_from: 2,
                        sources: &[1],
                        supersedes: None,
                    }],
                ),
                &[WHERE],
            ),
        ),
        // contact-21 does not know (1), and guard-0 is right about that (2).
        // On tick 5 contact-21 sees guard-0 and comes to know (3, superseding
        // 1). guard-0 sees the gaze on the same tick (4, superseding 2).
        (
            "accepted/caught",
            output(
                package(
                    "caught",
                    vec![
                        event(1, 1, "contact-21", "guard-0", Observation, WHERE, 500),
                        event(2, 2, "guard-0", "contact-21", Observation, CUE, 500),
                        event(3, 5, "contact-21", "guard-0", Observation, WHERE, 9_000),
                        event(4, 5, "guard-0", "contact-21", Observation, CUE, 9_000),
                    ],
                    &[
                        B {
                            id: 1,
                            holders: &[],
                            bearer: "contact-21",
                            proposition: WHERE,
                            status: Rejected,
                            valid_from: 1,
                            sources: &[1],
                            supersedes: None,
                        },
                        B {
                            id: 2,
                            holders: &["guard-0"],
                            bearer: "contact-21",
                            proposition: WHERE,
                            status: Rejected,
                            valid_from: 2,
                            sources: &[2],
                            supersedes: None,
                        },
                        B {
                            id: 3,
                            holders: &[],
                            bearer: "contact-21",
                            proposition: WHERE,
                            status: Believed,
                            valid_from: 5,
                            sources: &[3],
                            supersedes: Some(1),
                        },
                        B {
                            id: 4,
                            holders: &["guard-0"],
                            bearer: "contact-21",
                            proposition: WHERE,
                            status: Believed,
                            valid_from: 5,
                            sources: &[4],
                            supersedes: Some(2),
                        },
                    ],
                ),
                &[WHERE, CUE],
            ),
        ),
        // contact-21 knows where guard-0 is (1), and guard-0 knows that it
        // knows (2). On tick 6 contact-21 looks away and loses track (3,
        // status unknown, superseding 1). guard-0 still holds that it is
        // exposed and restates it (4, superseding 2): wrong about contact-21.
        (
            "accepted/false-exposure",
            output(
                package(
                    "false-exposure",
                    vec![
                        event(1, 2, "contact-21", "guard-0", Observation, WHERE, 9_000),
                        event(2, 2, "guard-0", "contact-21", Observation, CUE, 9_000),
                        event(3, 6, "contact-21", "guard-0", Observation, WHERE, 0),
                        event(4, 6, "guard-0", "contact-21", Inference, CUE, 9_000),
                    ],
                    &[
                        B {
                            id: 1,
                            holders: &[],
                            bearer: "contact-21",
                            proposition: WHERE,
                            status: Believed,
                            valid_from: 2,
                            sources: &[1],
                            supersedes: None,
                        },
                        B {
                            id: 2,
                            holders: &["guard-0"],
                            bearer: "contact-21",
                            proposition: WHERE,
                            status: Believed,
                            valid_from: 2,
                            sources: &[2],
                            supersedes: None,
                        },
                        B {
                            id: 3,
                            holders: &[],
                            bearer: "contact-21",
                            proposition: WHERE,
                            status: Unknown,
                            valid_from: 6,
                            sources: &[3],
                            supersedes: Some(1),
                        },
                        B {
                            id: 4,
                            holders: &["guard-0"],
                            bearer: "contact-21",
                            proposition: WHERE,
                            status: Believed,
                            valid_from: 6,
                            sources: &[4],
                            supersedes: Some(2),
                        },
                    ],
                ),
                &[WHERE],
            ),
        ),
        // contact-21 knows where guard-0 is (1). The host gave guard-0 no
        // cue, and guard-0 thinks it is hidden (2): wrong about contact-21.
        (
            "accepted/false-concealment",
            output(
                package(
                    "false-concealment",
                    vec![
                        event(1, 2, "contact-21", "guard-0", Observation, WHERE, 9_000),
                        event(2, 3, "guard-0", "contact-21", Observation, CUE, 500),
                    ],
                    &[
                        B {
                            id: 1,
                            holders: &[],
                            bearer: "contact-21",
                            proposition: WHERE,
                            status: Believed,
                            valid_from: 2,
                            sources: &[1],
                            supersedes: None,
                        },
                        B {
                            id: 2,
                            holders: &["guard-0"],
                            bearer: "contact-21",
                            proposition: WHERE,
                            status: Rejected,
                            valid_from: 3,
                            sources: &[2],
                            supersedes: None,
                        },
                    ],
                ),
                &[WHERE, CUE],
            ),
        ),
        // guard-0 thinks contact-21 thinks guard-0 knows where it is: order
        // 2, beyond the corpus depth of 1.
        (
            "rejected/nesting-bounded",
            output(
                package(
                    "nesting-bounded",
                    vec![event(
                        1,
                        2,
                        "guard-0",
                        "contact-21",
                        Observation,
                        CUE,
                        9_000,
                    )],
                    &[B {
                        id: 1,
                        holders: &["guard-0", "contact-21"],
                        bearer: "guard-0",
                        proposition: WHERE,
                        status: Believed,
                        valid_from: 2,
                        sources: &[1],
                        supersedes: None,
                    }],
                ),
                &[WHERE],
            ),
        ),
    ]
}

fn output_bytes(output: &RunnerOutput) -> Vec<u8> {
    let mut bytes = canonical::canonical_bytes(output).unwrap();
    bytes.push(b'\n');
    bytes
}

fn write_generated() {
    for (case, output) in corpus() {
        let dir = root().join(case);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("runner-output.json"), output_bytes(&output)).unwrap();
    }
}

#[test]
fn every_runner_output_is_generated_from_the_contract_types() {
    if bless() {
        write_generated();
    }
    for (case, output) in corpus() {
        let path = root().join(case).join("runner-output.json");
        let want = output_bytes(&output);
        assert!(
            read(&path) == want,
            "{} differs from what the contract types generate; rerun with \
             ENACTION_BLESS=1 after a deliberate change.\nwant:\n{}",
            path.display(),
            String::from_utf8_lossy(&want)
        );
    }
}

// ---------------------------------------------------------------- sums

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

/// The `SHA256SUMS` text for the sorted complete file set under `root`,
/// excluding `SHA256SUMS` itself, and the names it covers.
fn sums(root: &Path) -> (String, Vec<String>) {
    let mut all = Vec::new();
    files(root, &mut all);
    let mut names: Vec<String> = all
        .iter()
        .map(|p| {
            p.strip_prefix(root)
                .unwrap()
                .to_str()
                .unwrap()
                .replace('\\', "/")
        })
        .filter(|n| n != "SHA256SUMS")
        .collect();
    names.sort();
    let mut text = String::new();
    for name in &names {
        let hex = canonical::sha256_hex(&read(&root.join(name)));
        text.push_str(&format!("{hex}  {name}\n"));
    }
    (text, names)
}

#[test]
fn sha256sums_covers_the_sorted_complete_file_set() {
    let root = root();
    if bless() {
        // Tests run in parallel: write the generated files this hash covers
        // before hashing, so a bless run is deterministic.
        write_generated();
    }
    let (want, names) = sums(&root);
    for required in [
        "README.adoc",
        "policies.json",
        "accepted/true-concealment/expected.json",
        "accepted/caught/runner-output.json",
        "accepted/false-exposure/expected.json",
        "accepted/false-concealment/runner-output.json",
        "rejected/nesting-bounded/runner-output.json",
    ] {
        assert!(names.iter().any(|n| n == required), "covers {required}");
    }
    if bless() {
        std::fs::write(root.join("SHA256SUMS"), &want).unwrap();
    }
    let have = String::from_utf8(read(&root.join("SHA256SUMS"))).unwrap();
    assert_eq!(
        have, want,
        "SHA256SUMS differs from the file set; rerun with ENACTION_BLESS=1 \
         after a deliberate change"
    );
}

fn copy_tree(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let path = entry.unwrap().path();
        let target = to.join(path.file_name().unwrap());
        if path.is_dir() {
            copy_tree(&path, &target);
        } else {
            std::fs::copy(&path, &target).unwrap();
        }
    }
}

/// The sums check is a check: on a copy of the corpus, a changed file, an
/// extra file and a missing file each make the committed `SHA256SUMS`
/// disagree with the file set.
#[test]
fn the_sums_check_fails_on_a_changed_an_extra_and_a_missing_file() {
    let committed = String::from_utf8(read(&root().join("SHA256SUMS"))).unwrap();
    let base = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("tom-sums")
        .join(std::process::id().to_string());
    let _ = std::fs::remove_dir_all(&base);
    type Plant = fn(&Path);
    let plants: [(&str, Plant); 3] = [
        ("changed", |dir| {
            let path = dir.join("accepted/caught/expected.json");
            let mut bytes = read(&path);
            bytes.push(b' ');
            std::fs::write(path, bytes).unwrap();
        }),
        ("extra", |dir| {
            std::fs::write(dir.join("accepted/caught/extra.json"), b"{}\n").unwrap();
        }),
        ("missing", |dir| {
            std::fs::remove_file(dir.join("accepted/false-exposure/expected.json")).unwrap();
        }),
    ];
    for (name, plant) in plants {
        let dir = base.join(name);
        copy_tree(&root(), &dir);
        assert_eq!(sums(&dir).0, committed, "{name}: an unplanted copy agrees");
        plant(&dir);
        assert_ne!(sums(&dir).0, committed, "{name}: the plant is caught");
    }
}

// ------------------------------------------------------------ the corpus

fn policies() -> enaction_contracts::Policies {
    parse_policies(&read(&root().join("policies.json"))).unwrap()
}

fn verdict_name(verdict: ModelVerdict) -> &'static str {
    match verdict {
        ModelVerdict::Agrees => "agrees",
        ModelVerdict::Diverges => "diverges",
        ModelVerdict::NoClaim => "no-claim",
    }
}

fn derived(package: &EpistemicPackage) -> serde_json::Value {
    let verdicts = model_agreements(package).expect("every accepted case derives");
    serde_json::json!({
        "models": verdicts
            .iter()
            .map(|(id, v)| serde_json::json!({"belief_id": id.0, "verdict": verdict_name(*v)}))
            .collect::<Vec<_>>()
    })
}

fn case_dirs(kind: &str) -> Vec<(String, PathBuf)> {
    let mut cases: Vec<_> = std::fs::read_dir(root().join(kind))
        .unwrap()
        .map(|e| {
            let path = e.unwrap().path();
            (
                path.file_name().unwrap().to_str().unwrap().to_string(),
                path,
            )
        })
        .collect();
    cases.sort();
    cases
}

#[test]
fn the_corpus_declares_a_depth_of_one() {
    assert_eq!(policies().depth.max_order, 1);
}

#[test]
fn every_accepted_case_derives_exactly_its_expected_output() {
    let policies = policies();
    let mut seen = BTreeSet::new();
    for (name, case) in case_dirs("accepted") {
        let (output, _) =
            validate_runner_output(&read(&case.join("runner-output.json")), &policies)
                .unwrap_or_else(|f| panic!("accepted/{name}: {f:?}"));
        let expected: serde_json::Value =
            serde_json::from_slice(&read(&case.join("expected.json"))).unwrap();
        assert_eq!(derived(&output.package), expected, "accepted/{name}");
        // The expected output lists every model in the package.
        let models = output
            .package
            .beliefs
            .iter()
            .filter(|b| b.order() >= 1)
            .count();
        assert_eq!(
            expected["models"].as_array().unwrap().len(),
            models,
            "accepted/{name}: every model is listed"
        );
        seen.insert(name);
    }
    let want: BTreeSet<String> = [
        "caught",
        "false-concealment",
        "false-exposure",
        "true-concealment",
    ]
    .iter()
    .map(|s| (*s).to_string())
    .collect();
    assert_eq!(seen, want, "the four accepted cases, and no more");
}

#[test]
fn every_rejected_case_is_refused_with_exactly_its_rule() {
    let policies = policies();
    let mut seen = Vec::new();
    for (rule, case) in case_dirs("rejected") {
        let faults = validate_runner_output(&read(&case.join("runner-output.json")), &policies)
            .expect_err(&format!("rejected/{rule} must be refused"));
        let rules: Vec<_> = faults.iter().map(|f| f.rule).collect();
        assert_eq!(rules, [rule.as_str()], "rejected/{rule}: {faults:?}");
        seen.push(rule);
    }
    assert_eq!(seen, ["nesting-bounded"]);
}

/// The two false cases are the point of the slice: the corpus holds models
/// that diverge, and a reader that compares a model with world truth gets
/// them wrong.
#[test]
fn the_corpus_tells_a_mind_reader_from_a_world_reader() {
    let mut world_reader_wrong = 0;
    for (case, output) in corpus() {
        if !case.starts_with("accepted/") {
            continue;
        }
        let truth: BTreeSet<_> = output.world_truth.iter().collect();
        for (id, verdict) in model_agreements(&output.package).unwrap() {
            let model = output
                .package
                .beliefs
                .iter()
                .find(|b| b.belief_id == id)
                .unwrap();
            let claims = matches!(
                model.status,
                BeliefStatus::Certain | Believed | BeliefStatus::Suspected
            );
            let world = if claims == truth.contains(&model.proposition) {
                ModelVerdict::Agrees
            } else {
                ModelVerdict::Diverges
            };
            if world != verdict {
                world_reader_wrong += 1;
            }
        }
    }
    assert!(
        world_reader_wrong >= 3,
        "the corpus must separate the two readers: {world_reader_wrong}"
    );
}

// ---------------------------------------------------------- the binary

fn run(policies: &Path, stdin: &[u8]) -> Output {
    let mut child = Command::new(BIN)
        .args(["--policies", policies.to_str().unwrap()])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let _ = child.stdin.take().unwrap().write_all(stdin);
    child.wait_with_output().unwrap()
}

fn show(out: &Output) -> String {
    format!(
        "status {:?}\nstdout:\n{}\nstderr:\n{}",
        out.status.code(),
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

#[test]
fn each_accepted_case_exits_0_through_the_built_binary() {
    let policies = root().join("policies.json");
    let cases = case_dirs("accepted");
    assert_eq!(cases.len(), 4);
    for (name, case) in cases {
        let input = read(&case.join("runner-output.json"));
        let out = run(&policies, &input);
        assert_eq!(out.status.code(), Some(0), "{name}: {}", show(&out));
        assert_eq!(out.stdout, input, "{name}: canonical bytes pass through");
    }
}

#[test]
fn the_order_2_case_exits_1_with_the_nesting_rule_through_the_built_binary() {
    let policies = root().join("policies.json");
    let case = root().join("rejected/nesting-bounded");
    let out = run(&policies, &read(&case.join("runner-output.json")));
    assert_eq!(out.status.code(), Some(1), "{}", show(&out));
    let stdout = std::str::from_utf8(&out.stdout).unwrap();
    let line = stdout.strip_suffix('\n').expect("one line");
    assert!(!line.contains('\n'), "exactly one line: {stdout:?}");
    let value: serde_json::Value = serde_json::from_str(line).unwrap();
    let faults = value["faults"].as_array().unwrap();
    assert_eq!(faults.len(), 1, "{line}");
    assert_eq!(faults[0]["rule"], "nesting-bounded", "{line}");
    assert_eq!(
        faults[0]["path"], "package.beliefs[belief_id=1].holders",
        "{line}"
    );
}

// ------------------------------------------- the rule, case by case

fn status_policy() -> StatusPolicy {
    policies().status
}

fn verdicts(package: &EpistemicPackage, depth: usize) -> Vec<(u64, ModelVerdict)> {
    validate_package(
        package,
        KERNEL_CONTRACT_VERSION,
        &DepthPolicy { max_order: depth },
        &status_policy(),
    )
    .expect("the test package is valid");
    model_agreements(package)
        .unwrap()
        .into_iter()
        .map(|(id, v)| (id.0, v))
        .collect()
}

fn one_event() -> Vec<EpistemicEvent> {
    vec![event(
        1,
        1,
        "guard-0",
        "contact-21",
        Observation,
        CUE,
        9_000,
    )]
}

fn b<'a>(id: u64, holders: &'a [&'a str], bearer: &'a str, status: BeliefStatus) -> B<'a> {
    B {
        id,
        holders,
        bearer,
        proposition: WHERE,
        status,
        valid_from: 1,
        sources: &[1],
        supersedes: None,
    }
}

#[test]
fn order_0_beliefs_get_no_verdict_and_models_come_in_belief_id_order() {
    let p = package(
        "order",
        one_event(),
        &[
            b(9, &["guard-0"], "contact-21", Believed),
            b(4, &[], "contact-21", Believed),
            b(2, &["guard-0"], "contact-22", Rejected),
        ],
    );
    assert_eq!(
        verdicts(&p, 1),
        [(2, ModelVerdict::Agrees), (9, ModelVerdict::Agrees)]
    );
}

#[test]
fn holding_is_certain_believed_or_suspected() {
    use BeliefStatus::{Certain, Suspected};
    for (own, held) in [
        (Certain, true),
        (Believed, true),
        (Suspected, true),
        (Rejected, false),
        (Unknown, false),
    ] {
        for (model, claims) in [
            (Certain, true),
            (Believed, true),
            (Suspected, true),
            (Rejected, false),
        ] {
            let p = package(
                "holding",
                one_event(),
                &[
                    b(1, &[], "contact-21", own),
                    b(2, &["guard-0"], "contact-21", model),
                ],
            );
            let want = if held == claims {
                ModelVerdict::Agrees
            } else {
                ModelVerdict::Diverges
            };
            assert_eq!(verdicts(&p, 1), [(2, want)], "own {own:?}, model {model:?}");
        }
    }
}

#[test]
fn a_model_with_status_unknown_makes_no_claim() {
    for own in [None, Some(Believed), Some(Rejected)] {
        let mut beliefs = vec![b(2, &["guard-0"], "contact-21", Unknown)];
        if let Some(status) = own {
            beliefs.push(b(1, &[], "contact-21", status));
        }
        let p = package("unknown", one_event(), &beliefs);
        assert_eq!(verdicts(&p, 1), [(2, ModelVerdict::NoClaim)], "{own:?}");
    }
}

#[test]
fn a_missing_own_belief_means_the_bearer_does_not_hold_it() {
    let p = package(
        "closed",
        one_event(),
        &[
            b(1, &["guard-0"], "contact-21", Believed),
            b(2, &["guard-0"], "contact-22", Rejected),
            // Another bearer's own belief, and another proposition, are not
            // contact-21's own belief about WHERE.
            b(3, &[], "contact-22", Believed),
            B {
                proposition: CUE,
                ..b(4, &[], "contact-21", Believed)
            },
        ],
    );
    assert_eq!(
        verdicts(&p, 1),
        [(1, ModelVerdict::Diverges), (2, ModelVerdict::Diverges)]
    );
}

/// The ascription path is read outermost first. `holders = [guard-0,
/// contact-21]`, `bearer = contact-22` is guard-0's model of contact-21's
/// model of contact-22's belief. One order less drops the FIRST holder:
/// `[contact-21]`, contact-22. Dropping the last instead would compare with
/// `[guard-0]`, contact-22, which this package makes disagree.
#[test]
fn one_order_less_drops_the_outermost_holder() {
    let p = package(
        "path",
        one_event(),
        &[
            b(1, &["guard-0", "contact-21"], "contact-22", Believed),
            b(2, &["contact-21"], "contact-22", Rejected),
            b(3, &["guard-0"], "contact-22", Believed),
        ],
    );
    let v = verdicts(&p, 2);
    assert_eq!(v[0], (1, ModelVerdict::Diverges), "{v:?}");
}

/// Supersession at the model's `valid_from`: a belief holds at tick `t`
/// when its `valid_from <= t` and no belief that supersedes it has a
/// `valid_from <= t`.
#[test]
fn the_counterpart_is_the_one_that_holds_at_the_models_valid_from() {
    let events = vec![
        event(1, 1, "contact-21", "guard-0", Observation, WHERE, 9_000),
        event(2, 4, "contact-21", "guard-0", Observation, WHERE, 0),
    ];
    let own = |id, status, valid_from, sources: &'static [u64], supersedes| B {
        valid_from,
        sources,
        supersedes,
        ..b(id, &[], "contact-21", status)
    };
    let model = |id, valid_from| B {
        valid_from,
        ..b(id, &["guard-0"], "contact-21", Believed)
    };
    let p = package(
        "supersession",
        events,
        &[
            own(1, Believed, 1, &[1], None),
            own(2, Rejected, 4, &[2], Some(1)),
            model(10, 1),
            model(11, 3),
            model(12, 4),
            model(13, 9),
        ],
    );
    assert_eq!(
        verdicts(&p, 1),
        [
            (10, ModelVerdict::Agrees),
            (11, ModelVerdict::Agrees),
            (12, ModelVerdict::Diverges),
            (13, ModelVerdict::Diverges),
        ]
    );
    // Before any own belief is valid, the bearer holds nothing.
    let p = package(
        "before",
        vec![event(
            1,
            1,
            "contact-21",
            "guard-0",
            Observation,
            WHERE,
            9_000,
        )],
        &[
            B {
                valid_from: 5,
                ..b(1, &[], "contact-21", Believed)
            },
            B {
                valid_from: 2,
                ..b(2, &["guard-0"], "contact-21", Believed)
            },
        ],
    );
    assert_eq!(verdicts(&p, 1), [(2, ModelVerdict::Diverges)]);
}

#[test]
fn two_counterparts_that_both_hold_are_refused_with_a_named_rule() {
    let p = package(
        "ambiguous",
        one_event(),
        &[
            b(1, &[], "contact-21", Believed),
            b(2, &[], "contact-21", Rejected),
            b(3, &["guard-0"], "contact-21", Believed),
            // A model with no claim needs no counterpart and is not refused.
            b(4, &["guard-0"], "contact-21", Unknown),
        ],
    );
    validate_package(
        &p,
        KERNEL_CONTRACT_VERSION,
        &DepthPolicy { max_order: 1 },
        &status_policy(),
    )
    .expect("the package rules allow it");
    let faults = model_agreements(&p).unwrap_err();
    assert_eq!(faults.len(), 1, "{faults:?}");
    assert_eq!(faults[0].rule(), "model-counterpart-unique");
    assert_eq!(faults[0].path(), "beliefs[belief_id=3]");
    assert_eq!(faults[0].candidates, [BeliefId(1), BeliefId(2)]);
    assert_eq!(MODEL_RULES, ["model-counterpart-unique"]);
}
