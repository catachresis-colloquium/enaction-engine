// SPDX-License-Identifier: MPL-2.0
//! Models are compared with minds (CAC-KERNEL §9, "Models are compared with
//! minds"; ADR-0028), against the published fixtures in `conformance/tom/v2/`.
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
use enaction_contracts::tom::{
    MODEL_RULES, ModelVerdict, holds, model_timelines, stale_divergence,
};
use enaction_contracts::{
    Belief, BeliefId, BeliefInterval, BeliefStatus, ContractVersion, DepthPolicy, EpistemicEvent,
    EpistemicPackage, EventId, EventKind, KERNEL_CONTRACT_VERSION, Mass, PackageFault,
    PackageManifest, ProfileRef, Proposition, Provenance, RUNNER_OUTPUT_CONTRACT, RunnerOutput,
    SeedPolicy, StatusPolicy, Subject, Supersession, package_digest, parse_policies,
    validate_package, validate_runner_output,
};

const BIN: &str = env!("CARGO_BIN_EXE_enaction-validate");

/// Where guard-0 is. The proposition every model in the corpus is about.
const WHERE: &str = "guard-0 at cell-14";
/// The cue: contact-21 looks at guard-0.
const CUE: &str = "contact-21 sees guard-0";

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../conformance/tom/v2")
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
    /// `(superseded belief, cause event)`.
    supersedes: Option<(u64, u64)>,
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
        supersedes: b.supersedes.map(|(belief, because)| Supersession {
            belief: BeliefId(belief),
            because: EventId(because),
        }),
    }
}

fn package(source: &str, events: Vec<EpistemicEvent>, beliefs: &[B<'_>]) -> EpistemicPackage {
    let mut package = EpistemicPackage {
        manifest: PackageManifest {
            kernel_version: KERNEL_CONTRACT_VERSION,
            profile: ProfileRef {
                id: "conformance/tom/v2".into(),
                version: ContractVersion { major: 1, minor: 0 },
                targets_kernel: KERNEL_CONTRACT_VERSION,
            },
            schema_version: ContractVersion { major: 1, minor: 0 },
            source: format!("conformance:tom/v2/{source}"),
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

/// A belief about `WHERE`, cited from one event, superseding nothing.
fn bw<'a>(
    id: u64,
    holders: &'a [&'a str],
    bearer: &'a str,
    status: BeliefStatus,
    valid_from: u64,
    sources: &'a [u64],
) -> B<'a> {
    B {
        id,
        holders,
        bearer,
        proposition: WHERE,
        status,
        valid_from,
        sources,
        supersedes: None,
    }
}

/// `belief` replaces `superseded` because of event `because`: the link
/// names its cause (ADR-0030).
fn superseding(belief: B<'_>, superseded: u64, because: u64) -> B<'_> {
    B {
        supersedes: Some((superseded, because)),
        ..belief
    }
}

/// The cue of contact-22 looking at guard-0, used only by `second-order`.
const CUE_22: &str = "contact-22 sees guard-0";

/// Every case of the corpus, by directory under `conformance/tom/v2/`.
///
/// In every accepted case guard-0 really is at cell-14, so `WHERE` is in
/// world truth. A reader that compared a model with world truth would get
/// most of the cases wrong.
#[allow(clippy::too_many_lines)]
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
                    &[bw(1, &["guard-0"], "contact-21", Rejected, 2, &[1])],
                ),
                &[WHERE],
            ),
        ),
        // contact-21 does not know (1), and guard-0 is right about that (2).
        // On tick 5 contact-21 sees guard-0 and comes to know (3, superseding
        // 1 because of event 3). guard-0 sees the gaze on the same tick (4,
        // superseding 2 because of event 4).
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
                        bw(1, &[], "contact-21", Rejected, 1, &[1]),
                        bw(2, &["guard-0"], "contact-21", Rejected, 2, &[2]),
                        superseding(bw(3, &[], "contact-21", Believed, 5, &[3]), 1, 3),
                        superseding(bw(4, &["guard-0"], "contact-21", Believed, 5, &[4]), 2, 4),
                    ],
                ),
                &[WHERE, CUE],
            ),
        ),
        // contact-21 knows where guard-0 is (1), and guard-0 knows that it
        // knows (2). On tick 6 contact-21 looks away and loses track (3,
        // status unknown, superseding 1). guard-0 states its model once and
        // never restates it: it agrees, then goes stale and diverges.
        (
            "accepted/false-exposure",
            output(
                package(
                    "false-exposure",
                    vec![
                        event(1, 2, "contact-21", "guard-0", Observation, WHERE, 9_000),
                        event(2, 2, "guard-0", "contact-21", Observation, CUE, 9_000),
                        event(3, 6, "contact-21", "guard-0", Observation, WHERE, 0),
                    ],
                    &[
                        bw(1, &[], "contact-21", Believed, 2, &[1]),
                        bw(2, &["guard-0"], "contact-21", Believed, 2, &[2]),
                        superseding(bw(3, &[], "contact-21", Unknown, 6, &[3]), 1, 3),
                    ],
                ),
                &[WHERE],
            ),
        ),
        // The same story, but the producer restates guard-0's model on tick 6
        // (4, superseding 2). The first model ends at tick 6 and only agrees;
        // the restated model diverges from the start, so neither is stale.
        (
            "accepted/false-exposure-restated",
            output(
                package(
                    "false-exposure-restated",
                    vec![
                        event(1, 2, "contact-21", "guard-0", Observation, WHERE, 9_000),
                        event(2, 2, "guard-0", "contact-21", Observation, CUE, 9_000),
                        event(3, 6, "contact-21", "guard-0", Observation, WHERE, 0),
                        event(4, 6, "guard-0", "contact-21", Inference, CUE, 9_000),
                    ],
                    &[
                        bw(1, &[], "contact-21", Believed, 2, &[1]),
                        bw(2, &["guard-0"], "contact-21", Believed, 2, &[2]),
                        superseding(bw(3, &[], "contact-21", Unknown, 6, &[3]), 1, 3),
                        superseding(bw(4, &["guard-0"], "contact-21", Believed, 6, &[4]), 2, 4),
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
                        bw(1, &[], "contact-21", Believed, 2, &[1]),
                        bw(2, &["guard-0"], "contact-21", Rejected, 3, &[2]),
                    ],
                ),
                &[WHERE, CUE],
            ),
        ),
        // contact-21 knows (1). guard-0 has a model of status unknown (2): it
        // makes no claim, so it is compared with nothing.
        (
            "accepted/no-claim",
            output(
                package(
                    "no-claim",
                    vec![
                        event(1, 2, "contact-21", "guard-0", Observation, WHERE, 9_000),
                        event(2, 3, "guard-0", "contact-21", Observation, CUE, 0),
                    ],
                    &[
                        bw(1, &[], "contact-21", Believed, 2, &[1]),
                        bw(2, &["guard-0"], "contact-21", Unknown, 3, &[2]),
                    ],
                ),
                &[WHERE, CUE],
            ),
        ),
        // Order 2, under the case's own depth of 2. contact-22 knows (1).
        // contact-21 thinks it does not (2, diverges). guard-0 thinks it does
        // (3, agrees). guard-0 thinks contact-21 thinks it does not (4). Read
        // outermost first, 4 is compared with 2 and agrees. Read the other way
        // round, 4 would be compared with 3 and diverge.
        (
            "accepted/second-order",
            output(
                package(
                    "second-order",
                    vec![
                        event(1, 1, "contact-22", "guard-0", Observation, WHERE, 9_000),
                        event(2, 1, "contact-21", "contact-22", Observation, CUE_22, 500),
                        event(3, 2, "guard-0", "contact-22", Observation, CUE_22, 9_000),
                        event(4, 2, "guard-0", "contact-21", Inference, CUE_22, 500),
                    ],
                    &[
                        bw(1, &[], "contact-22", Believed, 2, &[1]),
                        bw(2, &["contact-21"], "contact-22", Rejected, 2, &[2]),
                        bw(3, &["guard-0"], "contact-22", Believed, 2, &[3]),
                        bw(
                            4,
                            &["guard-0", "contact-21"],
                            "contact-22",
                            Rejected,
                            2,
                            &[4],
                        ),
                    ],
                ),
                &[WHERE, CUE_22],
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
                    &[bw(
                        1,
                        &["guard-0", "contact-21"],
                        "guard-0",
                        Believed,
                        2,
                        &[1],
                    )],
                ),
                &[WHERE],
            ),
        ),
        // The false-exposure story with its link dropped: on tick 6
        // contact-21 loses track (3, unknown), but 3 supersedes nothing. Two
        // own beliefs of one key, no contradiction record: a silent
        // overwrite (CAC-KERNEL §3 invariant 4, ADR-0029).
        (
            "rejected/belief-key-chained",
            output(
                package(
                    "belief-key-chained",
                    vec![
                        event(1, 2, "contact-21", "guard-0", Observation, WHERE, 9_000),
                        event(2, 6, "contact-21", "guard-0", Observation, WHERE, 0),
                    ],
                    &[
                        bw(1, &[], "contact-21", Believed, 2, &[1]),
                        bw(2, &[], "contact-21", Unknown, 6, &[2]),
                    ],
                ),
                &[WHERE],
            ),
        ),
        // The false-exposure story, but the link names event 9 as its
        // cause, and the package has no event 9.
        (
            "rejected/supersession-cause-present",
            output(
                package(
                    "supersession-cause-present",
                    vec![
                        event(1, 2, "contact-21", "guard-0", Observation, WHERE, 9_000),
                        event(2, 6, "contact-21", "guard-0", Observation, WHERE, 0),
                    ],
                    &[
                        bw(1, &[], "contact-21", Believed, 2, &[1]),
                        superseding(bw(2, &[], "contact-21", Unknown, 6, &[2]), 1, 9),
                    ],
                ),
                &[WHERE],
            ),
        ),
        // The restated false exposure, but guard-0's restated model (4
        // supersedes 2) names event 3 as its cause, and cites it. Event 3
        // reached contact-21, not guard-0. guard-0 cannot revise on what it
        // did not receive.
        (
            "rejected/supersession-cause-observed",
            output(
                package(
                    "supersession-cause-observed",
                    vec![
                        event(1, 2, "contact-21", "guard-0", Observation, WHERE, 9_000),
                        event(2, 2, "guard-0", "contact-21", Observation, CUE, 9_000),
                        event(3, 6, "contact-21", "guard-0", Observation, WHERE, 0),
                    ],
                    &[
                        bw(1, &[], "contact-21", Believed, 2, &[1]),
                        bw(2, &["guard-0"], "contact-21", Believed, 2, &[2]),
                        superseding(bw(3, &[], "contact-21", Unknown, 6, &[3]), 1, 3),
                        superseding(bw(4, &["guard-0"], "contact-21", Believed, 6, &[3]), 2, 3),
                    ],
                ),
                &[WHERE],
            ),
        ),
        // The false-exposure story, but belief 2 rests on event 1, the
        // evidence of the belief it replaces, and not on event 2, which it
        // names as its cause.
        (
            "rejected/supersession-cause-cited",
            output(
                package(
                    "supersession-cause-cited",
                    vec![
                        event(1, 2, "contact-21", "guard-0", Observation, WHERE, 9_000),
                        event(2, 6, "contact-21", "guard-0", Observation, WHERE, 0),
                    ],
                    &[
                        bw(1, &[], "contact-21", Believed, 2, &[1]),
                        superseding(bw(2, &[], "contact-21", Unknown, 6, &[1]), 1, 2),
                    ],
                ),
                &[WHERE],
            ),
        ),
        // contact-21 restates its belief on tick 6 (2 supersedes 1), but the
        // cause it names is event 1, the evidence that belief 1 already
        // rests on: nothing new reached it.
        (
            "rejected/supersession-cause-new",
            output(
                package(
                    "supersession-cause-new",
                    vec![event(
                        1,
                        2,
                        "contact-21",
                        "guard-0",
                        Observation,
                        WHERE,
                        9_000,
                    )],
                    &[
                        bw(1, &[], "contact-21", Believed, 2, &[1]),
                        superseding(bw(2, &[], "contact-21", Unknown, 6, &[1]), 1, 1),
                    ],
                ),
                &[WHERE],
            ),
        ),
        // Belief 1 starts on tick 6, on event 2. Belief 2 replaces it from
        // tick 7 and names event 1, on tick 2, as its cause: an event older
        // than what it replaces.
        (
            "rejected/supersession-cause-not-earlier",
            output(
                package(
                    "supersession-cause-not-earlier",
                    vec![
                        event(1, 2, "contact-21", "guard-0", Observation, WHERE, 9_000),
                        event(2, 6, "contact-21", "guard-0", Observation, WHERE, 9_000),
                    ],
                    &[
                        bw(1, &[], "contact-21", Believed, 6, &[2]),
                        superseding(bw(2, &[], "contact-21", Unknown, 7, &[1]), 1, 1),
                    ],
                ),
                &[WHERE],
            ),
        ),
        // contact-21 loses track from tick 4 (2 supersedes 1), but the event
        // named as the cause is on tick 6: an effect before its cause.
        (
            "rejected/supersession-cause-not-later",
            output(
                package(
                    "supersession-cause-not-later",
                    vec![
                        event(1, 2, "contact-21", "guard-0", Observation, WHERE, 9_000),
                        event(2, 6, "contact-21", "guard-0", Observation, WHERE, 0),
                    ],
                    &[
                        bw(1, &[], "contact-21", Believed, 2, &[1]),
                        superseding(bw(2, &[], "contact-21", Unknown, 4, &[2]), 1, 2),
                    ],
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
        "accepted/false-exposure-restated/expected.json",
        "accepted/false-concealment/runner-output.json",
        "accepted/no-claim/expected.json",
        "accepted/second-order/policies.json",
        "accepted/second-order/expected.json",
        "rejected/nesting-bounded/runner-output.json",
        "rejected/belief-key-chained/runner-output.json",
        "rejected/supersession-cause-present/runner-output.json",
        "rejected/supersession-cause-observed/runner-output.json",
        "rejected/supersession-cause-cited/runner-output.json",
        "rejected/supersession-cause-new/runner-output.json",
        "rejected/supersession-cause-not-earlier/runner-output.json",
        "rejected/supersession-cause-not-later/runner-output.json",
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

/// The policies a case is validated with: its own `policies.json` when it
/// has one, otherwise the corpus file.
fn policies_path(case: &Path) -> PathBuf {
    let own = case.join("policies.json");
    if own.exists() {
        own
    } else {
        root().join("policies.json")
    }
}

fn policies_at(path: &Path) -> enaction_contracts::Policies {
    parse_policies(&read(path)).unwrap()
}

fn verdict_name(verdict: ModelVerdict) -> &'static str {
    match verdict {
        ModelVerdict::Agrees => "agrees",
        ModelVerdict::Diverges => "diverges",
        ModelVerdict::NoClaim => "no-claim",
    }
}

/// The derived timelines as the corpus writes them. `until` is present only
/// for a model that is superseded.
fn derived(package: &EpistemicPackage) -> serde_json::Value {
    let timelines = model_timelines(package).expect("every accepted case derives");
    serde_json::json!({
        "models": timelines
            .iter()
            .map(|t| {
                let mut model = serde_json::json!({
                    "belief_id": t.model.0,
                    "timeline": t
                        .segments
                        .iter()
                        .map(|s| serde_json::json!({
                            "from_tick": s.from_tick,
                            "verdict": verdict_name(s.verdict),
                        }))
                        .collect::<Vec<_>>(),
                    "stale_divergent": stale_divergence(t),
                });
                if let Some(until) = t.until {
                    model["until"] = until.into();
                }
                model
            })
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
fn the_corpus_declares_a_depth_of_one_and_second_order_a_depth_of_two() {
    assert_eq!(
        policies_at(&root().join("policies.json")).depth.max_order,
        1
    );
    let second = root().join("accepted/second-order");
    assert_eq!(policies_at(&policies_path(&second)).depth.max_order, 2);
    // Only second-order carries its own policies.
    for (name, case) in case_dirs("accepted") {
        assert_eq!(
            case.join("policies.json").exists(),
            name == "second-order",
            "{name}"
        );
    }
}

#[test]
fn every_accepted_case_derives_exactly_its_expected_output() {
    let mut seen = BTreeSet::new();
    for (name, case) in case_dirs("accepted") {
        let policies = policies_at(&policies_path(&case));
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
        "false-exposure-restated",
        "no-claim",
        "second-order",
        "true-concealment",
    ]
    .iter()
    .map(|s| (*s).to_string())
    .collect();
    assert_eq!(seen, want, "the seven accepted cases, and no more");
}

/// Only the unrestated false exposure is stale-divergent: it agreed when it
/// was stated and diverges later.
#[test]
fn only_the_unrestated_false_exposure_is_stale_divergent() {
    for (case, output) in corpus() {
        if !case.starts_with("accepted/") {
            continue;
        }
        let stale: Vec<u64> = model_timelines(&output.package)
            .unwrap()
            .iter()
            .filter(|t| stale_divergence(t))
            .map(|t| t.model.0)
            .collect();
        let want: &[u64] = if case == "accepted/false-exposure" {
            &[2]
        } else {
            &[]
        };
        assert_eq!(stale, want, "{case}");
    }
}

#[test]
fn every_rejected_case_is_refused_with_exactly_its_rule() {
    let mut seen = Vec::new();
    for (rule, case) in case_dirs("rejected") {
        let policies = policies_at(&policies_path(&case));
        let faults = validate_runner_output(&read(&case.join("runner-output.json")), &policies)
            .expect_err(&format!("rejected/{rule} must be refused"));
        let rules: Vec<_> = faults.iter().map(|f| f.rule).collect();
        assert_eq!(rules, [rule.as_str()], "rejected/{rule}: {faults:?}");
        seen.push(rule);
    }
    assert_eq!(
        seen,
        [
            "belief-key-chained",
            "nesting-bounded",
            "supersession-cause-cited",
            "supersession-cause-new",
            "supersession-cause-not-earlier",
            "supersession-cause-not-later",
            "supersession-cause-observed",
            "supersession-cause-present",
        ]
    );
}

/// The corpus separates a mind reader from a world reader. A world reader
/// gives each model one verdict, its claim against world truth.
#[test]
fn the_corpus_tells_a_mind_reader_from_a_world_reader() {
    let mut world_reader_wrong = 0;
    for (case, output) in corpus() {
        if !case.starts_with("accepted/") {
            continue;
        }
        let truth: BTreeSet<_> = output.world_truth.iter().collect();
        for timeline in model_timelines(&output.package).unwrap() {
            let model = output
                .package
                .beliefs
                .iter()
                .find(|b| b.belief_id == timeline.model)
                .unwrap();
            let world = if holds(model.status) == truth.contains(&model.proposition) {
                ModelVerdict::Agrees
            } else {
                ModelVerdict::Diverges
            };
            let mind: Vec<_> = timeline.segments.iter().map(|s| s.verdict).collect();
            if mind != [world] {
                world_reader_wrong += 1;
            }
        }
    }
    assert!(
        world_reader_wrong >= 5,
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

/// The `(rule, path)` of every fault in the one-line JSON refusal.
fn refusal(out: &Output) -> Vec<(String, String)> {
    let stdout = std::str::from_utf8(&out.stdout).unwrap();
    let line = stdout.strip_suffix('\n').expect("one line");
    assert!(!line.contains('\n'), "exactly one line: {stdout:?}");
    let value: serde_json::Value = serde_json::from_str(line).unwrap();
    value["faults"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| {
            (
                f["rule"].as_str().unwrap().to_string(),
                f["path"].as_str().unwrap().to_string(),
            )
        })
        .collect()
}

#[test]
fn each_accepted_case_exits_0_through_the_built_binary() {
    let cases = case_dirs("accepted");
    assert_eq!(cases.len(), 7);
    for (name, case) in cases {
        let input = read(&case.join("runner-output.json"));
        let out = run(&policies_path(&case), &input);
        assert_eq!(out.status.code(), Some(0), "{name}: {}", show(&out));
        assert_eq!(out.stdout, input, "{name}: canonical bytes pass through");
    }
}

#[test]
fn the_order_2_case_exits_1_with_the_nesting_rule_through_the_built_binary() {
    let case = root().join("rejected/nesting-bounded");
    let out = run(
        &policies_path(&case),
        &read(&case.join("runner-output.json")),
    );
    assert_eq!(out.status.code(), Some(1), "{}", show(&out));
    assert_eq!(
        refusal(&out),
        [(
            "nesting-bounded".to_string(),
            "package.beliefs[belief_id=1].holders".to_string()
        )]
    );
    // The accepted order-2 case is refused too, under the corpus depth of 1:
    // its own depth of 2 is what admits it.
    let case = root().join("accepted/second-order");
    let out = run(
        &root().join("policies.json"),
        &read(&case.join("runner-output.json")),
    );
    assert_eq!(out.status.code(), Some(1), "{}", show(&out));
    assert_eq!(
        refusal(&out),
        [(
            "nesting-bounded".to_string(),
            "package.beliefs[belief_id=4].holders".to_string()
        )]
    );
}

// ------------------------------------------- the rule, case by case

fn status_policy() -> StatusPolicy {
    policies_at(&root().join("policies.json")).status
}

type Line = Vec<(u64, ModelVerdict)>;

/// Every model's id, segments as `(from_tick, verdict)`, and `until`.
fn timelines(package: &EpistemicPackage, depth: usize) -> Vec<(u64, Line, Option<u64>)> {
    validate_package(
        package,
        KERNEL_CONTRACT_VERSION,
        &DepthPolicy { max_order: depth },
        &status_policy(),
    )
    .expect("the test package is valid");
    model_timelines(package)
        .unwrap()
        .into_iter()
        .map(|t| {
            (
                t.model.0,
                t.segments
                    .iter()
                    .map(|s| (s.from_tick, s.verdict))
                    .collect(),
                t.until,
            )
        })
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
    bw(id, holders, bearer, status, 1, &[1])
}

use ModelVerdict::{Agrees, Diverges, NoClaim};

#[test]
fn order_0_beliefs_get_no_timeline_and_models_come_in_belief_id_order() {
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
        timelines(&p, 1),
        [(2, vec![(1, Agrees)], None), (9, vec![(1, Agrees)], None)]
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
        assert_eq!(holds(own), held);
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
            let want = if held == claims { Agrees } else { Diverges };
            assert_eq!(
                timelines(&p, 1),
                [(2, vec![(1, want)], None)],
                "own {own:?}, model {model:?}"
            );
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
        assert_eq!(timelines(&p, 1), [(2, vec![(1, NoClaim)], None)], "{own:?}");
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
            // contact-22's own belief makes model 2 diverge. It, and
            // contact-21's own belief about another proposition, are not
            // contact-21's own belief about WHERE.
            b(3, &[], "contact-22", Believed),
            B {
                proposition: CUE,
                ..b(4, &[], "contact-21", Believed)
            },
        ],
    );
    assert_eq!(
        timelines(&p, 1),
        [
            (1, vec![(1, Diverges)], None),
            (2, vec![(1, Diverges)], None)
        ]
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
    let t = timelines(&p, 2);
    assert_eq!(t[0], (1, vec![(1, Diverges)], None), "{t:?}");
}

/// A belief holds at tick `t` when its `valid_from <= t` and no belief that
/// supersedes it has a `valid_from <= t`. A model's timeline has a segment
/// for each change of verdict over its lifetime.
#[test]
fn a_timeline_follows_the_counterpart_through_supersession() {
    let events = vec![
        event(1, 1, "contact-21", "guard-0", Observation, WHERE, 9_000),
        event(2, 4, "contact-21", "guard-0", Observation, WHERE, 0),
    ];
    // One model per package: under ADR-0029 the models of one key form one
    // chain, so four unlinked models of the same key cannot share a package.
    let with_model = |id, status, valid_from| {
        package(
            "supersession",
            events.clone(),
            &[
                bw(1, &[], "contact-21", Believed, 1, &[1]),
                superseding(bw(2, &[], "contact-21", Rejected, 4, &[2]), 1, 2),
                bw(id, &["guard-0"], "contact-21", status, valid_from, &[1]),
            ],
        )
    };
    let cases = [
        (
            with_model(10, Believed, 1),
            vec![(1, Agrees), (4, Diverges)],
            true,
        ),
        (
            with_model(11, Believed, 3),
            vec![(3, Agrees), (4, Diverges)],
            true,
        ),
        (with_model(12, Believed, 4), vec![(4, Diverges)], false),
        (with_model(13, Rejected, 9), vec![(9, Agrees)], false),
    ];
    for (p, line, stale) in cases {
        let got = timelines(&p, 1);
        assert_eq!(got.len(), 1, "{got:?}");
        assert_eq!((&got[0].1, got[0].2), (&line, None), "model {}", got[0].0);
        assert_eq!(stale_divergence(&model_timelines(&p).unwrap()[0]), stale);
    }

    // Before any own belief is valid, the bearer holds nothing. Diverging
    // first and agreeing later is not stale divergence.
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
            bw(1, &[], "contact-21", Believed, 5, &[1]),
            bw(2, &["guard-0"], "contact-21", Believed, 2, &[1]),
        ],
    );
    assert_eq!(
        timelines(&p, 1),
        [(2, vec![(2, Diverges), (5, Agrees)], None)]
    );
    assert!(!stale_divergence(&model_timelines(&p).unwrap()[0]));
}

#[test]
fn a_change_of_counterpart_with_the_same_verdict_is_one_segment() {
    use BeliefStatus::Certain;
    let p = package(
        "merge",
        vec![
            event(1, 1, "contact-21", "guard-0", Observation, WHERE, 9_000),
            event(2, 4, "contact-21", "guard-0", Observation, WHERE, 10_000),
        ],
        &[
            bw(1, &[], "contact-21", Believed, 1, &[1]),
            superseding(bw(2, &[], "contact-21", Certain, 4, &[2]), 1, 2),
            bw(3, &["guard-0"], "contact-21", Believed, 1, &[1]),
        ],
    );
    assert_eq!(timelines(&p, 1), [(3, vec![(1, Agrees)], None)]);
}

/// A model's lifetime ends at the earliest `valid_from` of a belief that
/// supersedes it (`until`). Changes from that tick on belong to its
/// successor. Unsuperseded, it holds to the end of the package.
#[test]
fn a_superseded_model_ends_where_its_successor_begins() {
    let p = package(
        "until",
        vec![
            event(1, 1, "contact-21", "guard-0", Observation, WHERE, 9_000),
            // guard-0 restates its model on tick 3, so its link needs a cause
            // that guard-0 received by then.
            event(2, 3, "guard-0", "contact-21", Observation, CUE, 9_000),
            event(3, 4, "contact-21", "guard-0", Observation, WHERE, 0),
        ],
        &[
            bw(1, &[], "contact-21", Believed, 1, &[1]),
            superseding(bw(2, &[], "contact-21", Rejected, 4, &[3]), 1, 3),
            bw(20, &["guard-0"], "contact-21", Believed, 1, &[1]),
            superseding(bw(21, &["guard-0"], "contact-21", Believed, 3, &[2]), 20, 2),
        ],
    );
    assert_eq!(
        timelines(&p, 1),
        [
            (20, vec![(1, Agrees)], Some(3)),
            (21, vec![(3, Agrees), (4, Diverges)], None),
        ]
    );
}

/// Before ADR-0029 a successor could start before the model it supersedes
/// (30 from tick 6, 31 from tick 5). That lets two beliefs of one chain hold
/// at once (`verification/proofs/coq/KeyChain.v`, `monotone_is_needed`). ADR-0030
/// removed the rule that said so: the package is still refused, by the two
/// cause rules that bound the cause between the two starts. Whatever tick the
/// cause is on, one of them fires.
#[test]
fn a_successor_that_starts_before_its_model_is_refused_by_a_cause_rule() {
    let package_with_cause_on = |tick: u64| {
        package(
            "backwards",
            vec![
                event(1, 1, "contact-21", "guard-0", Observation, WHERE, 9_000),
                event(2, tick, "guard-0", "contact-21", Observation, CUE, 500),
            ],
            &[
                bw(1, &[], "contact-21", Believed, 1, &[1]),
                bw(30, &["guard-0"], "contact-21", Believed, 6, &[1]),
                superseding(bw(31, &["guard-0"], "contact-21", Rejected, 5, &[2]), 30, 2),
            ],
        )
    };
    for (tick, rule) in [
        (5, "supersession-cause-not-earlier"),
        (6, "supersession-cause-not-later"),
    ] {
        let p = package_with_cause_on(tick);
        let faults = validate_package(
            &p,
            KERNEL_CONTRACT_VERSION,
            &DepthPolicy { max_order: 1 },
            &status_policy(),
        )
        .expect_err("a backwards supersession is refused");
        let rules: Vec<_> = faults.iter().map(PackageFault::rule).collect();
        assert_eq!(rules, [rule], "cause on tick {tick}: {faults:?}");
        assert_eq!(faults[0].path(), "beliefs[belief_id=31].supersedes.because");
    }
}

#[test]
fn an_ambiguity_inside_the_lifetime_refuses_the_model() {
    let events = vec![
        event(1, 1, "contact-21", "guard-0", Observation, WHERE, 9_000),
        // The cause of model 6, which guard-0 received by tick 3.
        event(2, 3, "guard-0", "contact-21", Observation, CUE, 9_000),
        event(3, 4, "contact-21", "guard-0", Observation, WHERE, 0),
    ];
    let p = package(
        "ambiguous",
        events,
        &[
            bw(1, &[], "contact-21", Believed, 1, &[1]),
            // Starts on tick 4 and supersedes nothing: from tick 4 two own
            // beliefs hold.
            bw(2, &[], "contact-21", Rejected, 4, &[3]),
            // Refused: its lifetime reaches tick 4.
            bw(3, &["guard-0"], "contact-21", Believed, 1, &[1]),
            // A model with no claim needs no counterpart and is not refused.
            bw(4, &["guard-0"], "contact-21", Unknown, 1, &[1]),
            // Not refused: its lifetime ends on tick 3, before the ambiguity.
            bw(5, &["guard-0"], "contact-21", Believed, 1, &[1]),
            superseding(bw(6, &["guard-0"], "contact-21", Believed, 3, &[2]), 5, 2),
        ],
    );
    // ADR-0029: the package rules refuse it at load. Belief 2 is a second
    // start for contact-21's own key (a silent overwrite of 1), and models
    // 4 and 5 are further starts for guard-0's key, whose first start is 3.
    let load = validate_package(
        &p,
        KERNEL_CONTRACT_VERSION,
        &DepthPolicy { max_order: 1 },
        &status_policy(),
    )
    .expect_err("an unlinked overwrite is refused at load");
    let refused: Vec<_> = load.iter().map(|f| (f.rule(), f.path())).collect();
    assert_eq!(
        refused,
        [
            (
                "belief-key-chained",
                "beliefs[belief_id=2].supersedes".to_string()
            ),
            (
                "belief-key-chained",
                "beliefs[belief_id=4].supersedes".to_string()
            ),
            (
                "belief-key-chained",
                "beliefs[belief_id=5].supersedes".to_string()
            ),
        ]
    );
    // Defence in depth: handed the invalid package anyway, the reader still
    // refuses rather than pick a counterpart.
    let faults = model_timelines(&p).unwrap_err();
    // Model 6 also reaches tick 4.
    assert_eq!(faults.len(), 2, "{faults:?}");
    assert_eq!(faults[0].rule(), "model-counterpart-unique");
    assert_eq!(faults[0].path(), "beliefs[belief_id=3]");
    assert_eq!(faults[0].tick, 4);
    assert_eq!(faults[0].candidates, [BeliefId(1), BeliefId(2)]);
    assert_eq!(faults[1].path(), "beliefs[belief_id=6]");
    assert_eq!(MODEL_RULES, ["model-counterpart-unique"]);
}

// ------------------------------------------------ ADR-0029, the Rust side

/// The documented reader rule (CAC-KERNEL §9): a belief holds at `t` when it
/// starts by `t` and nothing that supersedes it has started by `t`.
fn in_force(package: &EpistemicPackage, belief: &Belief, t: u64) -> bool {
    belief.valid_from <= t
        && !package.beliefs.iter().any(|b| {
            b.supersedes
                .is_some_and(|link| link.belief == belief.belief_id)
                && b.valid_from <= t
        })
}

/// One planted break of a chain, or none.
#[derive(Debug, Clone, Copy)]
enum Break {
    Unchanged,
    /// The second belief drops its link: a silent overwrite.
    Unlink,
    /// The third belief replaces the first, which the second replaces.
    Fork,
    /// The last link names an event the package does not have.
    CauseAbsent,
    /// The last link's cause reached guard-0, not contact-21.
    CauseSeenByOther,
    /// The last belief does not cite its cause.
    CauseNotCited,
    /// The belief the last link replaces already rests on its cause.
    CauseStale,
    /// The last link's cause is on tick 0, before the belief it replaces.
    CauseBeforeReplaced,
    /// The last link's cause is after the successor starts.
    CauseAfterSuccessor,
}

const BREAKS: [Break; 9] = [
    Break::Unchanged,
    Break::Unlink,
    Break::Fork,
    Break::CauseAbsent,
    Break::CauseSeenByOther,
    Break::CauseNotCited,
    Break::CauseStale,
    Break::CauseBeforeReplaced,
    Break::CauseAfterSuccessor,
];

/// The one rule each break trips, `None` for an unbroken chain.
fn rule_of(brk: Break) -> Option<&'static str> {
    match brk {
        Break::Unchanged => None,
        Break::Unlink | Break::Fork => Some("belief-key-chained"),
        Break::CauseAbsent => Some("supersession-cause-present"),
        Break::CauseSeenByOther => Some("supersession-cause-observed"),
        Break::CauseNotCited => Some("supersession-cause-cited"),
        Break::CauseStale => Some("supersession-cause-new"),
        Break::CauseBeforeReplaced => Some("supersession-cause-not-earlier"),
        Break::CauseAfterSuccessor => Some("supersession-cause-not-later"),
    }
}

/// contact-21's own beliefs about `WHERE` as one chain (ids 1..=k, each
/// superseding the one before, `valid_from` non-decreasing), plus guard-0's
/// model of it (id 100), with one planted break or none.
///
/// Belief `i` rests on event `i`, contact-21's, and names it as the cause of
/// its link, so a link is lawful when the event is on a tick from the
/// replaced belief's start to its own (`cause_at_start` picks which end).
/// Events 900 (tick 0) and 901 (after every start) are the causes that the
/// two timing breaks name.
fn chain_case(
    steps: &[u64],
    believed: &[bool],
    cause_at_start: &[bool],
    model_from: u64,
    brk: Break,
) -> EpistemicPackage {
    let mut from = Vec::with_capacity(steps.len());
    let mut t = 0;
    for step in steps {
        t += step;
        from.push(t);
    }
    let k = from.len();
    // `ids[i]` is `i + 1`: belief `i + 1` cites `&ids[i..=i]`.
    let ids: Vec<u64> = (1..=k as u64).collect();
    let status = |i: usize| if believed[i] { Believed } else { Rejected };
    let mut beliefs: Vec<B<'_>> = (0..k)
        .map(|i| {
            let id = i as u64 + 1;
            let b = bw(id, &[], "contact-21", status(i), from[i], &ids[i..=i]);
            if i == 0 {
                b
            } else {
                superseding(b, id - 1, id)
            }
        })
        .collect();
    // Event `id` is on the successor's start, or on the replaced belief's.
    let mut events = vec![event(
        1,
        from[0],
        "contact-21",
        "guard-0",
        Observation,
        WHERE,
        9_000,
    )];
    for i in 1..k {
        let tick = if cause_at_start[i] {
            from[i - 1]
        } else {
            from[i]
        };
        events.push(event(
            i as u64 + 1,
            tick,
            "contact-21",
            "guard-0",
            Observation,
            WHERE,
            9_000,
        ));
    }
    let last = k - 1;
    let last_id = k as u64;
    match brk {
        Break::Unchanged => {}
        Break::Unlink => beliefs[1].supersedes = None,
        Break::Fork => beliefs[2].supersedes = Some((1, 3)),
        Break::CauseAbsent => beliefs[last].supersedes = Some((last_id - 1, 999)),
        Break::CauseSeenByOther => events[last].observer = "guard-0".into(),
        Break::CauseNotCited => beliefs[last].sources = &ids[..1],
        Break::CauseStale => beliefs[last - 1].sources = &ids[last - 1..=last],
        Break::CauseBeforeReplaced => {
            events.insert(
                0,
                event(900, 0, "contact-21", "guard-0", Observation, WHERE, 9_000),
            );
            beliefs[last].supersedes = Some((last_id - 1, 900));
            beliefs[last].sources = &[900];
        }
        Break::CauseAfterSuccessor => {
            events.push(event(
                901,
                from[last] + 5,
                "contact-21",
                "guard-0",
                Observation,
                WHERE,
                9_000,
            ));
            beliefs[last].supersedes = Some((last_id - 1, 901));
            beliefs[last].sources = &[901];
        }
    }
    beliefs.push(bw(
        100,
        &["guard-0"],
        "contact-21",
        Believed,
        model_from,
        &[1],
    ));
    package("adr-0029-property", events, &beliefs)
}

fn load(p: &EpistemicPackage) -> Result<(), Vec<PackageFault>> {
    validate_package(
        p,
        KERNEL_CONTRACT_VERSION,
        &DepthPolicy { max_order: 1 },
        &status_policy(),
    )
}

/// The rules of the six cause rules that a load reports.
fn cause_rules(p: &EpistemicPackage) -> Vec<&'static str> {
    load(p).err().map_or_else(Vec::new, |faults| {
        faults
            .iter()
            .map(PackageFault::rule)
            .filter(|rule| rule.starts_with("supersession-cause-"))
            .collect()
    })
}

/// No successor starts before the belief it replaces.
fn successors_never_start_early(p: &EpistemicPackage) -> bool {
    p.beliefs.iter().all(|b| {
        b.supersedes.is_none_or(|link| {
            p.beliefs
                .iter()
                .find(|old| old.belief_id == link.belief)
                .is_none_or(|old| old.valid_from <= b.valid_from)
        })
    })
}

proptest::proptest! {
    #![proptest_config(proptest::prelude::ProptestConfig::with_cases(512))]

    /// The Rust half of `verification/proofs/{coq,agda}/KeyChain`: every
    /// package the validator accepts has at most one belief of a key in
    /// force at any tick, so the real reader never meets an ambiguous
    /// counterpart, and no successor starts before the belief it replaces.
    /// Each planted break is refused with exactly its rule: the two chain
    /// rules and the six cause rules.
    #[test]
    fn one_key_one_holder_at_a_time(
        // First step from 1 so tick 0 is before every start; later steps
        // may be 0 (same-tick successors are allowed).
        first in 1u64..4,
        rest in proptest::collection::vec(0u64..4, 2..6),
        believed in proptest::collection::vec(proptest::bool::ANY, 7),
        cause_at_start in proptest::collection::vec(proptest::bool::ANY, 7),
        model_from in 0u64..20,
        which in 0usize..BREAKS.len(),
    ) {
        let mut steps = vec![first];
        steps.extend(rest);
        let brk = BREAKS[which];
        let p = chain_case(&steps, &believed, &cause_at_start, model_from, brk);
        match rule_of(brk) {
            None => {
                proptest::prop_assert_eq!(load(&p), Ok(()));
                proptest::prop_assert!(successors_never_start_early(&p));
                let end = p.beliefs.iter().map(|b| b.valid_from).max().unwrap() + 2;
                for t in 0..=end {
                    let holding = p
                        .beliefs
                        .iter()
                        .filter(|b| b.holders.is_empty() && in_force(&p, b, t))
                        .count();
                    proptest::prop_assert!(holding <= 1, "tick {}: {} in force", t, holding);
                }
                proptest::prop_assert!(model_timelines(&p).is_ok());
            }
            Some(rule) => {
                let rules: Vec<_> = load(&p).unwrap_err().iter().map(PackageFault::rule).collect();
                proptest::prop_assert_eq!(rules, [rule]);
            }
        }
    }

    /// ADR-0030: monotonicity is not a rule, it follows. For chains whose
    /// starts and cause ticks are arbitrary, a package that none of the six
    /// cause rules refuses never has a successor that starts before the
    /// belief it replaces.
    #[test]
    fn a_package_the_cause_rules_pass_has_no_early_successor(
        from in proptest::collection::vec(0u64..8, 2..5),
        ticks in proptest::collection::vec(0u64..8, 4),
    ) {
        let p = arbitrary_chain(&from, &ticks);
        if cause_rules(&p).is_empty() {
            proptest::prop_assert!(successors_never_start_early(&p), "{:?}", from);
        }
    }
}

/// Own beliefs 1..=k of contact-21 with arbitrary `valid_from`, belief `i`
/// replacing `i - 1` because of event `i`, which rests on and is cited by
/// belief `i`. Event ticks are the sorted `ticks` (cycled), so the events
/// are in order and the cause ticks are arbitrary.
fn arbitrary_chain(from: &[u64], ticks: &[u64]) -> EpistemicPackage {
    let mut sorted: Vec<u64> = (0..from.len()).map(|i| ticks[i % ticks.len()]).collect();
    sorted.sort_unstable();
    let events: Vec<_> = sorted
        .iter()
        .enumerate()
        .map(|(i, &tick)| {
            event(
                i as u64 + 1,
                tick,
                "contact-21",
                "guard-0",
                Observation,
                WHERE,
                9_000,
            )
        })
        .collect();
    let ids: Vec<u64> = (1..=from.len() as u64).collect();
    let beliefs: Vec<B<'_>> = from
        .iter()
        .enumerate()
        .map(|(i, &start)| {
            let id = i as u64 + 1;
            let b = bw(id, &[], "contact-21", Believed, start, &ids[i..=i]);
            if i == 0 {
                b
            } else {
                superseding(b, id - 1, id)
            }
        })
        .collect();
    package("adr-0030-property", events, &beliefs)
}

/// The monotone property is not vacuous: over every small chain of two
/// beliefs, some pass the cause rules and some do not.
#[test]
fn the_cause_rules_pass_some_two_chains_and_refuse_others() {
    let (mut passed, mut refused) = (0, 0);
    for first in 0..6 {
        for second in 0..6 {
            for tick in 0..6 {
                let p = arbitrary_chain(&[first, second], &[tick, tick]);
                if cause_rules(&p).is_empty() {
                    passed += 1;
                    assert!(first <= tick && tick <= second);
                    assert!(successors_never_start_early(&p));
                } else {
                    refused += 1;
                    assert!(
                        !(first <= tick && tick <= second),
                        "{first} {tick} {second}"
                    );
                }
            }
        }
    }
    assert!(
        passed > 0 && refused > 0,
        "passed {passed}, refused {refused}"
    );
}
